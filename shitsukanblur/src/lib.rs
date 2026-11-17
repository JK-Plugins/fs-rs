//! F's ShitsuKanBlur の移植。
//!
//! セル画のベタ塗り (指定した色) に、決まった向きから当たる光を足して
//! 「質感」を出す。塗りの**光が当たる側**を明るく、**影になる側**を
//! 暗くするだけで、絵の形はいじらない。
//!
//! 手順 (元の SitsuKanSub8):
//!
//! 1. 入力の色を見て印をつける: `Color0`〜`Color7` のどれかなら
//!    アルファを最大 (対象)、`EXColor0`〜`EXColor7` なら半分 (おまけ)、
//!    どちらでもなければ 0
//! 2. 印のあるところ (対象とおまけ) を緑に写して、**上下左右へ3回
//!    太らせる** (`LENGTH_MIN` 回。横に1歩・縦に1歩を3度)
//! 3. `MoveG2R`: 太らせた印を「そのまま」「`Direction` の向きへ
//!    `HilightLength` ずらしたもの」「逆へ `ShadowLength` ずらしたもの」の
//!    3つぶん、緑のビットに詰める (0x30 / 0x0C / 0x03)
//! 4. `SplitG2RGB`: 詰めたビットを解いて、赤 = 印、緑 = 光が当たる側
//!    (印はあるがずらした印が無いところ)、青 = 影の側にする
//! 5. 緑と青を別々にガウスでぼかす (`HilightBlur` / `ShadowBlur`)。
//!    印の外へは漏らさない。幅をそのままかけたあと、**1/4 の幅でもう
//!    一度**かける
//! 6. 対象でないところの緑・青を落とし、`Noise` のぶん散らす
//! 7. `Blend` で出しかたを決める
//!    - `WithOriginal`: 色を足して、対象でないところは入力のまま
//!      (`WhiteKey` が入っていれば真っ白は抜く)
//!    - `Only`: 色を足したものだけ
//!    - `MaskHilight` / `MaskShadow`: 光 (影) の重みを `MaskColor` の
//!      アルファにして出す
//!
//! 元から写した細かいところ:
//!
//! - 色あわせは RGB がぴったり同じかどうか (`compPix8`、アルファは見ない)
//! - ずらし量は `(ベクトル / 65536 + 0.5)` を切り捨てたうえで、
//!   さらに太らせたぶん (3) を足す (符号ごとに ±)
//! - 色を足すのは `LvSet`: 正なら `(最大値 - 元) × 割合`、負なら
//!   `元 × 割合` を足す。重み (緑・青) を掛けてから足す
//! - `Hilight_Lv` と `Hilight_Red/Green/Blue` は続けてかかる (2段)
//! - ぼかしの重みは `exp(-i²/(2(幅/3)²)) × 256`。印の切れ目で止める
//! - `Noise` は画素ごとに `± Noise × 最大値` を緑と青へ足す
//!   (種はフレーム番号)
//! - `OutOfScreen` が入っていると、ずらして画面の外を見たときに 0 を
//!   返す (入っていなければ縁を押さえる)
//!
//! 元と違うところ: なし (8/16/32bit すべて元から書かれている)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ShitsuKanBlur

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;

/// 太らせる回数 (元の LENGTH_MIN)。
const LENGTH_MIN: i64 = 3;

/// 1画素。深さに依らず生の目盛りで持つ。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深さごとの目盛り。
#[derive(Clone, Copy, Debug)]
pub struct Ops {
    pub max: f64,
    pub float: bool,
}

impl Ops {
    fn half(&self) -> f64 {
        if self.float {
            0.5
        } else if self.max > 255.0 {
            16384.0
        } else {
            128.0
        }
    }

    /// 元の RoundByteLong (切り捨てて 0..最大値)。
    fn clip(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 1.0)
        } else {
            v.clamp(0.0, self.max).trunc()
        }
    }

    /// 8bit の目盛りに直す (色あわせに使う)。
    fn to8(&self, p: &Px) -> (i64, i64, i64) {
        if self.float {
            let q = libs::utils::conv_32_to_8(&ae::sys::PF_PixelFloat {
                alpha: p.a as f32,
                red: p.r as f32,
                green: p.g as f32,
                blue: p.b as f32,
            });
            (q.red as i64, q.green as i64, q.blue as i64)
        } else if self.max > 255.0 {
            let q = libs::utils::conv_16_to_8(&ae::sys::PF_Pixel16 {
                alpha: p.a as u16,
                red: p.r as u16,
                green: p.g as u16,
                blue: p.b as u16,
            });
            (q.red as i64, q.green as i64, q.blue as i64)
        } else {
            (p.r as i64, p.g as i64, p.b as i64)
        }
    }
}

/// 画素の並び。
pub struct Buf {
    pub w: i64,
    pub h: i64,
    pub px: Vec<Px>,
}

impl Buf {
    pub fn new(w: i64, h: i64) -> Self {
        Buf {
            w,
            h,
            px: vec![Px::default(); (w * h).max(0) as usize],
        }
    }

    pub fn at(&self, x: i64, y: i64) -> Px {
        self.px[(x + y * self.w) as usize]
    }

    pub fn set(&mut self, x: i64, y: i64, p: Px) {
        self.px[(x + y * self.w) as usize] = p;
    }

    /// 元の GetPX8_CLIP / GetPX8_NOCLIP。
    fn get(&self, x: i64, y: i64, edge: bool) -> Px {
        if edge {
            if x < 0 || y < 0 || x > self.w - 1 || y > self.h - 1 {
                return Px::default();
            }
            self.at(x, y)
        } else {
            self.at(x.clamp(0, self.w - 1), y.clamp(0, self.h - 1))
        }
    }
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Clone, Debug)]
pub struct Info {
    /// 対象の色 (8bit の目盛り)。
    pub colour: Vec<(i64, i64, i64)>,
    /// おまけの色。
    pub ex_colour: Vec<(i64, i64, i64)>,
    pub hi_length: i64,
    pub hi_blur: i64,
    pub hi_lv: f64,
    pub hi_r: f64,
    pub hi_g: f64,
    pub hi_b: f64,
    pub sw_length: i64,
    pub sw_blur: i64,
    pub sw_lv: f64,
    pub sw_r: f64,
    pub sw_g: f64,
    pub sw_b: f64,
    pub noise: f64,
    pub edge: bool,
    pub draw: i32,
    pub mask_colour: Px,
    pub white: bool,
    /// ずらし量 (画素)。
    pub hi_off: (i64, i64),
    pub sw_off: (i64, i64),
}

/// 元の compPix8。RGB がぴったり同じか。
fn same(list: &[(i64, i64, i64)], p: &Px, ops: &Ops) -> bool {
    let (r, g, b) = ops.to8(p);
    list.iter().any(|c| c.0 == r && c.1 == g && c.2 == b)
}

/// 元の GetTargetPixels。印をアルファに入れる。対象の数を返す。
fn get_target(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) -> i64 {
    let mut count = 0;
    for i in 0..out.px.len() {
        let p = inp.px[i];
        let a = if same(&info.colour, &p, ops) {
            count += 1;
            ops.max
        } else if same(&info.ex_colour, &p, ops) {
            ops.half()
        } else {
            0.0
        };
        out.px[i] = Px {
            a,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
    }
    count
}

/// 元の MaxHorG2R / MaxVurG2R。隣に印があれば赤を立てる (太らせる)。
fn dilate(out: &mut Buf, vert: bool, ops: &Ops) {
    let (w, h) = (out.w, out.h);
    for y in 0..h {
        for x in 0..w {
            let mut b = out.at(x, y).g != 0.0;
            if !b {
                let (dx, dy) = if vert { (0, 1) } else { (1, 0) };
                if (if vert { y } else { x }) > 0 && out.at(x - dx, y - dy).g > 0.0 {
                    b = true;
                }
                if !b
                    && (if vert { y < h - 1 } else { x < w - 1 })
                    && out.at(x + dx, y + dy).g > 0.0
                {
                    b = true;
                }
            }
            let mut p = out.at(x, y);
            p.r = if b { ops.max } else { 0.0 };
            out.set(x, y, p);
        }
    }
}

/// 元の RedToGreen。
fn red_to_green(out: &mut Buf) {
    out.px.iter_mut().for_each(|p| p.g = p.r);
}

/// 元の MoveG2R。3つの印をビットに詰める。
fn move_g2r(out: &mut Buf, info: &Info, ops: &Ops) {
    let (w, h) = (out.w, out.h);
    let mut red = vec![0.0f64; out.px.len()];
    for y in 0..h {
        for x in 0..w {
            let own = out.at(x, y).g as i64;
            let p1 = out.get(x + info.hi_off.0, y + info.hi_off.1, info.edge).g as i64;
            let p2 = out.get(x - info.sw_off.0, y - info.sw_off.1, info.edge).g as i64;
            let r = (own & 0x30) | (p1 & 0x0C) | (p2 & 0x03);
            red[(x + y * w) as usize] = ops.clip(r as f64);
        }
    }
    for i in 0..out.px.len() {
        out.px[i].r = red[i];
    }
}

/// 元の SplitG2RGB。詰めたビットを赤・緑・青へ解く。
fn split_g2rgb(out: &mut Buf, ops: &Ops) {
    for p in out.px.iter_mut() {
        let g = p.g as i64;
        let moto = (g & 0x30) >> 4;
        let up = (g & 0x0C) >> 2;
        let down = g & 0x03;
        p.r = if moto > 0 { ops.max } else { 0.0 };
        p.g = if moto - up > 0 { ops.max } else { 0.0 };
        p.b = if moto - down > 0 { ops.max } else { 0.0 };
    }
}

/// 元の SwapGB。
fn swap_gb(out: &mut Buf) {
    out.px.iter_mut().for_each(|p| std::mem::swap(&mut p.g, &mut p.b));
}

/// 元の makeTbl。ガウスの重み。
fn make_tbl(range: i64) -> Vec<f64> {
    let zone = range as f64 / 3.0;
    (0..=range)
        .map(|i| ((-(i * i) as f64 / (2.0 * zone * zone)).exp() * 256.0) as i64 as f64)
        .collect()
}

/// 元の horBlurG / vurBlurG。緑だけを、印 (赤) の中でぼかす。
fn blur_g(out: &mut Buf, blur: i64, tbl: &[f64], vert: bool, ops: &Ops) {
    if blur <= 0 {
        return;
    }
    let (outer, inner) = if vert { (out.w, out.h) } else { (out.h, out.w) };
    for o in 0..outer {
        let line: Vec<(f64, f64)> = (0..inner)
            .map(|i| {
                let p = if vert { out.at(o, i) } else { out.at(i, o) };
                (p.g, p.r)
            })
            .collect();
        for i in 0..inner {
            let (x, y) = if vert { (o, i) } else { (i, o) };
            let mut p = out.at(x, y);
            if line[i as usize].1 != ops.max {
                p.g = 0.0;
                out.set(x, y, p);
                continue;
            }
            let mut count = tbl[0];
            let mut sg = line[i as usize].0 * tbl[0];
            for side in [-1i64, 1] {
                for k in 1..=blur {
                    let j = i + side * k;
                    if j < 0 || j >= inner {
                        break;
                    }
                    if line[j as usize].1 > 0.0 {
                        let gauss = tbl[k as usize];
                        sg += line[j as usize].0 * gauss;
                        count += gauss;
                    }
                }
            }
            if count > 0.0 {
                p.g = ops.clip(if ops.float {
                    sg / count
                } else {
                    ((sg as i64) / (count as i64)) as f64
                });
                out.set(x, y, p);
            }
        }
    }
}

/// 元の MaskedGBFromA。対象でないところの緑・青を落とす。
fn masked_gb(out: &mut Buf, ops: &Ops) {
    for p in out.px.iter_mut() {
        if p.a != ops.max {
            p.g = 0.0;
            p.b = 0.0;
        }
        p.r = 0.0;
    }
}

/// 元の Noise8。
fn noise(out: &mut Buf, info: &Info, rng: &mut XorShift, ops: &Ops) {
    let v = (info.noise * ops.max) as i64;
    for p in out.px.iter_mut() {
        let rnd = v - (v * 2 * rng.next() as i64 / XorShift::RAND_MAX as i64);
        if rnd != 0 && p.a > 0.0 {
            p.g = ops.clip(p.g + rnd as f64);
            p.b = ops.clip(p.b + rnd as f64);
        }
    }
}

/// 元の LvSet。`lv` が正なら上へ、負なら下へ寄せる。
fn lv_set(src: f64, lv: f64, alpha: f64, ops: &Ops) -> f64 {
    if lv == 0.0 || alpha == 0.0 {
        return src;
    }
    let mut r = if lv < 0.0 {
        src * lv
    } else {
        (ops.max - src) * lv
    };
    if alpha != 1.0 {
        r *= alpha;
    }
    src + r
}

/// 元の ColorSet8。光と影のぶんだけ色を動かす。
fn colour_set(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    for i in 0..out.px.len() {
        if out.px[i].a != ops.max {
            out.px[i].a = 0.0;
            continue;
        }
        let s = inp.px[i];
        let (mut r, mut g, mut b) = (s.r, s.g, s.b);
        let a = out.px[i].g / ops.max;
        if a > 0.0 {
            r = lv_set(r, info.hi_lv, a, ops);
            g = lv_set(g, info.hi_lv, a, ops);
            b = lv_set(b, info.hi_lv, a, ops);
            r = lv_set(r, info.hi_r, a, ops);
            g = lv_set(g, info.hi_g, a, ops);
            b = lv_set(b, info.hi_b, a, ops);
        }
        let a = out.px[i].b / ops.max;
        if a > 0.0 {
            r = lv_set(r, info.sw_lv, a, ops);
            g = lv_set(g, info.sw_lv, a, ops);
            b = lv_set(b, info.sw_lv, a, ops);
            r = lv_set(r, info.sw_r, a, ops);
            g = lv_set(g, info.sw_g, a, ops);
            b = lv_set(b, info.sw_b, a, ops);
        }
        // 元は +0.5 して (A_long) で切り捨てる。
        out.px[i].r = ops.clip(if ops.float { r } else { (r + 0.5).trunc() });
        out.px[i].g = ops.clip(if ops.float { g } else { (g + 0.5).trunc() });
        out.px[i].b = ops.clip(if ops.float { b } else { (b + 0.5).trunc() });
    }
}

/// 元の CopyTagetEX。対象でないところへ入力を戻す。
fn copy_target_ex(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    for i in 0..out.px.len() {
        if out.px[i].a == ops.max {
            continue;
        }
        let s = inp.px[i];
        let drop = info.white && s.r >= ops.max && s.g >= ops.max && s.b >= ops.max;
        out.px[i] = if drop { Px::default() } else { s };
    }
}

/// 元の SitsuKanSub8。手順ぜんぶ。
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, frame: i64, ops: &Ops) {
    let count = get_target(inp, out, info, ops);
    if count > 0 {
        // 印を緑へ。
        for p in out.px.iter_mut() {
            if p.a != 0.0 {
                p.g = ops.max;
            }
        }
        if info.hi_length > 0 || info.sw_length > 0 {
            for _ in 0..LENGTH_MIN {
                dilate(out, false, ops);
                red_to_green(out);
                dilate(out, true, ops);
                red_to_green(out);
            }
            move_g2r(out, info, ops);
            red_to_green(out);
            split_g2rgb(out, ops);

            if info.hi_blur > 0 {
                let b = info.hi_blur;
                let tbl = make_tbl(b);
                blur_g(out, b, &tbl, false, ops);
                blur_g(out, b, &tbl, true, ops);
                let b2 = b / 4;
                if b2 > 0 {
                    let tbl = make_tbl(b2);
                    blur_g(out, b2, &tbl, false, ops);
                    blur_g(out, b2, &tbl, true, ops);
                }
            }
            if info.sw_blur > 0 {
                let b = info.sw_blur;
                let tbl = make_tbl(b);
                swap_gb(out);
                blur_g(out, b, &tbl, false, ops);
                blur_g(out, b, &tbl, true, ops);
                let b2 = b / 4;
                if b2 > 0 {
                    let tbl = make_tbl(b2);
                    blur_g(out, b2, &tbl, false, ops);
                    blur_g(out, b2, &tbl, true, ops);
                }
                swap_gb(out);
            }
        }
        masked_gb(out, ops);
        if info.noise > 0.0 {
            let mut rng = XorShift::new(frame as u32);
            noise(out, info, &mut rng, ops);
        }
        if info.draw == 1 || info.draw == 2 {
            colour_set(inp, out, info, ops);
        }
    }
    if info.draw == 1 {
        copy_target_ex(inp, out, info, ops);
    } else if info.draw == 3 {
        for p in out.px.iter_mut() {
            let a = p.g;
            *p = info.mask_colour;
            p.a = a;
        }
    } else if info.draw == 4 {
        for p in out.px.iter_mut() {
            let a = p.b;
            *p = info.mask_colour;
            p.a = a;
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ColTopic,
    En(u8),
    Col(u8),
    ColTopicEnd,
    ExTopic,
    ExEn(u8),
    ExCol(u8),
    ExTopicEnd,
    Angle,
    HiTopic,
    HiLength,
    HiBlur,
    HiLv,
    HiRed,
    HiGreen,
    HiBlue,
    HiTopicEnd,
    SwTopic,
    SwLength,
    SwBlur,
    SwLv,
    SwRed,
    SwGreen,
    SwBlue,
    SwTopicEnd,
    Noise,
    Edge,
    Draw,
    MaskColor,
    White,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// 色8つの既定値 (元の PF_ADD_COLOR の並び)。
const COLS: [(u8, u8, u8); 8] = [
    (0xFF, 0x00, 0x00),
    (0x00, 0xFF, 0x00),
    (0x00, 0x00, 0xFF),
    (0xFF, 0xFF, 0x00),
    (0xFF, 0x00, 0x00),
    (0x00, 0xFF, 0x00),
    (0x00, 0x00, 0xFF),
    (0x00, 0xFF, 0xFF),
];

/// 元の PF_ADD_FLOAT_SLIDER (-200..200、精度1桁)。
fn pct(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    dflt: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(-200.0);
            f.set_valid_max(200.0);
            f.set_slider_min(-100.0);
            f.set_slider_max(100.0);
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
        params.add_group(
            Params::ColTopic,
            Params::ColTopicEnd,
            "TargetColors",
            false,
            |p| {
                for i in 0..8u8 {
                    p.add(
                        Params::En(i),
                        &format!("Enabled{i}"),
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    let c = COLS[i as usize];
                    p.add(
                        Params::Col(i),
                        &format!("Color{i}"),
                        ae::ColorDef::setup(move |f| {
                            f.set_default(ae::Pixel8 {
                                alpha: 0xFF,
                                red: c.0,
                                green: c.1,
                                blue: c.2,
                            });
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;
        params.add_group(
            Params::ExTopic,
            Params::ExTopicEnd,
            "TargetEXColors",
            false,
            |p| {
                for i in 0..8u8 {
                    p.add(
                        Params::ExEn(i),
                        &format!("EnabledEX{i}"),
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    let c = COLS[i as usize];
                    p.add(
                        Params::ExCol(i),
                        &format!("EXColor{i}"),
                        ae::ColorDef::setup(move |f| {
                            f.set_default(ae::Pixel8 {
                                alpha: 0xFF,
                                red: c.0,
                                green: c.1,
                                blue: c.2,
                            });
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;
        // 元: PF_ADD_ANGLE("Direction", 0)
        params.add(
            Params::Angle,
            "Direction",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        params.add_group(
            Params::HiTopic,
            Params::HiTopicEnd,
            "Hilight",
            false,
            |p| {
                // 元: PF_ADD_FIXED("HilightLength", 0, 200, 0, 50, 10)
                p.add(
                    Params::HiLength,
                    "HilightLength",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(200.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(50.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(10.0);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_SLIDER("HilightBlur", 0, 300, 0, 100, 20)
                p.add(
                    Params::HiBlur,
                    "HilightBlur",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(300);
                        f.set_slider_min(0);
                        f.set_slider_max(100);
                        f.set_default(20);
                        f.set_value(f.default());
                    }),
                )?;
                pct(p, Params::HiLv, "Hilight_Lv(%)", 50.0)?;
                pct(p, Params::HiRed, "Hilight_Red(%)", 0.0)?;
                pct(p, Params::HiGreen, "Hilight_Green(%)", 0.0)?;
                pct(p, Params::HiBlue, "Hilight_Blue(%)", 0.0)?;
                Ok(())
            },
        )?;
        params.add_group(
            Params::SwTopic,
            Params::SwTopicEnd,
            "Shadow",
            false,
            |p| {
                // 元: PF_ADD_FIXED("ShadowLength", 0, 200, 0, 100, 10)
                p.add(
                    Params::SwLength,
                    "ShadowLength",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(200.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(10.0);
                        f.set_value(f.default());
                    }),
                )?;
                p.add(
                    Params::SwBlur,
                    "ShadowBlur",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(300);
                        f.set_slider_min(0);
                        f.set_slider_max(100);
                        f.set_default(20);
                        f.set_value(f.default());
                    }),
                )?;
                pct(p, Params::SwLv, "Shadow_Lv(%)", -50.0)?;
                pct(p, Params::SwRed, "Shadow_Red(%)", 0.0)?;
                pct(p, Params::SwGreen, "Shadow_Green(%)", 0.0)?;
                pct(p, Params::SwBlue, "Shadow_Blue(%)", 0.0)?;
                Ok(())
            },
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("Noise", 0, 50, 0, 5, 1)
        params.add(
            Params::Noise,
            "Noise",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(50.0);
                f.set_slider_min(0.0);
                f.set_slider_max(5.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("OutOfScreen", "EdgeRepeat", FALSE)
        params.add(
            Params::Edge,
            "OutOfScreen",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("EdgeRepeat");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP("Blend", 4, 1, "WithOriginal|Only|MaskHilight|MaskShadow")
        params.add(
            Params::Draw,
            "Blend",
            ae::PopupDef::setup(|f| {
                f.set_options(&["WithOriginal", "Only", "MaskHilight", "MaskShadow"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::MaskColor,
            "MaskColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0xFF,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::White,
            "WhiteKey",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
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
                out_data.set_return_msg("fs-rs shitsukanblur");
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
        let depth = out_layer.bit_depth();
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };
        let ds = in_data.downsample_x();
        // 元の downScaleNoClip / downScale。
        let down = |v: i64| -> i64 { v * ds.num as i64 / ds.den as i64 };

        // 元の GetParams。
        let mut colour = Vec::new();
        let mut ex_colour = Vec::new();
        for i in 0..8u8 {
            if params.get(Params::En(i))?.as_checkbox()?.value() {
                let c = params.get(Params::Col(i))?.as_color()?.value();
                colour.push((c.red as i64, c.green as i64, c.blue as i64));
            }
        }
        for i in 0..8u8 {
            if params.get(Params::ExEn(i))?.as_checkbox()?.value() {
                let c = params.get(Params::ExCol(i))?.as_color()?.value();
                ex_colour.push((c.red as i64, c.green as i64, c.blue as i64));
            }
        }
        let angle = libs::utils::flt2fix(params.get(Params::Angle)?.as_angle()?.value() as f64);
        let fixed = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(libs::utils::flt2fix(params.get(p)?.as_float_slider()?.value() as f64) as i64)
        };
        let pctv = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() as f64 / 100.0)
        };
        let hi_length = down(fixed(Params::HiLength, params)?);
        let mut hi_blur = down(params.get(Params::HiBlur)?.as_slider()?.value() as i64);
        if hi_length <= 0 {
            hi_blur = 0;
        }
        let sw_length = down(fixed(Params::SwLength, params)?);
        let mut sw_blur = down(params.get(Params::SwBlur)?.as_slider()?.value() as i64);
        if sw_length <= 0 {
            sw_blur = 0;
        }
        // 元の ToParam8。ずらし量は「ベクトル / 65536 + 0.5」を切り捨て、
        // さらに太らせたぶん (downScale(3)) を足す。
        let lm = {
            let v = LENGTH_MIN;
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        let offset = |len: i64| -> (i64, i64) {
            if len <= 0 {
                return (0, 0);
            }
            let (vx, vy) = libs::utils::rot_length(angle, len as i32);
            let mut x = (vx as f64 / 65536.0 + 0.5) as i64;
            let mut y = (vy as f64 / 65536.0 + 0.5) as i64;
            if x > 0 {
                x += lm;
            } else if x < 0 {
                x -= lm;
            }
            if y > 0 {
                y += lm;
            } else if y < 0 {
                y -= lm;
            }
            (x, y)
        };
        let mask8 = params.get(Params::MaskColor)?.as_color()?.value();
        let mask_colour = match depth {
            8 => Px {
                a: mask8.alpha as f64,
                r: mask8.red as f64,
                g: mask8.green as f64,
                b: mask8.blue as f64,
            },
            16 => {
                let c = libs::utils::conv_8_to_16(&mask8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
            _ => {
                let c = libs::utils::conv_8_to_32(&mask8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
        };
        let info = Info {
            colour,
            ex_colour,
            hi_length,
            hi_blur,
            hi_lv: pctv(Params::HiLv, params)?,
            hi_r: pctv(Params::HiRed, params)?,
            hi_g: pctv(Params::HiGreen, params)?,
            hi_b: pctv(Params::HiBlue, params)?,
            sw_length,
            sw_blur,
            sw_lv: pctv(Params::SwLv, params)?,
            sw_r: pctv(Params::SwRed, params)?,
            sw_g: pctv(Params::SwGreen, params)?,
            sw_b: pctv(Params::SwBlue, params)?,
            noise: pctv(Params::Noise, params)?,
            edge: params.get(Params::Edge)?.as_checkbox()?.value(),
            draw: params.get(Params::Draw)?.as_popup()?.value(),
            mask_colour,
            white: params.get(Params::White)?.as_checkbox()?.value(),
            hi_off: offset(hi_length),
            sw_off: offset(sw_length),
        };
        // 元の ae->frame()。
        let frame = if in_data.time_step() != 0 {
            (in_data.current_time() / in_data.time_step()) as i64
        } else {
            0
        };

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
                exec(&inp, &mut out, &info, frame, &ops);
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
            colour: vec![(255, 0, 0)],
            ex_colour: vec![],
            hi_length: 10 << 16,
            hi_blur: 0,
            hi_lv: 0.5,
            hi_r: 0.0,
            hi_g: 0.0,
            hi_b: 0.0,
            sw_length: 10 << 16,
            sw_blur: 0,
            sw_lv: -0.5,
            sw_r: 0.0,
            sw_g: 0.0,
            sw_b: 0.0,
            noise: 0.0,
            edge: false,
            draw: 1,
            mask_colour: Px {
                a: 255.0,
                r: 255.0,
                g: 255.0,
                b: 255.0,
            },
            white: false,
            hi_off: (0, -13),
            sw_off: (0, -13),
        }
    }

    fn fill(buf: &mut Buf, x0: i64, y0: i64, x1: i64, y1: i64, p: Px) {
        for y in y0..y1 {
            for x in x0..x1 {
                buf.set(x, y, p);
            }
        }
    }

    fn red() -> Px {
        Px {
            a: 255.0,
            r: 255.0,
            g: 0.0,
            b: 0.0,
        }
    }

    /// 対象の色だけに印が立つ。
    #[test]
    fn only_the_target_colour_is_marked() {
        let o = ops8();
        let mut i = info();
        i.ex_colour = vec![(0, 255, 0)];
        let mut inp = Buf::new(3, 1);
        inp.px[0] = red();
        inp.px[1] = Px {
            a: 255.0,
            r: 0.0,
            g: 255.0,
            b: 0.0,
        };
        inp.px[2] = Px {
            a: 255.0,
            r: 255.0,
            g: 255.0,
            b: 255.0,
        };
        let mut out = Buf::new(3, 1);
        let n = get_target(&inp, &mut out, &i, &o);
        assert_eq!(n, 1);
        assert_eq!(out.px[0].a, 255.0, "対象");
        assert_eq!(out.px[1].a, 128.0, "おまけ");
        assert_eq!(out.px[2].a, 0.0);
    }

    /// 光の側が明るく、影の側が暗くなる。
    #[test]
    fn the_light_and_shadow_sides_move_apart() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(48, 48);
        fill(&mut inp, 8, 8, 40, 40, red());
        let mut out = Buf::new(48, 48);
        exec(&inp, &mut out, &i, 0, &o);
        // 上へずらしているので、塗りの上側が光り、下側が影になる。
        let top = out.at(24, 10).r;
        let bottom = out.at(24, 38).r;
        assert!(top > 255.0 * 0.9 || top > bottom, "{top} {bottom}");
        assert!(bottom <= 255.0);
        assert_ne!(top, bottom, "上と下で違う");
    }

    /// 対象でないところは入力のまま (`WithOriginal`)。
    #[test]
    fn the_rest_is_left_as_the_input() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(32, 32);
        fill(&mut inp, 8, 8, 24, 24, red());
        let blue = Px {
            a: 255.0,
            r: 0.0,
            g: 0.0,
            b: 255.0,
        };
        fill(&mut inp, 0, 0, 32, 4, blue);
        let mut out = Buf::new(32, 32);
        exec(&inp, &mut out, &i, 0, &o);
        assert_eq!(out.at(1, 1), blue, "対象でないところはそのまま");
    }

    /// `Only` なら対象の外は透明になる。
    #[test]
    fn the_only_mode_drops_the_rest() {
        let o = ops8();
        let mut i = info();
        i.draw = 2;
        let mut inp = Buf::new(32, 32);
        fill(&mut inp, 8, 8, 24, 24, red());
        let mut out = Buf::new(32, 32);
        exec(&inp, &mut out, &i, 0, &o);
        assert_eq!(out.at(1, 1).a, 0.0);
        assert_eq!(out.at(16, 16).a, 255.0);
    }

    /// マスクを出す回は `MaskColor` のアルファになる。
    #[test]
    fn the_mask_modes_show_the_weight() {
        let o = ops8();
        let mut inp = Buf::new(32, 32);
        fill(&mut inp, 8, 8, 24, 24, red());
        for draw in [3, 4] {
            let mut i = info();
            i.draw = draw;
            let mut out = Buf::new(32, 32);
            exec(&inp, &mut out, &i, 0, &o);
            assert!(out.px.iter().all(|p| p.r == 255.0), "色は MaskColor");
            assert!(out.px.iter().any(|p| p.a > 0.0), "どこかに重みが出る");
        }
    }

    /// ぼかすと重みがなめらかになる。
    #[test]
    fn the_blur_softens_the_weight() {
        let o = ops8();
        let mut i = info();
        i.draw = 3;
        i.hi_blur = 6;
        let mut inp = Buf::new(48, 48);
        fill(&mut inp, 8, 8, 40, 40, red());
        let mut sharp = Buf::new(48, 48);
        let mut j = i.clone();
        j.hi_blur = 0;
        exec(&inp, &mut sharp, &j, 0, &o);
        let mut soft = Buf::new(48, 48);
        exec(&inp, &mut soft, &i, 0, &o);
        let mid = |b: &Buf| (0..48).filter(|&y| b.at(24, y).a > 0 as f64 && b.at(24, y).a < 255.0).count();
        assert!(mid(&soft) > mid(&sharp), "中間の値が増える");
    }

    /// `LvSet` は正で上へ、負で下へ寄せる。
    #[test]
    fn the_level_moves_up_or_down() {
        let o = ops8();
        assert_eq!(lv_set(100.0, 0.0, 1.0, &o), 100.0);
        assert!(lv_set(100.0, 0.5, 1.0, &o) > 100.0);
        assert!(lv_set(100.0, -0.5, 1.0, &o) < 100.0);
        // 重みが小さければ動きも小さい。
        let a = lv_set(100.0, 0.5, 0.1, &o);
        let b = lv_set(100.0, 0.5, 1.0, &o);
        assert!(a < b);
    }
}
