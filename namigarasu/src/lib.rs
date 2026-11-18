//! F's NamiGarasu の移植。
//!
//! すりガラス越しに見たように絵をゆがませる。ゆがみの型紙 (ディスプレース
//! メントマップ) を、大小の丸をたくさん重ねて作るのが肝で、丸は白と黒を
//! 交互に置くので、濃淡がなめらかにつながった「波」になる。
//!
//! 手順 (元の Exec):
//!
//! 1. 出力をまんなかの灰色 (0.5) で塗る。`Noise` が入っていれば画素ごとに
//!    アルファを乱数で散らす (種はフレーム番号なので毎フレーム変わる)
//! 2. `LayerA`/`LayerB`/`LayerC` のぶんだけ繰り返す
//!    - 赤を灰色に戻し、`?_value` 個の丸を `?_size` の大きさで赤に描く。
//!      丸は白・黒の交互で、濃さは `?_strong`
//!    - `Rot` の向きへ `Speed × フレーム × ?_Speed` だけずらした赤を読んで、
//!      アルファにオーバーレイで重ねる (画面は上下左右つながっている)
//! 3. アルファを赤に写して 3×3 でぼかす、を2回
//! 4. `DispMap` が入っていれば、できた型紙をそのまま絵にして終わり
//! 5. そうでなければ型紙を使って入力をずらす。ずらす量は
//!    `(アルファ - 0.5) × X_Length / Y_Length`。`AddMap` が入っていれば
//!    型紙のぶんだけ明るさも足す
//!
//! 丸の絵柄は元の `DrawSqData.h` にある 11 枚のビットマップ
//! (10×10 から 200×200) をそのまま持ってきて `sq_data.bin` にしてある。
//! 並びは `dataSize` のとおり。
//!
//! 元から写した細かいところ:
//!
//! - `WaveValue` は「ゆがみの全体の強さ」で、`X_Length`・`Y_Length`・
//!   `AddMap`・`Noise` にまとめて掛かる
//! - 丸の大きさは `?_size` ごとの表 (`RND_TABLE`) から乱数で選ぶ。
//!   表は小さい丸と大きい丸の混ざり具合を変えてある
//! - 丸の位置は `乱数 × 幅 / 0x7FFF` で、画面の外にはみ出したぶんは
//!   反対側へ回り込む
//! - 乱数は層ごとに `?_Seed` で蒔き直す (`Draw` の中でもう一度蒔く)
//! - `Noise` が 0 でも、画素ごとに乱数を2つ空読みする (元のまま)
//! - 型紙を読むときは画面がつながっているものとして回り込み、
//!   絵を読むときは縁で押さえる
//! - ずらし量は 16 ビットの固定小数で、読むときは 8 ビット精度の双一次
//!
//! 直したバグは `libs::touchdraw::blur_to_alpha` の注を参照 (元の
//! `AlphaBlur` は TouchDrawCenter のものと一字一句同じで、四隅と右の辺が
//! ずれている)。`FAITHFUL` で元に戻る。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/NamiGarasu

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
use libs::touchdraw::{blur_to_alpha, Buf, Ops, Px};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 元の DrawSqData.h。10×10 から 200×200 までの丸11枚を順に並べたもの。
static SQ_DATA: &[u8] = include_bytes!("sq_data.bin");

/// 元の dataSize。
const SQ_SIZE: [i64; 11] = [10, 20, 40, 60, 80, 100, 120, 140, 160, 180, 200];

/// 元の rndTable。`?_size` ごとに、どの大きさの丸を出すか。
const RND_TABLE: [[usize; 8]; 6] = [
    [0, 0, 0, 1, 0, 1, 0, 1],
    [2, 3, 2, 1, 2, 1, 2, 1],
    [4, 5, 4, 3, 4, 5, 2, 3],
    [6, 7, 6, 7, 6, 5, 4, 5],
    [8, 8, 8, 9, 5, 7, 7, 6],
    [10, 10, 10, 10, 10, 9, 8, 7],
];

/// 丸1枚の始まりの位置。
fn sq_offset(si: usize) -> usize {
    SQ_SIZE[..si].iter().map(|n| (n * n) as usize).sum()
}

/// 層1つぶんのパラメータ。元の MapInfo。
#[derive(Clone, Copy, Debug, Default)]
pub struct Map {
    pub size: usize,
    pub value: i64,
    pub strong: f64,
    pub speed: f64,
    pub seed: u32,
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub length_x: i64,
    pub length_y: i64,
    pub rot: i64,
    pub speed: i64,
    pub level: f64,
    pub noise: f64,
    pub map: [Map; 3],
    pub disp_map: bool,
}

/// その深さのまんなかの値 (元の PF_HALF_CHAN8 / 16 / 32)。
fn half(ops: &Ops) -> f64 {
    if ops.float {
        0.5
    } else if ops.max > 255.0 {
        16384.0
    } else {
        128.0
    }
}

/// 元の clearHalf。まんなかの灰色で塗って、`Noise` のぶんだけ散らす。
pub fn clear_half(out: &mut Buf, info: &Info, rng: &mut XorShift, ops: &Ops) {
    let h = half(ops);
    for i in 0..out.px.len() {
        out.px[i] = Px {
            a: h,
            r: h,
            g: h,
            b: h,
        };
        if info.noise > 0.0 {
            if rng.next() > 0x3fff {
                // 元: (最大値 - 2×最大値×乱数) × Noise。32bit は 1.0 を最大値に。
                let m = if ops.float { 1.0 } else { ops.max };
                let a = (m - 2.0 * m * rng.next_double()) * info.noise;
                // 元は整数の深さだけ、足す前に (A_long) で 0 へ向けて切り捨てる。
                let a = if ops.float { a } else { a as i64 as f64 };
                out.px[i].a = if ops.float {
                    (h + a).clamp(0.0, 32.0)
                } else {
                    (h + a).clamp(0.0, ops.max)
                };
            } else {
                rng.next_double();
            }
        } else {
            // 元は Noise が 0 でも2つ空読みする。
            rng.next_double();
            rng.next_double();
        }
    }
}

/// 元の drawSq。丸1枚を赤に置く。白 (`is_w` が 0) と黒を交互に。
fn draw_sq(out: &mut Buf, x0: i64, y0: i64, si: usize, strong: f64, is_w: bool, ops: &Ops) {
    let len = SQ_SIZE[si];
    let len_h = len / 2;
    let base = sq_offset(si);
    let mut cnt = 0usize;
    let top = if ops.float { 1.0 } else { ops.max };
    for ay in 0..len {
        let mut y = (y0 + ay - len_h) % out.h;
        if y < 0 {
            y += out.h;
        } else if y >= out.h {
            y -= out.h;
        }
        for ax in 0..len {
            let mut x = (x0 + ax - len_h) % out.w;
            if x < 0 {
                x += out.w;
            } else if x >= out.w {
                x -= out.w;
            }
            let par = SQ_DATA[base + cnt] as f64 / 255.0 * strong;
            cnt += 1;
            if par <= 0.0 {
                continue;
            }
            let s = out.at(x, y).r;
            let d = if is_w { 0.0 } else { top };
            let mut v = s + (d - s) * par;
            if !ops.float {
                v = (v + 0.5).trunc();
            }
            let mut p = out.at(x, y);
            p.r = v.clamp(0.0, top);
            out.set(x, y, p);
        }
    }
}

/// 元の Draw。層1つぶんの丸をぜんぶ置く。
pub fn draw_layer(out: &mut Buf, map: &Map, ops: &Ops) {
    if map.value <= 0 || map.strong <= 0.0 {
        return;
    }
    let mut rng = XorShift::new(map.seed);
    let mut is_w = false;
    for _ in 0..map.value {
        // 元: 乱数 × 幅 / 0x7FFF (整数の掛け算と割り算)。
        let x = (rng.next() as i64 * out.w) / 0x7FFF;
        let y = (rng.next() as i64 * out.h) / 0x7FFF;
        let r = ((rng.next() as f64 * 7.0 / 0x7FFF as f64) + 0.5) as i64;
        let r = r.clamp(0, 7) as usize;
        draw_sq(out, x, y, RND_TABLE[map.size][r], map.strong, is_w, ops);
        is_w = !is_w;
    }
}

/// 元の getRed_sub。画面がつながっているものとして赤を読む。
fn red_wrap(buf: &Buf, x: i64, y: i64) -> f64 {
    let x = if x < 0 {
        x + buf.w
    } else if x >= buf.w {
        x - buf.w
    } else {
        x
    };
    let y = if y < 0 {
        y + buf.h
    } else if y >= buf.h {
        y - buf.h
    } else {
        y
    };
    buf.at(x, y).r
}

/// 元の getRed。固定小数の位置から赤を双一次で読む (重みは 8 ビット)。
fn red_at(buf: &Buf, x: i64, y: i64, ops: &Ops) -> f64 {
    let xx = x >> 16;
    let yy = y >> 16;
    let dx1 = (x & 0xFFFF) >> 8;
    let dy1 = (y & 0xFFFF) >> 8;
    let dx2 = 256 - dx1;
    let dy2 = 256 - dy1;
    if dx1 == 0 && dy1 == 0 {
        return red_wrap(buf, xx, yy);
    }
    let p = [
        red_wrap(buf, xx, yy),
        red_wrap(buf, xx + 1, yy),
        red_wrap(buf, xx, yy + 1),
        red_wrap(buf, xx + 1, yy + 1),
    ];
    if ops.float {
        let f = |d: i64| d as f64 / 256.0;
        let v = p[0] * f(dx2) * f(dy2)
            + p[1] * f(dx1) * f(dy2)
            + p[2] * f(dx2) * f(dy1)
            + p[3] * f(dx1) * f(dy1);
        v.min(1.0)
    } else {
        let v = ((p[0] as i64 * dx2 * dy2)
            + (p[1] as i64 * dx1 * dy2)
            + (p[2] as i64 * dx2 * dy1)
            + (p[3] as i64 * dx1 * dy1))
            >> 16;
        (v as f64).min(ops.max)
    }
}

/// 元の shiftRed。ずらした赤をアルファにオーバーレイで重ねる。
pub fn shift_red(out: &mut Buf, shift_x: i64, shift_y: i64, ops: &Ops) {
    let hf = half(ops);
    let mut vals = vec![0.0f64; out.px.len()];
    for y in 0..out.h {
        for x in 0..out.w {
            vals[(x + y * out.w) as usize] = if shift_x == 0 && shift_y == 0 {
                red_wrap(out, x, y)
            } else {
                red_at(out, (x << 16) - shift_x, (y << 16) - shift_y, ops)
            };
        }
    }
    for i in 0..out.px.len() {
        let ret = vals[i];
        if ret == hf {
            continue;
        }
        let mut base = out.px[i].a;
        if ops.float && base > 1.0 {
            base = 1.0;
        }
        let v = if ops.float {
            if ret < hf {
                2.0 * base * ret
            } else {
                1.0 - 2.0 * (1.0 - base) * (1.0 - ret)
            }
            .clamp(0.0, 1.0)
        } else {
            let m = ops.max as i64;
            if ret < hf {
                (((2 * base as i64 * ret as i64) / m).clamp(0, m)) as f64
            } else {
                let b = m - base as i64;
                let r = m - ret as i64;
                let r = ((2 * b * r) / m).clamp(0, m);
                ((m - r).clamp(0, m)) as f64
            }
        };
        out.px[i].a = v;
    }
}

/// 元の alphatoToRed。
pub fn alpha_to_red(out: &mut Buf) {
    for p in out.px.iter_mut() {
        p.r = p.a;
    }
}

/// 元の dispMap。型紙をそのまま絵にする。
pub fn disp_map(inp: &Buf, out: &mut Buf) {
    for i in 0..out.px.len() {
        let a = out.px[i].a;
        out.px[i].r = a;
        out.px[i].g = a;
        out.px[i].b = a;
        out.px[i].a = inp.px[i].a;
    }
}

/// 元の getP_sub。縁で押さえて1画素読む。
fn px_clamp(buf: &Buf, x: i64, y: i64) -> Px {
    buf.at(x.clamp(0, buf.w - 1), y.clamp(0, buf.h - 1))
}

/// 元の getP。固定小数の位置から双一次で読む (重みは 8 ビット)。
fn px_at(buf: &Buf, x: i64, y: i64, ops: &Ops) -> Px {
    let xx = x >> 16;
    let yy = y >> 16;
    let dx1 = (x & 0xFFFF) >> 8;
    let dy1 = (y & 0xFFFF) >> 8;
    let dx2 = 256 - dx1;
    let dy2 = 256 - dy1;
    if dx1 == 0 && dy1 == 0 {
        return px_clamp(buf, xx, yy);
    }
    let p = [
        px_clamp(buf, xx, yy),
        px_clamp(buf, xx + 1, yy),
        px_clamp(buf, xx, yy + 1),
        px_clamp(buf, xx + 1, yy + 1),
    ];
    let mix = |f: fn(&Px) -> f64| -> f64 {
        if ops.float {
            let w = |d: i64| d as f64 / 256.0;
            f(&p[0]) * w(dx2) * w(dy2)
                + f(&p[1]) * w(dx1) * w(dy2)
                + f(&p[2]) * w(dx2) * w(dy1)
                + f(&p[3]) * w(dx1) * w(dy1)
        } else {
            let v = ((f(&p[0]) as i64 * dx2 * dy2)
                + (f(&p[1]) as i64 * dx1 * dy2)
                + (f(&p[2]) as i64 * dx2 * dy1)
                + (f(&p[3]) as i64 * dx1 * dy1))
                >> 16;
            v as f64
        }
    };
    Px {
        a: ops.clip(mix(|p| p.a)),
        r: ops.clip(mix(|p| p.r)),
        g: ops.clip(mix(|p| p.g)),
        b: ops.clip(mix(|p| p.b)),
    }
}

/// 元の imageMap。型紙のぶんだけ入力をずらして読む。
pub fn image_map(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    let hf = half(ops);
    for y in 0..out.h {
        for x in 0..out.w {
            let i = (x + y * out.w) as usize;
            let shft = (out.px[i].a - hf) / hf;
            let sx = (info.length_x as f64 * shft) as i64;
            let sy = (info.length_y as f64 * shft) as i64;
            if sx == 0 && sy == 0 {
                out.px[i] = px_clamp(inp, x, y);
            } else {
                let a = ((out.px[i].a - hf) * info.level) as i64 as f64;
                let mut p = px_at(inp, (x << 16) - sx, (y << 16) - sy, ops);
                if info.level > 0.0 {
                    p.r = ops.clip(p.r + a);
                    p.g = ops.clip(p.g + a);
                    p.b = ops.clip(p.b + a);
                }
                out.px[i] = p;
            }
        }
    }
}

/// 元の Exec の早抜け。ゆがませる量が 0 か、型紙に何も出ないときは
/// 手順を通らない。`true` を返したらそこで終わり。
fn shortcut(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) -> bool {
    let no_len = info.length_x <= 0 && info.length_y <= 0;
    let flat = info.noise <= 0.0 && info.level <= 0.0;
    let no_value = info.map.iter().all(|m| m.value <= 0) && flat;
    let no_strong = info.map.iter().all(|m| m.strong <= 0.0) && flat;
    if !(no_len || no_value || no_strong) {
        return false;
    }
    if info.disp_map {
        // 元: まんなかの灰色 + アルファいっぱいで塗って、アルファだけ入力から。
        let h = half(ops);
        let top = if ops.float { 1.0 } else { ops.max };
        for i in 0..out.px.len() {
            out.px[i] = Px {
                a: top,
                r: h,
                g: h,
                b: h,
            };
            out.px[i].a = inp.px[i].a;
        }
    } else {
        out.px.copy_from_slice(&inp.px);
    }
    true
}

/// 元の Exec。手順ぜんぶ。`frame` はフレーム番号。
#[allow(clippy::too_many_arguments)]
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, frame: i64, scale: i64, ops: &Ops) {
    if shortcut(inp, out, info, ops) {
        return;
    }
    // 元: speed * frame / downSaleValue()。固定小数のまま整数で計算する。
    let len = (info.speed * frame) / scale.max(1);
    let mut rng = XorShift::new(frame as u32);
    clear_half(out, info, &mut rng, ops);
    for idx in 0..3 {
        let map = info.map[idx];
        if map.strong <= 0.0 || map.value <= 0 {
            continue;
        }
        if idx > 0 {
            // 元の clearRed。
            let h = half(ops);
            out.px.iter_mut().for_each(|p| p.r = h);
        }
        draw_layer(out, &map, ops);
        let (vx, vy) = libs::utils::rot_length(
            info.rot as i32,
            (len as f64 * map.speed) as i32,
        );
        let sx = (vx as i64) % (out.w << 16);
        let sy = (vy as i64) % (out.h << 16);
        shift_red(out, sx, sy, ops);
    }
    alpha_to_red(out);
    blur_to_alpha(out, true, ops, FAITHFUL);
    alpha_to_red(out);
    blur_to_alpha(out, true, ops, FAITHFUL);

    if info.disp_map {
        disp_map(inp, out);
    } else {
        image_map(inp, out, info, ops);
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    WaveValue,
    Rot,
    Speed,
    AddMap,
    Noise,
    ATopic,
    ASize,
    AValue,
    AStrong,
    ASpeed,
    ASeed,
    ATopicEnd,
    BTopic,
    BSize,
    BValue,
    BStrong,
    BSpeed,
    BSeed,
    BTopicEnd,
    CTopic,
    CSize,
    CValue,
    CStrong,
    CSpeed,
    CSeed,
    CTopicEnd,
    DispMap,
    LengthX,
    LengthY,
}

/// 層1つぶんのスロット。
const LAYERS: [(Params, Params, Params, Params, Params, Params, Params); 3] = [
    (
        Params::ATopic,
        Params::ASize,
        Params::AValue,
        Params::AStrong,
        Params::ASpeed,
        Params::ASeed,
        Params::ATopicEnd,
    ),
    (
        Params::BTopic,
        Params::BSize,
        Params::BValue,
        Params::BStrong,
        Params::BSpeed,
        Params::BSeed,
        Params::BTopicEnd,
    ),
    (
        Params::CTopic,
        Params::CSize,
        Params::CValue,
        Params::CStrong,
        Params::CSpeed,
        Params::CSeed,
        Params::CTopicEnd,
    ),
];

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// 元の PF_ADD_FIXED (精度1桁)。
fn fixed_slider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    vmax: f32,
    smax: f32,
    dflt: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(0.0);
            f.set_valid_max(vmax);
            f.set_slider_min(0.0);
            f.set_slider_max(smax);
            f.set_precision(1);
            f.set_curve_tolerance(0.05);
            f.set_default(dflt);
            f.set_value(f.default());
        }),
    )
}

impl AdobePluginGlobal for Plugin {
    fn can_load(_host_name: &str, _host_version: &str) -> bool {
        true
    }

    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        fixed_slider(params, Params::WaveValue, "WaveValue", 100.0, 50.0, 0.0)?;
        // 元: PF_ADD_ANGLE("Rot", 45)
        params.add(
            Params::Rot,
            "Rot",
            ae::AngleDef::setup(|f| {
                f.set_default(45.0);
                f.set_value(f.default());
            }),
        )?;
        fixed_slider(params, Params::Speed, "Speed", 32000.0, 100.0, 20.0)?;
        fixed_slider(params, Params::AddMap, "AddMap", 100.0, 100.0, 25.0)?;
        fixed_slider(params, Params::Noise, "Noise", 100.0, 100.0, 25.0)?;
        let names = ["A", "B", "C"];
        let sizes = [2, 3, 6];
        let values = [40, 20, 30];
        let strongs = [35.0f64, 50.0, 50.0];
        let speeds = [50.0f64, 75.0, 100.0];
        let seeds = [0, 1000, 2000];
        for (i, g) in LAYERS.iter().enumerate() {
            let n = names[i];
            params.add_group(g.0, g.6, &format!("Layer{n}"), false, |p| {
                // 元: PF_ADD_SLIDER("?_size", 1, 6, 1, 6, 既定)
                let sz = sizes[i];
                p.add(
                    g.1,
                    &format!("{n}_size"),
                    ae::SliderDef::setup(move |f| {
                        f.set_valid_min(1);
                        f.set_valid_max(6);
                        f.set_slider_min(1);
                        f.set_slider_max(6);
                        f.set_default(sz);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_SLIDER("?_value", 0, 200, 0, 100, 既定)
                let va = values[i];
                p.add(
                    g.2,
                    &format!("{n}_value"),
                    ae::SliderDef::setup(move |f| {
                        f.set_valid_min(0);
                        f.set_valid_max(200);
                        f.set_slider_min(0);
                        f.set_slider_max(100);
                        f.set_default(va);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_FLOAT_SLIDER("?_Strong", 0, 100, 0, 100, 既定)
                let st = strongs[i];
                p.add(
                    g.3,
                    &format!("{n}_Strong"),
                    ae::FloatSliderDef::setup(move |f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(st);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_FLOAT_SLIDER("?_Speed", -500, 500, 0, 100, 既定)
                let sp = speeds[i];
                p.add(
                    g.4,
                    &format!("{n}_Speed"),
                    ae::FloatSliderDef::setup(move |f| {
                        f.set_valid_min(-500.0);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(sp);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_SLIDER("?_Seed", 0, 30000, 0, 10000, 既定)
                let sd = seeds[i];
                p.add(
                    g.5,
                    &format!("{n}_Seed"),
                    ae::SliderDef::setup(move |f| {
                        f.set_valid_min(0);
                        f.set_valid_max(30000);
                        f.set_slider_min(0);
                        f.set_slider_max(10000);
                        f.set_default(sd);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            })?;
        }
        // 元: PF_ADD_CHECKBOX("DispMap", "on", FALSE) + CANNOT_TIME_VARY
        params.add_with_flags(
            Params::DispMap,
            "DispMap",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
            ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::NONE,
        )?;
        fixed_slider(params, Params::LengthX, "X_Length", 1000.0, 100.0, 100.0)?;
        fixed_slider(params, Params::LengthY, "Y_Length", 1000.0, 100.0, 100.0)?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg("fs-rs namigarasu");
            }
            ae::Command::GlobalSetup => {
                self.global_setup(&in_data)?;
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                if !in_data.is_premiere() {
                    return Err(Error::BadCallbackParameter);
                }
                self.do_render(&in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { extra } => {
                self.smart_pre_render(&in_data, extra)?;
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let Some(input_world) = cb.checkout_layer_pixels(0)? else {
                    return Ok(());
                };
                if let Ok(Some(output_world)) = cb.checkout_output() {
                    self.do_render(&in_data, input_world, output_world, params)?;
                }
                cb.checkin_layer_pixels(0)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn global_setup(&mut self, in_data: &InData) -> Result<(), ae::Error> {
        win_dbg_logger::DEBUGGER_LOGGER.set_force_log_without_debugger(true);
        log::info!("GlobalSetup");
        if in_data.is_premiere() {
            let suite = ae::pf::suites::PixelFormat::new()?;
            suite.clear_supported_pixel_formats(in_data.effect_ref())?;
            for x in [
                ae::pr::PixelFormat::Bgra4444_8u,
                ae::pr::PixelFormat::Bgra4444_16u,
                ae::pr::PixelFormat::Bgra4444_32f,
            ] {
                suite.add_supported_pixel_format(in_data.effect_ref(), x)?;
            }
        }
        Ok(())
    }

    fn smart_pre_render(
        &mut self,
        in_data: &InData,
        mut extra: ae::PreRenderExtra,
    ) -> Result<(), ae::Error> {
        let req = extra.output_request();
        if let Ok(in_result) = extra.callbacks().checkout_layer(
            0,
            0,
            &req,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        ) {
            let _ = extra.union_result_rect(in_result.result_rect.into());
            let _ = extra.union_max_result_rect(in_result.max_result_rect.into());
        }
        Ok(())
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let fixed = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(libs::utils::flt2fix(params.get(p)?.as_float_slider()?.value() as f64) as i64)
        };

        // 元の GetParams。WaveValue はゆがみ全体に掛かる。
        let mv = fixed(Params::WaveValue, params)? as f64 / 6553600.0;
        let length_x = (fixed(Params::LengthX, params)? as f64 * mv + 0.5) as i64;
        let length_y = (fixed(Params::LengthY, params)? as f64 * mv + 0.5) as i64;
        let level = (fixed(Params::AddMap, params)? as f64 / 6553600.0) * mv;
        let noise = (fixed(Params::Noise, params)? as f64 / 6553600.0) * mv;
        // 元の RoundAngle360。
        let rot = {
            let mut v =
                libs::utils::flt2fix(params.get(Params::Rot)?.as_angle()?.value() as f64) as i64
                    % (360 << 16);
            if v < 0 {
                v += 360 << 16;
            }
            v
        };
        let speed = fixed(Params::Speed, params)?;

        let mut map = [Map::default(); 3];
        // 元の valueTable。?_size ごとに丸の数を増やす。
        const VALUE_TABLE: [i64; 6] = [128, 32, 16, 4, 2, 1];
        for (i, g) in LAYERS.iter().enumerate() {
            let size = (params.get(g.1)?.as_slider()?.value() as i64 - 1).clamp(0, 5) as usize;
            map[i] = Map {
                size,
                value: params.get(g.2)?.as_slider()?.value() as i64 * VALUE_TABLE[size],
                strong: params.get(g.3)?.as_float_slider()?.value() as f64 / 100.0,
                speed: params.get(g.4)?.as_float_slider()?.value() as f64 / 100.0,
                seed: params.get(g.5)?.as_slider()?.value() as u32,
            };
        }
        let disp_map = params.get(Params::DispMap)?.as_checkbox()?.value();

        let info = Info {
            length_x,
            length_y,
            rot,
            speed,
            level,
            noise,
            map,
            disp_map,
        };

        let depth = out_layer.bit_depth();
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };
        // 元の ae->frame() は current_time / time_step。
        let frame = if in_data.time_step() != 0 {
            (in_data.current_time() / in_data.time_step()) as i64
        } else {
            0
        };
        // 元の downSaleValue() は den/num の整数の割り算。
        let ds = in_data.downsample_x();
        let scale = (ds.den as i64 / ds.num as i64).max(1);

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        if w == 0 || h == 0 {
            return Ok(());
        }
        let mut inp = Buf::new(w, h);
        let mut out = Buf::new(w, h);
        macro_rules! go {
            ($wr:ident, $get:expr, $put:expr) => {{
                let src = &in_layer;
                for y in 0..h {
                    for x in 0..w {
                        inp.set(x, y, $get(src.$wr(x as usize, y as usize)));
                    }
                }
                exec(&inp, &mut out, &info, frame, scale, &ops);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x as usize, y as usize), &out.at(x, y));
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                |p: &mut ae::Pixel8| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::Pixel8, s: &Px| {
                    d.alpha = s.a as i64 as u8;
                    d.red = s.r as i64 as u8;
                    d.green = s.g as i64 as u8;
                    d.blue = s.b as i64 as u8;
                }
            ),
            16 => go!(
                as_pixel16_mut,
                |p: &mut ae::Pixel16| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::Pixel16, s: &Px| {
                    d.alpha = s.a as i64 as u16;
                    d.red = s.r as i64 as u16;
                    d.green = s.g as i64 as u16;
                    d.blue = s.b as i64 as u16;
                }
            ),
            _ => go!(
                as_pixel32_mut,
                |p: &mut ae::PixelF32| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::PixelF32, s: &Px| {
                    d.alpha = s.a as f32;
                    d.red = s.r as f32;
                    d.green = s.g as f32;
                    d.blue = s.b as f32;
                }
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops8() -> Ops {
        Ops {
            max: 255.0,
            float: false,
        }
    }

    fn info() -> Info {
        Info {
            length_x: 10 << 16,
            length_y: 10 << 16,
            rot: 45 << 16,
            speed: 0,
            level: 0.0,
            noise: 0.0,
            map: [Map::default(); 3],
            disp_map: false,
        }
    }

    /// 丸の絵柄は 11 枚ぶん入っていて、どれも真ん中がいちばん濃い。
    #[test]
    fn the_blob_data_is_all_there() {
        assert_eq!(SQ_DATA.len(), SQ_SIZE.iter().map(|n| (n * n) as usize).sum::<usize>());
        for si in 0..SQ_SIZE.len() {
            let n = SQ_SIZE[si];
            let base = sq_offset(si);
            let mid = SQ_DATA[base + ((n / 2) * n + n / 2) as usize];
            let corner = SQ_DATA[base];
            assert!(mid > corner, "{si}: {mid} {corner}");
        }
    }

    /// 灰色で塗ったあと、`Noise` が 0 なら全部まんなかの値。
    #[test]
    fn the_canvas_starts_grey() {
        let o = ops8();
        let mut out = Buf::new(4, 4);
        let mut rng = XorShift::new(0);
        clear_half(&mut out, &info(), &mut rng, &o);
        assert!(out.px.iter().all(|p| p.a == 128.0 && p.r == 128.0));
    }

    /// `Noise` を入れるとアルファだけばらける。
    #[test]
    fn the_noise_only_moves_alpha() {
        let o = ops8();
        let mut i = info();
        i.noise = 0.5;
        let mut out = Buf::new(16, 16);
        let mut rng = XorShift::new(0);
        clear_half(&mut out, &i, &mut rng, &o);
        assert!(out.px.iter().any(|p| p.a != 128.0), "散る");
        assert!(out.px.iter().all(|p| p.r == 128.0), "赤は動かない");
    }

    /// 丸を1つ置くと、白と黒が交互になる。
    #[test]
    fn the_blobs_alternate_black_and_white() {
        let o = ops8();
        let mut out = Buf::new(64, 64);
        out.px.iter_mut().for_each(|p| p.r = 128.0);
        let m = Map {
            size: 0,
            value: 2,
            strong: 1.0,
            speed: 0.0,
            seed: 0,
        };
        draw_layer(&mut out, &m, &o);
        let hi = out.px.iter().filter(|p| p.r > 128.0).count();
        let lo = out.px.iter().filter(|p| p.r < 128.0).count();
        assert!(hi > 0 && lo > 0, "白 {hi} 黒 {lo}");
    }

    /// `shift_red` は灰色のところを素通りさせ、濃いところだけ効かせる。
    #[test]
    fn the_overlay_leaves_grey_alone() {
        let o = ops8();
        let mut out = Buf::new(3, 1);
        for (i, r) in [128.0, 255.0, 0.0].iter().enumerate() {
            out.px[i] = Px {
                a: 128.0,
                r: *r,
                g: 0.0,
                b: 0.0,
            };
        }
        shift_red(&mut out, 0, 0, &o);
        assert_eq!(out.px[0].a, 128.0, "灰色はそのまま");
        assert_eq!(out.px[1].a, 255.0, "白はもっと明るく");
        assert_eq!(out.px[2].a, 0.0, "黒はもっと暗く");
    }

    /// 型紙がまんなかのままなら絵は動かない。
    #[test]
    fn a_flat_map_leaves_the_image_alone() {
        let o = ops8();
        let mut inp = Buf::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                inp.set(
                    x,
                    y,
                    Px {
                        a: 255.0,
                        r: (x * 30) as f64,
                        g: (y * 30) as f64,
                        b: 0.0,
                    },
                );
            }
        }
        let mut out = Buf::new(8, 8);
        out.px.iter_mut().for_each(|p| p.a = 128.0);
        image_map(&inp, &mut out, &info(), &o);
        for i in 0..out.px.len() {
            assert_eq!(out.px[i], inp.px[i], "{i}");
        }
    }

    /// 型紙がかたよると絵がずれる。
    #[test]
    fn a_leaning_map_moves_the_image() {
        let o = ops8();
        let mut inp = Buf::new(16, 1);
        inp.set(
            2,
            0,
            Px {
                a: 255.0,
                r: 255.0,
                g: 255.0,
                b: 255.0,
            },
        );
        let mut out = Buf::new(16, 1);
        out.px.iter_mut().for_each(|p| p.a = 255.0);
        let mut i = info();
        i.length_y = 0;
        image_map(&inp, &mut out, &i, &o);
        // アルファが上いっぱいなので、絵は 10 画素ぶん右へずれる。
        assert_eq!(out.at(2, 0).r, 0.0, "元の位置はもう白くない");
        assert!(out.at(12, 0).r > 0.0, "10 画素ぶん先が白くなる");
    }
}
