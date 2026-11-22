//! F's TouchDrawCenter の移植。
//!
//! 画面の四辺の外から中心へ向かう短い線 (タッチ) をたくさん引いて、鉛筆で
//! 縁を掃いたような縁取りを描く。線は中心に向かう途中で**不透明度が
//! 落ちていく**ので、外側が濃く内側が薄い。描いたものはいちど 3×3 の
//! ぼかしを通してからアルファにして、指定色で塗る。
//!
//! 手順 (元の Exec):
//!
//! 1. キャンバスを消す。長さ・本数・不透明度のどれかが 0 以下なら、
//!    Original_Blend が入っていれば入力をコピーして終わり
//! 2. `drawTouch`: 四辺ぶんの本数を決め、一本ずつ中心へ向かって
//!    アルファを書く (同じ値を赤にも控える)
//! 3. `AlphaBlur`: 赤を 3×3 (重み 2,4,2 / 4,8,4 / 2,4,2) でぼかして
//!    アルファに書き戻す
//! 4. `CopyImage`: 指定色にそのアルファを付け、Original_Blend が
//!    入っていれば入力の上に重ねる
//!
//! 四辺の本数の決め方 (元の CalcParams) は、まず四辺とも `Value` 本に
//! してから、中心が寄っているほうの本数を減らして反対側に移す。
//! `(c/2) == (p/2)` のときは「だいたい真ん中」として動かさない。
//!
//! 一本の引き方 (元の drawTouchSub):
//!
//! - 始点から中心への差を `Length`×`Length_Random` の割合だけ進む
//! - 歩数は横と縦の差の大きいほうの整数部。不透明度は歩数ぶんで 0 まで
//!   落ちる
//! - 1歩ごとに小数の位置へ4点に分けてアルファを足す。足し方は
//!   `c0 + c1 - c0*c1/最大値` (スクリーン合成)
//!
//! 元から写した細かいところ:
//!
//! - 位置と不透明度の計算は **f32** (元の PF_FpShort)。乱数だけ double
//! - 始点は画面の外 (上なら `-height/20`、下なら `height-1 + height/20`)
//! - 横 (縦) のばらつきは `-幅/5` から `幅 + 幅/5` の範囲
//! - 1本につき `Point_Count + 1` 回引く (`j <= point_count`)
//! - `Point_Length` だけ downsample を掛ける
//! - ぼかしの重みは内側 32 で `>>5`、辺は 24、角は 18 で割る
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **左右の辺で始点のばらつきに `lr0` を2回使っている**
//!    (`yy + lr0 + F_RAND_D()*lr0`)。上下の辺は `lr0 + 乱数*lr1` で
//!    `±Point_Length` に散るのに、左右だけ `-Point_Length` から
//!    `-2*Point_Length` の範囲、つまり必ず上へ寄る。`lr1` にした
//! 2. **ぼかしの四隅のうち3つが読み書きの位置を間違えている**。右上は
//!    `data[w-2]` へ書いて `w-2, w-1` 列を読み (本当は `w` へ書いて
//!    `w-1, w` 列)、左下・右下も同じように1行ぶんずれている。画面の
//!    右上・左下・右下の角だけぼかしが掛からず、代わりに内側の画素が
//!    上書きされていた
//! 3. **ぼかしの右の辺で、下の行の2つだけ1列ぶん内側を読んでいる**
//!    (`adr2 += (wt - 2)` が `wt - 1` のつもり)。右端の列のぼかしが
//!    1画素ぶん左に傾く
//! 4. **ぴったり格子に乗ったときの書き込みに範囲の確かめがない**
//!    (`xx2 == 0 && yy2 == 0` の枝)。始点は画面の外から始まるので、
//!    座標がちょうど整数になるとバッファの外へ書く。確かめを足した
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/TouchDrawCenter

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    seed: i64,
    center_x: f32,
    center_y: f32,
    length_max: f32,
    length_rnd: f32,
    value: i64,
    value_left: i64,
    value_right: i64,
    value_top: i64,
    value_bottom: i64,
    opacity: f32,
    opacity_rnd: f32,
    point_count: i64,
    point_length: f32,
}

/// 深度ごとの作法。元の mixCh8/16/32 と丸めのちがいをまとめたもの。
#[derive(Debug, Clone, Copy)]
struct Ops {
    /// 8bit は 255、16bit は 32768、32bit は 1.0。
    max: f64,
    float: bool,
}

impl Ops {
    /// 元の mixCh。スクリーン合成。
    fn mix(&self, c0: f64, c1: f64) -> f64 {
        let v = if self.float {
            c0 + c1 - c0 * c1
        } else {
            // 元: 8bit は c0 + c1 - (c0 * c1 >> 8)、16bit は >> 15。
            // 8bit だけ 256 (= MAX+1)、16bit は 32768 (= MAX)。
            let sh = if self.max == 255.0 { 256.0 } else { 32768.0 };
            c0 + c1 - (c0 * c1 / sh).floor()
        };
        v.clamp(0.0, self.max)
    }

    /// 元の `v = (A_long)(PF_MAX_CHAN * opacity)` / 32bit は不透明度そのまま。
    fn scale(&self, opacity: f32) -> f64 {
        if self.float {
            opacity as f64
        } else {
            (self.max * opacity as f64).trunc()
        }
    }

    /// 4点に分けるときの重みを掛けたあとの丸め。
    fn part(&self, v: f64) -> f64 {
        if self.float {
            v
        } else {
            v.trunc()
        }
    }

    /// 元の RoundByteLong / RoundShort / RoundFpShortDouble。
    fn round(&self, v: f64) -> f64 {
        let hi = if self.float { 32.0 } else { self.max };
        v.clamp(0.0, hi)
    }

    /// ぼかしの割り算。8/16bit は整数、32bit は f64。
    fn div(&self, v: f64, by: f64) -> f64 {
        if self.float {
            v / by
        } else {
            (v / by).trunc()
        }
    }
}

/// 描く下書き。元は出力の赤とアルファを使い回している。
struct Mask {
    w: usize,
    h: usize,
    /// (赤, アルファ)
    px: Vec<(f64, f64)>,
}

impl Mask {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![(0.0, 0.0); w * h],
        }
    }
}

/// 1歩ぶんの書き込み。元の PsetAlpha。
fn pset(m: &mut Mask, x: f32, y: f32, opacity: f32, ops: &Ops) {
    let mut opacity = opacity;
    if opacity <= 0.0 {
        return;
    }
    if opacity > 1.0 {
        opacity = 1.0;
    }
    let (w, h) = (m.w as i64, m.h as i64);
    // 元: (A_long) なので 0 方向への切り捨て。
    let x1 = x as i64;
    let y1 = y as i64;
    let (x2, y2) = (x1 + 1, y1 + 1);
    if x1 >= w || y1 >= h || x2 < 0 || y2 < 0 {
        return;
    }
    let xx2 = x as f64 - x1 as f64;
    let yy2 = y as f64 - y1 as f64;
    let xx1 = 1.0 - xx2;
    let yy1 = 1.0 - yy2;
    let v = ops.scale(opacity);

    let mut put = |px: i64, py: i64, weight: f64| {
        let i = (px + py * w) as usize;
        let a = ops.mix(m.px[i].1, ops.part(v * weight));
        m.px[i] = (a, a);
    };
    if xx2 == 0.0 && yy2 == 0.0 {
        // 元はここだけ範囲を確かめていない (直したところ 4)。
        if FAITHFUL || (x1 >= 0 && y1 >= 0) {
            put(x1, y1, 1.0);
        }
        return;
    }
    if x1 >= 0 && y1 >= 0 {
        put(x1, y1, xx1 * yy1);
    }
    if x2 < w && y1 >= 0 {
        put(x2, y1, xx2 * yy1);
    }
    if x2 < w && y2 < h {
        put(x2, y2, xx2 * yy2);
    }
    if x1 >= 0 && y2 < h {
        put(x1, y2, xx1 * yy2);
    }
}

/// 1本ぶんの線。元の drawTouchSub。
fn draw_touch_sub(
    m: &mut Mask,
    start_x: f32,
    start_y: f32,
    center_x: f32,
    center_y: f32,
    length: f32,
    opacity: f32,
    ops: &Ops,
) {
    if opacity <= 0.0 {
        return;
    }
    let mut rnd = length;
    if rnd <= 0.0 {
        return;
    } else if rnd > 1.0 {
        rnd = 1.0;
    }
    let mut x = center_x - start_x;
    let mut x_f = 1.0f32;
    if x < 0.0 {
        x_f = -1.0;
        x = -x;
    }
    let mut y = center_y - start_y;
    let mut y_f = 1.0f32;
    if y < 0.0 {
        y_f = -1.0;
        y = -y;
    }
    x *= rnd;
    y *= rnd;
    if x == 0.0 && y == 0.0 {
        return;
    }
    let len = if x >= y { x as i64 } else { y as i64 };
    if len <= 0 {
        // 元も for の回数が 0 になって何も書かない。
        return;
    }
    let dx = x / len as f32 * x_f;
    let dy = y / len as f32 * y_f;
    let dl = opacity / len as f32;
    let (mut px, mut py, mut po) = (start_x, start_y, opacity);
    for _ in 0..len {
        pset(m, px, py, po, ops);
        px += dx;
        py += dy;
        po -= dl;
    }
}

/// 元の drawTouch。四辺ぶんを順に引く。
fn draw_touch(m: &mut Mask, info: &Info, ops: &Ops) {
    let mut rng = XorShift::new(info.seed as u32);
    let (w, h) = (m.w as i64, m.h as i64);

    // 不透明度のばらつき。
    let p0 = info.opacity * (1.0 - info.opacity_rnd);
    let p1 = info.opacity - p0;
    // 長さのばらつき。
    let r0 = info.length_max * (1.0 - info.length_rnd);
    let r1 = info.length_max - r0;
    // 横・縦のばらつき。
    let w0 = w as f32 / -5.0;
    let w1 = w as f32 + (w as f32 / 5.0) - w0;
    let h0 = h as f32 / -5.0;
    let h1 = h as f32 + (h as f32 / 5.0) - h0;
    // 始点の散らし。元は左右の辺で lr1 ではなく lr0 を使っていた
    // (直したところ 1)。
    let lr0 = -info.point_length;
    let lr1 = info.point_length * 2.0;
    let lr_side = if FAITHFUL { lr0 } else { lr1 };

    let mut rnd = |a: f32, b: f32, rng: &mut XorShift| -> f32 {
        (a as f64 + rng.next_double() * b as f64) as f32
    };

    // 上。
    if info.value_top > 0 {
        let start_y = h as f32 / -20.0;
        for _ in 0..info.value_top {
            let xx = rnd(w0, w1, &mut rng);
            for _ in 0..=info.point_count {
                let opacity = rnd(p0, p1, &mut rng);
                let length = rnd(r0, r1, &mut rng);
                let start_x = rnd(xx + lr0, lr1, &mut rng);
                draw_touch_sub(
                    m,
                    start_x,
                    start_y,
                    info.center_x,
                    info.center_y,
                    length,
                    opacity,
                    ops,
                );
            }
        }
    }
    // 下。
    if info.value_bottom > 0 {
        let start_y = h as f32 - 1.0 + h as f32 / 20.0;
        for _ in 0..info.value_bottom {
            let xx = rnd(w0, w1, &mut rng);
            for _ in 0..=info.point_count {
                let opacity = rnd(p0, p1, &mut rng);
                let length = rnd(r0, r1, &mut rng);
                let start_x = rnd(xx + lr0, lr1, &mut rng);
                draw_touch_sub(
                    m,
                    start_x,
                    start_y,
                    info.center_x,
                    info.center_y,
                    length,
                    opacity,
                    ops,
                );
            }
        }
    }
    // 左。
    if info.value_left > 0 {
        let start_x = w as f32 / -20.0;
        for _ in 0..info.value_left {
            let yy = rnd(h0, h1, &mut rng);
            for _ in 0..=info.point_count {
                let opacity = rnd(p0, p1, &mut rng);
                let length = rnd(r0, r1, &mut rng);
                let start_y = rnd(yy + lr0, lr_side, &mut rng);
                draw_touch_sub(
                    m,
                    start_x,
                    start_y,
                    info.center_x,
                    info.center_y,
                    length,
                    opacity,
                    ops,
                );
            }
        }
    }
    // 右。
    if info.value_right > 0 {
        let start_x = w as f32 - 1.0 + w as f32 / 20.0;
        for _ in 0..info.value_right {
            let yy = rnd(h0, h1, &mut rng);
            for _ in 0..=info.point_count {
                let opacity = rnd(p0, p1, &mut rng);
                let length = rnd(r0, r1, &mut rng);
                let start_y = rnd(yy + lr0, lr_side, &mut rng);
                draw_touch_sub(
                    m,
                    start_x,
                    start_y,
                    info.center_x,
                    info.center_y,
                    length,
                    opacity,
                    ops,
                );
            }
        }
    }
}

/// 元の AlphaBlur。赤を 3×3 でぼかしてアルファに書く。
///
/// 元は内側・辺・角を別々に書いていて、角のうち3つの読み書きの位置が
/// ずれている (直したところ 2)。
fn alpha_blur(m: &mut Mask, ops: &Ops) {
    let (iw, ih) = (m.w as i64, m.h as i64);
    if iw < 2 || ih < 2 {
        return;
    }
    let w = iw - 1;
    let h = ih - 1;
    let red = |x: i64, y: i64| m.px[(x + y * iw) as usize].0;

    // 内側。重みの合計は 32。
    let mut out: Vec<(i64, f64)> = Vec::new();
    for y in 1..h {
        for x in 1..w {
            let v = red(x - 1, y - 1) * 2.0
                + red(x, y - 1) * 4.0
                + red(x + 1, y - 1) * 2.0
                + red(x - 1, y) * 4.0
                + red(x, y) * 8.0
                + red(x + 1, y) * 4.0
                + red(x - 1, y + 1) * 2.0
                + red(x, y + 1) * 4.0
                + red(x + 1, y + 1) * 2.0;
            out.push((x + y * iw, ops.round(ops.div(v, 32.0))));
        }
    }
    // 上下の辺。重みの合計は 24。
    for x in 1..w {
        let v = red(x - 1, 0) * 4.0
            + red(x, 0) * 8.0
            + red(x + 1, 0) * 4.0
            + red(x - 1, 1) * 2.0
            + red(x, 1) * 4.0
            + red(x + 1, 1) * 2.0;
        out.push((x, ops.round(ops.div(v, 24.0))));
        let v = red(x - 1, h) * 4.0
            + red(x, h) * 8.0
            + red(x + 1, h) * 4.0
            + red(x - 1, h - 1) * 2.0
            + red(x, h - 1) * 4.0
            + red(x + 1, h - 1) * 2.0;
        out.push((x + h * iw, ops.round(ops.div(v, 24.0))));
    }
    // 左右の辺。
    for y in 1..h {
        let v = red(0, y - 1) * 4.0
            + red(1, y - 1) * 2.0
            + red(0, y) * 8.0
            + red(1, y) * 4.0
            + red(0, y + 1) * 4.0
            + red(1, y + 1) * 2.0;
        out.push((y * iw, ops.round(ops.div(v, 24.0))));
        // 元は下の2つだけ1列ぶん内側を読んでいる (直したところ 4)。
        let (lo, hi) = if FAITHFUL { (w - 2, w - 1) } else { (w - 1, w) };
        let v = red(w - 1, y - 1) * 2.0
            + red(w, y - 1) * 4.0
            + red(w - 1, y) * 4.0
            + red(w, y) * 8.0
            + red(lo, y + 1) * 2.0
            + red(hi, y + 1) * 4.0;
        out.push((w + y * iw, ops.round(ops.div(v, 24.0))));
    }
    // 四隅。重みの合計は 18。元は右上・左下・右下の読み書きがずれていた。
    let corner = |ax: i64, ay: i64, bx: i64, by: i64| -> f64 {
        // (ax,ay) が角。角 8、横隣 4、縦隣 4、斜め 2。
        let v = red(ax, ay) * 8.0 + red(bx, ay) * 4.0 + red(ax, by) * 4.0 + red(bx, by) * 2.0;
        ops.round(ops.div(v, 18.0))
    };
    let (tr_w, bl_w, br_w) = if FAITHFUL {
        // 元のずれをそのまま: 書き先も読み先も内側へ寄る。
        let f = |ax: i64, ay: i64, bx: i64, by: i64, wa: f64, wb: f64, wc: f64, wd: f64| -> f64 {
            let v =
                red(ax, ay) * wa + red(bx, ay) * wb + red(ax, by) * wc + red(bx, by) * wd;
            ops.round(ops.div(v, 18.0))
        };
        (
            ((w - 2) + 0 * iw, f(w - 2, 0, w - 1, 1, 4.0, 8.0, 2.0, 4.0)),
            ((h - 1) * iw, f(0, h - 2, 1, h - 1, 4.0, 2.0, 8.0, 4.0)),
            (
                (h - 1) * iw + (w - 1),
                f(w - 1, h - 2, w, h - 1, 2.0, 4.0, 4.0, 8.0),
            ),
        )
    } else {
        (
            (w, corner(w, 0, w - 1, 1)),
            (h * iw, corner(0, h, 1, h - 1)),
            (h * iw + w, corner(w, h, w - 1, h - 1)),
        )
    };
    out.push((0, corner(0, 0, 1, 1)));
    out.push(tr_w);
    out.push(bl_w);
    out.push(br_w);

    for (i, v) in out {
        m.px[i as usize].1 = v;
    }
}

/// 元の MixPx。入力の上に下書きを重ねる。
fn mix_px(s: (f64, f64, f64, f64), d: (f64, f64, f64, f64), ops: &Ops) -> (f64, f64, f64, f64) {
    let (sa, sr, sg, sb) = s;
    let (da, dr, dg, db) = d;
    if sa == 0.0 || da == ops.max {
        return d;
    }
    if da == 0.0 {
        return s;
    }
    // 元: 8bit は ((MAX+1) - da) * sa >> 8、16bit は (MAX - da) * sa >> 15、
    // 32bit は (1 - da) * sa。
    let as2 = if ops.float {
        (1.0 - da) * sa
    } else if ops.max == 255.0 {
        (((ops.max + 1.0) - da) * sa / 256.0).floor()
    } else {
        ((ops.max - da) * sa / 32768.0).floor()
    };
    let mut ad2 = da + as2;
    if ad2 > ops.max {
        ad2 = ops.max;
    }
    if ad2 <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let f = |dc: f64, sc: f64| {
        let v = dc * da + sc * as2;
        ops.round(ops.div(v, ad2))
    };
    (ad2, f(dr, sr), f(dg, sg), f(db, sb))
}

/// 元の CalcParams。四辺の本数を中心の寄りで振り分ける。
fn calc_counts(info: &mut Info, w: i64, h: i64) {
    info.value_left = info.value;
    info.value_right = info.value;
    info.value_top = info.value;
    info.value_bottom = info.value;
    if info.value <= 0 {
        return;
    }
    // 上下。
    let c = h / 2;
    let p = info.center_y as i64;
    if c / 2 == p / 2 {
        // 元: だいたい同じなら動かさない。
    } else if p <= 0 {
        info.value_bottom += info.value_top;
        info.value_top = 0;
    } else if p >= h - 1 {
        info.value_top += info.value_bottom;
        info.value_bottom = 0;
    } else if p > c {
        let vv = info.value_bottom * (p - c) / (h - c);
        info.value_bottom -= vv;
        info.value_top += vv;
    } else {
        let vv = info.value_top * (c - p) / c;
        info.value_top -= vv;
        info.value_bottom += vv;
    }
    // 左右。
    let c = w / 2;
    let p = info.center_x as i64;
    if c / 2 == p / 2 {
    } else if p <= 0 {
        info.value_right += info.value_left;
        info.value_left = 0;
    } else if p >= w - 1 {
        info.value_left += info.value_right;
        info.value_right = 0;
    } else if p > c {
        let vv = info.value_right * (p - c) / (w - c);
        info.value_right -= vv;
        info.value_left += vv;
    } else {
        let vv = info.value_left * (c - p) / c;
        info.value_left -= vv;
        info.value_right += vv;
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Seed,
    Center,
    LengthMax,
    LengthRnd,
    Value,
    Color,
    Opacity,
    OpacityRnd,
    PointCount,
    PointLength,
    Org,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

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
        // 元: PF_ADD_SLIDER("RandomSeed", 0, 32000, 0, 144, 0)
        params.add(
            Params::Seed,
            "RandomSeed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(32000);
                f.set_slider_min(0);
                f.set_slider_max(144);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("Center", 50, 50)
        params.add(
            Params::Center,
            "Center",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED の並び。
        for (p, name, vmax, smax, dflt) in [
            (Params::LengthMax, "Length", 200.0, 100.0, 100.0),
            (Params::LengthRnd, "Length_Random", 200.0, 100.0, 50.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(move |f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(vmax);
                    f.set_slider_min(0.0);
                    f.set_slider_max(smax);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_SLIDER("Value", 0, 3000, 0, 100, 30)
        params.add(
            Params::Value,
            "Value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(3000);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(30);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("Color", 0, 0, 0)
        params.add(
            Params::Color,
            "Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
                f.set_value(f.default());
            }),
        )?;
        for (p, name, dflt) in [
            (Params::Opacity, "Opacity", 100.0),
            (Params::OpacityRnd, "Opacity_Random", 50.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(move |f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(200.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_SLIDER("Point_Count", 0, 100, 0, 10, 5)
        params.add(
            Params::PointCount,
            "Point_Count",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(5);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("Point_Length", 0, 300, 0, 30, 10)
        params.add(
            Params::PointLength,
            "Point_Length",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(30.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(10.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("Original_Blend", "ON", TRUE)
        params.add(
            Params::Org,
            "Original_Blend",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
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
                out_data.set_return_msg("fs-rs touchdrawcenter");
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
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let ds = in_data.downsample_x();
        // 元の GetParams。
        let seed = params.get(Params::Seed)?.as_slider()?.value().max(0) as i64;
        let (cx, cy) = params.get(Params::Center)?.as_point()?.value();
        // 元: PF_FixedPoint を 1<<16 で割る。
        let center_x = (libs::utils::flt2fix(cx as f64) as f64 / 65536.0) as f32;
        let center_y = (libs::utils::flt2fix(cy as f64) as f64 / 65536.0) as f32;
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f32, Error> {
            Ok((params.get(p)?.as_float_slider()?.value() as f32 / 100.0).max(0.0))
        };
        let mut info = Info {
            seed,
            center_x,
            center_y,
            length_max: pct(Params::LengthMax, params)?,
            length_rnd: pct(Params::LengthRnd, params)?,
            value: params.get(Params::Value)?.as_slider()?.value().max(0) as i64,
            value_left: 0,
            value_right: 0,
            value_top: 0,
            value_bottom: 0,
            opacity: pct(Params::Opacity, params)?,
            opacity_rnd: pct(Params::OpacityRnd, params)?,
            point_count: params.get(Params::PointCount)?.as_slider()?.value().max(0) as i64,
            // 元: Point_Length は % で割らず、downsample だけ掛ける。
            point_length: (params.get(Params::PointLength)?.as_float_slider()?.value() as f32
                * ds.num as f32
                / ds.den as f32)
                .max(0.0),
        };
        let colour = params.get(Params::Color)?.as_color()?.value();
        let org = params.get(Params::Org)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }
        calc_counts(&mut info, w as i64, h as i64);

        // 元の Exec: 長さ・本数・不透明度のどれかが 0 以下なら描かない。
        if info.length_max <= 0.0 || info.value <= 0 || info.opacity <= 0.0 {
            if org {
                ae::pf::suites::WorldTransform::new()?.copy_hq(
                    in_data.effect_ref(),
                    &in_layer,
                    &mut out_layer,
                    None,
                    None,
                )?;
            } else {
                macro_rules! clear {
                    ($wr:ident, $zero:expr) => {
                        for y in 0..h {
                            for x in 0..w {
                                *out_layer.$wr(x, y) = $zero;
                            }
                        }
                    };
                }
                match depth {
                    8 => clear!(
                        as_pixel8_mut,
                        ae::Pixel8 {
                            alpha: 0,
                            red: 0,
                            green: 0,
                            blue: 0
                        }
                    ),
                    16 => clear!(
                        as_pixel16_mut,
                        ae::Pixel16 {
                            alpha: 0,
                            red: 0,
                            green: 0,
                            blue: 0
                        }
                    ),
                    _ => clear!(
                        as_pixel32_mut,
                        ae::PixelF32 {
                            alpha: 0.0,
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0
                        }
                    ),
                }
            }
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $col:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                let mut m = Mask::new(w, h);
                draw_touch(&mut m, &info, &ops);
                alpha_blur(&mut m, &ops);
                let c = $col;
                for y in 0..h {
                    for x in 0..w {
                        let d = (m.px[x + y * w].1, c.0, c.1, c.2);
                        let d = if org {
                            mix_px($get(in_layer.$rd(x, y)), d, &ops)
                        } else {
                            d
                        };
                        $put(out_layer.$wr(x, y), d);
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                Ops {
                    max: 255.0,
                    float: false
                },
                (colour.red as f64, colour.green as f64, colour.blue as f64),
                |p: &ae::Pixel8| (
                    p.alpha as f64,
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64
                ),
                |p: &mut ae::Pixel8, d: (f64, f64, f64, f64)| {
                    p.alpha = d.0 as i64 as u8;
                    p.red = d.1 as i64 as u8;
                    p.green = d.2 as i64 as u8;
                    p.blue = d.3 as i64 as u8;
                }
            ),
            16 => {
                let c16 = libs::utils::conv_8_to_16(&colour);
                go!(
                    as_pixel16,
                    as_pixel16_mut,
                    Ops {
                        max: 32768.0,
                        float: false
                    },
                    (c16.red as f64, c16.green as f64, c16.blue as f64),
                    |p: &ae::Pixel16| (
                        p.alpha as f64,
                        p.red as f64,
                        p.green as f64,
                        p.blue as f64
                    ),
                    |p: &mut ae::Pixel16, d: (f64, f64, f64, f64)| {
                        p.alpha = d.0 as i64 as u16;
                        p.red = d.1 as i64 as u16;
                        p.green = d.2 as i64 as u16;
                        p.blue = d.3 as i64 as u16;
                    }
                )
            }
            _ => {
                let c32 = libs::utils::conv_8_to_32(&colour);
                go!(
                    as_pixel32,
                    as_pixel32_mut,
                    Ops {
                        max: 1.0,
                        float: true
                    },
                    (c32.red as f64, c32.green as f64, c32.blue as f64),
                    |p: &ae::PixelF32| (
                        p.alpha as f64,
                        p.red as f64,
                        p.green as f64,
                        p.blue as f64
                    ),
                    |p: &mut ae::PixelF32, d: (f64, f64, f64, f64)| {
                        p.alpha = d.0 as f32;
                        p.red = d.1 as f32;
                        p.green = d.2 as f32;
                        p.blue = d.3 as f32;
                    }
                )
            }
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
            seed: 0,
            center_x: 64.0,
            center_y: 32.0,
            length_max: 1.0,
            length_rnd: 0.5,
            value: 5,
            value_left: 5,
            value_right: 5,
            value_top: 5,
            value_bottom: 5,
            opacity: 1.0,
            opacity_rnd: 0.5,
            point_count: 5,
            point_length: 10.0,
        }
    }

    /// スクリーン合成なので、重ねても最大値を超えない。
    #[test]
    fn the_mix_never_passes_the_maximum() {
        let o = ops8();
        assert_eq!(o.mix(0.0, 100.0), 100.0);
        assert!(o.mix(200.0, 200.0) <= 255.0);
        assert_eq!(o.mix(255.0, 255.0), 255.0);
    }

    /// 格子にぴったり乗った点は1画素だけに書く。
    #[test]
    fn an_exact_hit_writes_one_pixel() {
        let o = ops8();
        let mut m = Mask::new(8, 8);
        pset(&mut m, 3.0, 4.0, 1.0, &o);
        assert_eq!(m.px[3 + 4 * 8].1, 255.0);
        assert_eq!(m.px.iter().filter(|p| p.1 > 0.0).count(), 1);
    }

    /// 小数の点は隣の4画素に分かれる。
    #[test]
    fn a_fractional_hit_spreads_over_four() {
        let o = ops8();
        let mut m = Mask::new(8, 8);
        pset(&mut m, 3.5, 4.5, 1.0, &o);
        assert_eq!(m.px.iter().filter(|p| p.1 > 0.0).count(), 4);
        for (x, y) in [(3, 4), (4, 4), (3, 5), (4, 5)] {
            assert!(m.px[x + y * 8].1 > 0.0);
        }
    }

    /// 画面の外の点は何も書かない。元は格子ぴったりのときだけ
    /// 範囲を見ていなかった (直したところ 4)。
    #[test]
    fn a_point_outside_writes_nothing() {
        let o = ops8();
        let mut m = Mask::new(8, 8);
        pset(&mut m, -5.0, -5.0, 1.0, &o);
        pset(&mut m, 20.0, 20.0, 1.0, &o);
        pset(&mut m, -1.0, 3.0, 1.0, &o);
        assert!(m.px.iter().all(|p| p.1 == 0.0));
    }

    /// 赤にも同じ値が控えられる (ぼかしが赤を読むので)。
    #[test]
    fn the_red_mirrors_the_alpha() {
        let o = ops8();
        let mut m = Mask::new(8, 8);
        pset(&mut m, 3.0, 4.0, 1.0, &o);
        assert_eq!(m.px[3 + 4 * 8].0, m.px[3 + 4 * 8].1);
    }

    /// 線は始点から中心へ向かい、進むほど薄くなる。
    #[test]
    fn the_touch_fades_towards_the_centre() {
        let o = ops8();
        let mut m = Mask::new(32, 8);
        draw_touch_sub(&mut m, 0.0, 4.0, 20.0, 4.0, 1.0, 1.0, &o);
        assert_eq!(m.px[0 + 4 * 32].1, 255.0);
        let a = m.px[5 + 4 * 32].1;
        let b = m.px[15 + 4 * 32].1;
        assert!(a > b, "{a} > {b}");
        assert!(b > 0.0);
    }

    /// 長さを半分にすると途中で止まる。
    #[test]
    fn a_shorter_touch_stops_early() {
        let o = ops8();
        let reach = |len: f32| {
            let mut m = Mask::new(32, 8);
            draw_touch_sub(&mut m, 0.0, 4.0, 30.0, 4.0, len, 1.0, &o);
            (0..32).filter(|x| m.px[x + 4 * 32].1 > 0.0).count()
        };
        assert!(reach(0.5) < reach(1.0));
    }

    /// ぼかしは四隅も含めて全部の画素に掛かる (直したところ 2)。
    #[test]
    fn the_blur_covers_every_corner() {
        let o = ops8();
        let mut m = Mask::new(8, 6);
        // 全部を一様に埋めれば、ぼかしても同じ値が返るはず。
        for p in m.px.iter_mut() {
            *p = (200.0, 0.0);
        }
        alpha_blur(&mut m, &o);
        for (i, p) in m.px.iter().enumerate() {
            assert_eq!(p.1, 200.0, "index {i} がぼけていない");
        }
    }

    /// 中心が上に寄ると、上の本数が減って下に移る。
    #[test]
    fn the_centre_shifts_the_counts() {
        let mut i = info();
        i.center_y = 10.0;
        i.center_x = 64.0;
        calc_counts(&mut i, 128, 128);
        assert!(i.value_top < i.value_bottom);
        assert_eq!(i.value_top + i.value_bottom, 2 * i.value);
    }

    /// 中心が画面の外なら片側に全部寄る。
    #[test]
    fn a_centre_off_the_edge_moves_everything() {
        let mut i = info();
        i.center_y = -5.0;
        calc_counts(&mut i, 128, 128);
        assert_eq!(i.value_top, 0);
        assert_eq!(i.value_bottom, 2 * i.value);
    }

    /// 左右の辺の散らしは上下と同じ幅になる (直したところ 1)。
    /// 元は必ず上へ寄っていた。
    #[test]
    fn the_side_scatter_is_centred() {
        let o = ops8();
        let mut i = info();
        i.value = 1;
        i.value_top = 0;
        i.value_bottom = 0;
        i.value_left = 40;
        i.value_right = 0;
        i.point_count = 3;
        i.point_length = 20.0;
        let mut m = Mask::new(96, 96);
        draw_touch(&mut m, &i, &o);
        let ys: Vec<usize> = (0..96)
            .filter(|y| (0..96).any(|x| m.px[x + y * 96].1 > 0.0))
            .collect();
        // 上にも下にも散っている (真ん中から上だけに偏っていない)。
        assert!(ys.iter().any(|y| *y > 48), "下側に線がない: {ys:?}");
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }
}
