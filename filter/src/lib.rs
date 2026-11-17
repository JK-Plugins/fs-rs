//! F's Filter の移植。
//!
//! 「明るいところ (暗いところ) を抜き出して、太らせて、ぼかして、元の絵に
//! 合成する」を**4組ぶん**重ねられる、光のにじみ (グロー) の工房。どの組も
//! 入力から作り直すので、組どうしは互いに影響しない。
//!
//! 手順 (元の Exec08 / Exec16):
//!
//! 1. `BaseEnabled` が入っていれば、入力を `BaseOpacity` の濃さで出力へ
//!    写す (乗算済みにする)。入っていなければ**出力は手つかず**のまま
//! 2. 有効な組がひとつもなければ、ここで飛ばす
//! 3. 組ごとに:
//!    1. 入力を 16bit の作業用の世界へ写し、乗算済みにする
//!    2. `Extract` が Hi なら明るいところ、Low なら暗いところだけを残す。
//!       しきい値は `border`、そこから `softness` ぶんが斜めに抜ける
//!    3. `Brightness` が 0 でなければ RGB に足す
//!    4. 太らせ・ぼかしのどれかが要るなら、**いちど縦横半分に縮めて**から
//!       `MinToMax` (収縮→膨張で小さな穴を埋める)、`Max` (膨張)、
//!       `Blur` (ガウス) を掛け、2倍に戻す
//!    5. `Blend` の種類と `Opacity` で出力へ合成する
//! 4. `Noise` が 0 より大きければ、出力の RGB に ±Noise の乱数を足す
//!    (種はフレーム番号)
//! 5. `Alpha_ON` が入っていればアルファを最大にし、入っていなければ
//!    乗算を戻す
//!
//! 元から写した細かいところ:
//!
//! - 作業用の世界は深度に関わらず 16bit。8bit の出力へは合成のたびに
//!   8→16→8 と往復する
//! - `Opacity` は `FilterOpacity` を掛けてから 0..1 で押さえる。0 なら
//!   その組は飛ばす
//! - 縮小は隣2画素の平均、拡大は隣との平均を挿む。どちらも整数の
//!   切り捨て割り算で、縮小後は右と下にはみ出たところを 0 で埋める
//! - 太らせ・ぼかしは**縮めた側 (幅/2・高さ/2)** の上だけで動く
//! - 太らせは行 (列) の最大値を先に数えておき、窓の中でそこに届いたら
//!   打ち切る。明るさは `(r+g+b)/3`、勝った画素を RGBA まるごと持ってくる
//! - ぼかしの重みは `exp(-i²/(2*(blur/3)²)) * 256` の整数表。アルファが
//!   0 の画素は色を足さないが、重みの合計には数える
//! - `MinToMax` は反転→膨張→反転→膨張の4手で、元のまま
//! - 太らせ・ぼかしの半径には downsample を掛ける (整数割り算)
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **32bit では何もしていなかった** (`case PF_PixelFormat_ARGB128:
//!    break;`)。出力の世界は手つかずのまま返るので、32bit のプロジェクトに
//!    置くと絵が壊れる。ライブラリには `Blend16To32` / `Noise32` /
//!    `FromPremultiply32` が揃っているので、落としただけと見て 8/16bit と
//!    同じ道を通すようにした
//! 2. **LighterColor と DarkerColor だけ、下地が透明なときに Opacity を
//!    無視していた**。ほかの7つは `par` を必ず掛けるのに、この2つは
//!    `par` を掛けるところが「下地のほうが明るい/暗い」の else の中に
//!    入っている。下地が透明な画素 (下敷きを描かない設定なら全面) で
//!    不透明度が効かなくなる
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Filter

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

const MAX8: i32 = 255;
const MAX16: i32 = 32768;
const HALF16: i32 = 16384;

/// 元の RoundShort。
fn rshort(v: i32) -> i32 {
    v.clamp(0, MAX16)
}
/// 元の RoundShortFpLong (double を受けて切り捨て)。
fn rshort_f(v: f64) -> i32 {
    v.clamp(0.0, MAX16 as f64) as i32
}
/// 元の RoundByteLong。
fn rbyte(v: i32) -> i32 {
    v.clamp(0, MAX8)
}
/// 元の RoundByteDouble (切り捨て)。
fn rbyte_f(v: f64) -> i32 {
    v.clamp(0.0, MAX8 as f64) as i32
}
/// 元の FS_CONVERT8TO16。整数の割り算。
fn c8to16(v: i32) -> i32 {
    rshort((v * MAX16 + 128) / MAX8)
}
/// 元の FS_CONVERT16TO8。
fn c16to8(v: i32) -> i32 {
    rbyte((v * MAX8 + HALF16) / MAX16)
}

/// 16bit の画素。元の PF_Pixel16。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct P16 {
    a: i32,
    r: i32,
    g: i32,
    b: i32,
}

/// 16bit の作業用の世界。元は NewWorld で作る ARGB64 (ここでは隙間なし)。
struct W16 {
    w: usize,
    h: usize,
    px: Vec<P16>,
}

impl W16 {
    fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            px: vec![P16::default(); w * h],
        }
    }
    fn at(&self, x: usize, y: usize) -> P16 {
        self.px[x + y * self.w]
    }
    fn set(&mut self, x: usize, y: usize, p: P16) {
        self.px[x + y * self.w] = p;
    }
}

/// 元の ToPremultiply16。
fn to_premultiply(w: &mut W16) {
    for p in w.px.iter_mut() {
        if p.a == 0 {
            *p = P16::default();
        } else {
            let a = p.a as f64 / MAX16 as f64;
            p.r = rshort_f(p.r as f64 * a);
            p.g = rshort_f(p.g as f64 * a);
            p.b = rshort_f(p.b as f64 * a);
        }
    }
}

/// 元の ExtractHi16 / ExtractLo16。`hi` が立っていれば明るいほうを残す。
fn extract(w: &mut W16, border: f64, softness: f64, hi: bool) {
    let soft = (softness * MAX16 as f64 + 0.5) as i32;
    let (start, last) = if hi {
        let last = (border * MAX16 as f64 + 0.5) as i32;
        (last - soft, last)
    } else {
        let start = (border * MAX16 as f64 + 0.5) as i32;
        (start, start + soft)
    };
    for p in w.px.iter_mut() {
        let v = (p.r + p.g + p.b) / 3;
        let inside = if hi { v <= start } else { v >= last };
        if inside {
            *p = P16::default();
        } else if (hi && v < last) || (!hi && v > start) {
            // 元: soft が 0 ならこの枝に入らない (start == last なので)。
            let q = if hi {
                MAX16 * (v - start) / soft
            } else {
                MAX16 * (last - v) / soft
            };
            p.r = rshort(p.r * q / MAX16);
            p.g = rshort(p.g * q / MAX16);
            p.b = rshort(p.b * q / MAX16);
            p.a = rshort(p.a * q / MAX16);
        }
    }
}

/// 元の Brightness16。アルファは触らない。
fn brightness(w: &mut W16, v: f64) {
    let n = (MAX16 as f64 * v) as i32;
    for p in w.px.iter_mut() {
        p.r = rshort(p.r + n);
        p.g = rshort(p.g + n);
        p.b = rshort(p.b + n);
    }
}

/// 元の Rev16。
fn rev(w: &mut W16) {
    for p in w.px.iter_mut() {
        p.a = MAX16 - p.a;
        p.r = MAX16 - p.r;
        p.g = MAX16 - p.g;
        p.b = MAX16 - p.b;
    }
}

/// 元の ToHarfSize16。左上の 1/4 に縮め、残りを 0 で埋める。
fn to_half(w: &mut W16) {
    let (ow, oh) = (w.w, w.h);
    let (wh, hh) = (ow / 2, oh / 2);
    let avg = |a: P16, b: P16| P16 {
        a: (a.a + b.a) / 2,
        r: (a.r + b.r) / 2,
        g: (a.g + b.g) / 2,
        b: (a.b + b.b) / 2,
    };
    for y in 0..oh {
        for x in 0..wh {
            let c = avg(w.at(2 * x, y), w.at(2 * x + 1, y));
            w.set(x, y, c);
        }
        for x in wh..ow {
            w.set(x, y, P16::default());
        }
    }
    for x in 0..ow {
        for y in 0..hh {
            let c = avg(w.at(x, 2 * y), w.at(x, 2 * y + 1));
            w.set(x, y, c);
        }
        for y in hh..oh {
            w.set(x, y, P16::default());
        }
    }
}

/// 元の ToDoubleSize16。隣との平均を挿んで元の大きさへ戻す。
fn to_double(w: &mut W16) {
    let (ow, oh) = (w.w, w.h);
    let (hw, hh) = (ow / 2, oh / 2);
    let avg = |a: P16, b: P16| P16 {
        a: (a.a + b.a) / 2,
        r: (a.r + b.r) / 2,
        g: (a.g + b.g) / 2,
        b: (a.b + b.b) / 2,
    };
    let mut scan: Vec<P16> = Vec::new();
    for y in 0..oh {
        scan.clear();
        for x in 0..hw {
            scan.push(w.at(x, y));
        }
        for x in 0..hw {
            let x2 = (x + 1).min(hw - 1);
            let c0 = scan[x];
            let c = if x == x2 { c0 } else { avg(c0, scan[x2]) };
            w.set(2 * x, y, c0);
            if 2 * x + 1 < ow {
                w.set(2 * x + 1, y, c);
            }
        }
    }
    for x in 0..ow {
        scan.clear();
        for y in 0..hh {
            scan.push(w.at(x, y));
        }
        for y in 0..hh {
            let y2 = (y + 1).min(hh - 1);
            let c0 = scan[y];
            let c = if y == y2 { c0 } else { avg(c0, scan[y2]) };
            w.set(x, 2 * y, c0);
            if 2 * y + 1 < oh {
                w.set(x, 2 * y + 1, c);
            }
        }
    }
}

/// 元の MaxAllHor / MaxAllVur。縮めた側 (幅/2・高さ/2) の上だけで動く。
fn max16(w: &mut W16, radius: i64) {
    if radius == 0 {
        return;
    }
    let (hw, hh) = (w.w / 2, w.h / 2);
    let lv = |p: &P16| rshort((p.r + p.g + p.b) / 3);
    // 横。
    let mut line: Vec<P16> = Vec::new();
    let mut lvs: Vec<i32> = Vec::new();
    for y in 0..hh {
        line.clear();
        lvs.clear();
        let mut mv = 0;
        for x in 0..hw {
            let p = w.at(x, y);
            let l = lv(&p);
            if mv < l {
                mv = l;
            }
            line.push(p);
            lvs.push(l);
        }
        if mv <= 0 {
            continue;
        }
        for x in 0..hw {
            let mut t = 0;
            let mut c = P16::default();
            let mut i = -radius;
            while i <= radius {
                let x2 = x as i64 + i;
                i += 1;
                if x2 < 0 {
                    continue;
                }
                if x2 >= hw as i64 {
                    break;
                }
                let x2 = x2 as usize;
                if t < lvs[x2] {
                    t = lvs[x2];
                    c = line[x2];
                }
                if t >= mv {
                    break;
                }
            }
            w.set(x, y, c);
        }
    }
    // 縦。
    for x in 0..hw {
        line.clear();
        lvs.clear();
        let mut mv = 0;
        for y in 0..hh {
            let p = w.at(x, y);
            let l = lv(&p);
            if mv < l {
                mv = l;
            }
            line.push(p);
            lvs.push(l);
        }
        if mv <= 0 {
            continue;
        }
        for y in 0..hh {
            let mut t = 0;
            let mut c = P16::default();
            for i in -radius..=radius {
                let y2 = y as i64 + i;
                if y2 >= 0 && y2 < hh as i64 {
                    let y2 = y2 as usize;
                    if t < lvs[y2] {
                        t = lvs[y2];
                        c = line[y2];
                    }
                    if t >= mv {
                        break;
                    }
                }
            }
            w.set(x, y, c);
        }
    }
}

/// 元の MakeTbl。`exp(-i²/(2*(blur/3)²)) * 256` の整数表。
fn blur_table(blur: i64) -> Vec<i64> {
    let zone = blur as f64 / 3.0;
    (0..=blur)
        .map(|i| ((-(i as f64) * i as f64 / (2.0 * zone * zone)).exp() * 256.0) as i64)
        .collect()
}

/// 元の BlurHor / BlurVur。
fn blur16(w: &mut W16, blur: i64) {
    if blur == 0 {
        return;
    }
    let blur = blur.max(1);
    let tbl = blur_table(blur);
    let (hw, hh) = (w.w / 2, w.h / 2);
    let mut scan: Vec<P16> = Vec::new();

    macro_rules! pass {
        ($outer:expr, $inner:expr, $get:expr, $put:expr) => {
            for o in 0..$outer {
                scan.clear();
                for i in 0..$inner {
                    scan.push($get(w, o, i));
                }
                for i in 0..$inner {
                    let mut count: i64 = 0;
                    let (mut r, mut g, mut b, mut a): (i64, i64, i64, i64) = (0, 0, 0, 0);
                    // 元: アルファが 0 の画素は色を足さないが、重みの合計には数える。
                    let mut idx: Vec<(usize, i64)> = Vec::with_capacity(2 * blur as usize + 1);
                    idx.push((i, tbl[0]));
                    for k in 1..=blur as usize {
                        if i >= k {
                            idx.push((i - k, tbl[k]));
                        }
                        if i + k < $inner {
                            idx.push((i + k, tbl[k]));
                        }
                    }
                    for (j, gauss) in idx {
                        let p = &scan[j];
                        if p.a > 0 {
                            r += p.r as i64 * gauss;
                            g += p.g as i64 * gauss;
                            b += p.b as i64 * gauss;
                            a += p.a as i64 * gauss;
                        }
                        count += gauss;
                    }
                    if count > 0 {
                        let p = P16 {
                            a: rshort((a / count) as i32),
                            r: rshort((r / count) as i32),
                            g: rshort((g / count) as i32),
                            b: rshort((b / count) as i32),
                        };
                        $put(w, o, i, p);
                    }
                }
            }
        };
    }
    pass!(
        hh,
        hw,
        |w: &W16, y: usize, x: usize| w.at(x, y),
        |w: &mut W16, y: usize, x: usize, p: P16| w.set(x, y, p)
    );
    pass!(
        hw,
        hh,
        |w: &W16, x: usize, y: usize| w.at(x, y),
        |w: &mut W16, x: usize, y: usize, p: P16| w.set(x, y, p)
    );
}

/// 元の BLEND_* マクロ。
fn b_normal(s: i32, d: i32, a: i32) -> i32 {
    rshort(s - (s - d) * a / MAX16)
}
fn b_screen(s: i32, d: i32) -> i32 {
    rshort(s + d - s * d / MAX16)
}
fn b_mult(s: i32, d: i32) -> i32 {
    rshort(s * d / MAX16)
}
fn b_overlay(s: i32, d: i32) -> i32 {
    if s < MAX16 / 2 {
        rshort(s * d * 2 / MAX16)
    } else {
        rshort(2 * (s + d - s * d / MAX16) - MAX16)
    }
}

/// 元の BlendPixel16_sub。`under` が下地、`upper` が重ねるほう。
fn blend_pixel(under: P16, upper: P16, par: i32, mode: i64) -> P16 {
    if par <= 0 || upper.alpha_le_zero() {
        return under;
    }
    // 元: LighterColor / DarkerColor だけ道が違う。
    if mode == 5 || mode == 8 {
        let lighter = mode == 5;
        if under.a <= 0 {
            // 元はここで par を掛け忘れていた (直したところ 2)。
            if FAITHFUL {
                return upper;
            }
            return apply_par(under, upper, par);
        }
        let um = under.r + under.g + under.b;
        let pm = upper.r + upper.g + upper.b;
        let keep_under = if lighter { um > pm } else { um < pm };
        if keep_under {
            return under;
        }
        return apply_par(under, upper, par);
    }

    let r = if under.a <= 0 {
        upper
    } else {
        let ch = |s: i32, d: i32| -> i32 {
            match mode {
                2 => rshort(s + d),              // Add
                3 => b_screen(s, d),             // Screen
                4 => s.max(d),                   // Lighten
                6 => b_mult(s, d),               // Multiply
                7 => s.min(d),                   // Darken
                9 => b_overlay(s, d),            // Overlay
                _ => b_normal(s, d, upper.a),    // Normal
            }
        };
        P16 {
            // Overlay だけアルファが乗算、ほかはスクリーン。
            a: if mode == 9 {
                b_mult(under.a, upper.a)
            } else {
                b_screen(under.a, upper.a)
            },
            r: ch(under.r, upper.r),
            g: ch(under.g, upper.g),
            b: ch(under.b, upper.b),
        }
    };
    apply_par(under, r, par)
}

/// 元の「par < PF_MAX_CHAN16 なら通常合成で薄める」ところ。
fn apply_par(under: P16, r: P16, par: i32) -> P16 {
    if par >= MAX16 {
        return r;
    }
    P16 {
        a: b_normal(under.a, r.a, par),
        r: b_normal(under.r, r.r, par),
        g: b_normal(under.g, r.g, par),
        b: b_normal(under.b, r.b, par),
    }
}

impl P16 {
    fn alpha_le_zero(&self) -> bool {
        self.a <= 0
    }
}

/// 元の FsRand。xorShift の上位 15bit。
struct FsRand {
    tbl: [u32; 4],
}

impl FsRand {
    fn new(s: u32) -> Self {
        let mut tbl = [0u32; 4];
        let mut ss = s.wrapping_add(100);
        for i in 1..=4u32 {
            ss = 1812433253u32
                .wrapping_mul(ss ^ (ss >> 30))
                .wrapping_add(i);
            tbl[(i - 1) as usize] = ss;
        }
        Self { tbl }
    }
    fn next(&mut self) -> i64 {
        let t = self.tbl[0] ^ (self.tbl[0] << 11);
        self.tbl[0] = self.tbl[1];
        self.tbl[1] = self.tbl[2];
        self.tbl[2] = self.tbl[3];
        self.tbl[3] = (self.tbl[3] ^ (self.tbl[3] >> 19)) ^ (t ^ (t >> 8));
        ((self.tbl[3] >> 16) & 0x7FFF) as i64
    }
}

/// 1組ぶんの設定。元の ParamSetInfo。
#[derive(Debug, Clone, Copy)]
struct SetInfo {
    enabled: bool,
    extract_mode: i64,
    border_hi: f64,
    softness_hi: f64,
    border_lo: f64,
    softness_lo: f64,
    brightness: f64,
    minmax: i64,
    max: i64,
    blur: i64,
    opacity: f64,
    blend_mode: i64,
}

/// 1組ぶんを作業用の世界の上で組み立てる。元の for の中身。
fn build_set(w: &mut W16, si: &SetInfo) {
    to_premultiply(w);
    match si.extract_mode {
        2 => extract(w, si.border_hi, si.softness_hi, true),
        3 => extract(w, si.border_lo, si.softness_lo, false),
        _ => {}
    }
    if si.brightness != 0.0 {
        brightness(w, si.brightness);
    }
    if si.minmax > 0 || si.max > 0 || si.blur > 0 {
        to_half(w);
        if si.minmax > 0 {
            rev(w);
            max16(w, si.minmax);
            rev(w);
            max16(w, si.minmax);
        }
        if si.max > 0 {
            max16(w, si.max);
        }
        if si.blur > 0 {
            blur16(w, si.blur);
        }
        to_double(w);
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    BaseEnabled,
    BaseOpacity,
    FilterOpacity,
    Topic0,
    Enabled0,
    Extract0,
    BorderHi0,
    SoftHi0,
    BorderLo0,
    SoftLo0,
    Bright0,
    MinMax0,
    Max0,
    Blur0,
    Opacity0,
    Blend0,
    TopicEnd0,
    Topic1,
    Enabled1,
    Extract1,
    BorderHi1,
    SoftHi1,
    BorderLo1,
    SoftLo1,
    Bright1,
    MinMax1,
    Max1,
    Blur1,
    Opacity1,
    Blend1,
    TopicEnd1,
    Topic2,
    Enabled2,
    Extract2,
    BorderHi2,
    SoftHi2,
    BorderLo2,
    SoftLo2,
    Bright2,
    MinMax2,
    Max2,
    Blur2,
    Opacity2,
    Blend2,
    TopicEnd2,
    Topic3,
    Enabled3,
    Extract3,
    BorderHi3,
    SoftHi3,
    BorderLo3,
    SoftLo3,
    Bright3,
    MinMax3,
    Max3,
    Blur3,
    Opacity3,
    Blend3,
    TopicEnd3,
    Noise,
    AlphaOn,
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
        // 元: PF_ADD_CHECKBOX("BaseEnabled", "on", TRUE)
        params.add(
            Params::BaseEnabled,
            "BaseEnabled",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("BaseOpacity", 0, 100, 0, 100, 1, 100, 1, 0, 0)
        params.add(
            Params::BaseOpacity,
            "BaseOpacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("FilterOpacity", 0, 300, 0, 150, 1, 100, 1, 0, 0)
        params.add(
            Params::FilterOpacity,
            "FilterOpacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(150.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_TOPIC("Filter_1") から PF_END_TOPIC まで。
        params.add_group(
            Params::Topic0,
            Params::TopicEnd0,
            "Filter_1",
            true,
            |p| {
                    // 元: PF_ADD_CHECKBOX("Enabled_1", "on", FALSE)
                    p.add(
                        Params::Enabled0,
                        "Enabled_1",
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Extract_1", 3, 1, "None|Hi|Low")
                    p.add(
                        Params::Extract0,
                        "Extract_1",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&["None", "Hi", "Low"]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderHi0,
                        "border_hi_1",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(50.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(75.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(70.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftHi0,
                        "softness_hi_1",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(50.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderLo0,
                        "border_lo_1",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(30.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftLo0,
                        "softness_lo_1",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(30.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Bright0,
                        "Brightness_1",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(-50.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(-20.0);
                            f.set_slider_max(20.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(0.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::MinMax0,
                        "MinToMax_1",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(10);
                            f.set_slider_min(0);
                            f.set_slider_max(5);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Max0,
                        "Max_1",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(50);
                            f.set_slider_min(0);
                            f.set_slider_max(20);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Blur0,
                        "Blur_1",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(200);
                            f.set_slider_min(0);
                            f.set_slider_max(50);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_FLOAT_SLIDER("Opacity_1", 0, 100, 0, 100, 1, 50, 1, 0, 0)
                    p.add(
                        Params::Opacity0,
                        "Opacity_1",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(50.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Blend_1", 9, 1, STR_BLEND_ITEMS)
                    p.add(
                        Params::Blend0,
                        "Blend_1",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&[
                                "Normal",
                                "Add",
                                "Screen",
                                "Lighten",
                                "LighterColor",
                                "Multiply",
                                "Darken",
                                "DarkerColor",
                                "Overlay",
                            ]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    Ok(())
            },
        )?;
        // 元: PF_ADD_TOPIC("Filter_2") から PF_END_TOPIC まで。
        params.add_group(
            Params::Topic1,
            Params::TopicEnd1,
            "Filter_2",
            true,
            |p| {
                    // 元: PF_ADD_CHECKBOX("Enabled_2", "on", FALSE)
                    p.add(
                        Params::Enabled1,
                        "Enabled_2",
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Extract_2", 3, 1, "None|Hi|Low")
                    p.add(
                        Params::Extract1,
                        "Extract_2",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&["None", "Hi", "Low"]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderHi1,
                        "border_hi_2",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(50.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(75.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(70.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftHi1,
                        "softness_hi_2",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(50.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderLo1,
                        "border_lo_2",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(30.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftLo1,
                        "softness_lo_2",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(30.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Bright1,
                        "Brightness_2",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(-50.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(-20.0);
                            f.set_slider_max(20.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(0.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::MinMax1,
                        "MinToMax_2",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(10);
                            f.set_slider_min(0);
                            f.set_slider_max(5);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Max1,
                        "Max_2",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(50);
                            f.set_slider_min(0);
                            f.set_slider_max(20);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Blur1,
                        "Blur_2",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(200);
                            f.set_slider_min(0);
                            f.set_slider_max(50);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_FLOAT_SLIDER("Opacity_2", 0, 100, 0, 100, 1, 50, 1, 0, 0)
                    p.add(
                        Params::Opacity1,
                        "Opacity_2",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(50.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Blend_2", 9, 1, STR_BLEND_ITEMS)
                    p.add(
                        Params::Blend1,
                        "Blend_2",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&[
                                "Normal",
                                "Add",
                                "Screen",
                                "Lighten",
                                "LighterColor",
                                "Multiply",
                                "Darken",
                                "DarkerColor",
                                "Overlay",
                            ]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    Ok(())
            },
        )?;
        // 元: PF_ADD_TOPIC("Filter_3") から PF_END_TOPIC まで。
        params.add_group(
            Params::Topic2,
            Params::TopicEnd2,
            "Filter_3",
            true,
            |p| {
                    // 元: PF_ADD_CHECKBOX("Enabled_3", "on", FALSE)
                    p.add(
                        Params::Enabled2,
                        "Enabled_3",
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Extract_3", 3, 1, "None|Hi|Low")
                    p.add(
                        Params::Extract2,
                        "Extract_3",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&["None", "Hi", "Low"]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderHi2,
                        "border_hi_3",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(50.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(75.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(70.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftHi2,
                        "softness_hi_3",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(50.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderLo2,
                        "border_lo_3",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(30.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftLo2,
                        "softness_lo_3",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(30.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Bright2,
                        "Brightness_3",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(-50.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(-20.0);
                            f.set_slider_max(20.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(0.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::MinMax2,
                        "MinToMax_3",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(10);
                            f.set_slider_min(0);
                            f.set_slider_max(5);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Max2,
                        "Max_3",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(50);
                            f.set_slider_min(0);
                            f.set_slider_max(20);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Blur2,
                        "Blur_3",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(200);
                            f.set_slider_min(0);
                            f.set_slider_max(50);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_FLOAT_SLIDER("Opacity_3", 0, 100, 0, 100, 1, 50, 1, 0, 0)
                    p.add(
                        Params::Opacity2,
                        "Opacity_3",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(50.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Blend_3", 9, 1, STR_BLEND_ITEMS)
                    p.add(
                        Params::Blend2,
                        "Blend_3",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&[
                                "Normal",
                                "Add",
                                "Screen",
                                "Lighten",
                                "LighterColor",
                                "Multiply",
                                "Darken",
                                "DarkerColor",
                                "Overlay",
                            ]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    Ok(())
            },
        )?;
        // 元: PF_ADD_TOPIC("Filter_4") から PF_END_TOPIC まで。
        params.add_group(
            Params::Topic3,
            Params::TopicEnd3,
            "Filter_4",
            true,
            |p| {
                    // 元: PF_ADD_CHECKBOX("Enabled_4", "on", FALSE)
                    p.add(
                        Params::Enabled3,
                        "Enabled_4",
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Extract_4", 3, 1, "None|Hi|Low")
                    p.add(
                        Params::Extract3,
                        "Extract_4",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&["None", "Hi", "Low"]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderHi3,
                        "border_hi_4",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(50.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(75.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(70.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftHi3,
                        "softness_hi_4",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(50.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::BorderLo3,
                        "border_lo_4",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(30.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::SoftLo3,
                        "softness_lo_4",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(30.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(30.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(20.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Bright3,
                        "Brightness_4",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(-50.0);
                            f.set_valid_max(50.0);
                            f.set_slider_min(-20.0);
                            f.set_slider_max(20.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(0.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::MinMax3,
                        "MinToMax_4",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(10);
                            f.set_slider_min(0);
                            f.set_slider_max(5);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Max3,
                        "Max_4",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(50);
                            f.set_slider_min(0);
                            f.set_slider_max(20);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        Params::Blur3,
                        "Blur_4",
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(200);
                            f.set_slider_min(0);
                            f.set_slider_max(50);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_FLOAT_SLIDER("Opacity_4", 0, 100, 0, 100, 1, 50, 1, 0, 0)
                    p.add(
                        Params::Opacity3,
                        "Opacity_4",
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(50.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_POPUP("Blend_4", 9, 1, STR_BLEND_ITEMS)
                    p.add(
                        Params::Blend3,
                        "Blend_4",
                        ae::PopupDef::setup(|f| {
                            f.set_options(&[
                                "Normal",
                                "Add",
                                "Screen",
                                "Lighten",
                                "LighterColor",
                                "Multiply",
                                "Darken",
                                "DarkerColor",
                                "Overlay",
                            ]);
                            f.set_default(1);
                            f.set_value(f.default());
                        }),
                    )?;
                    Ok(())
            },
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("Noise", 0, 100, 0, 5, 1, 0, 1, 0, 0)
        params.add(
            Params::Noise,
            "Noise",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(5.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("Alpha_ON", "on", FALSE)
        params.add(
            Params::AlphaOn,
            "Alpha_ON",
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
                out_data.set_return_msg("fs-rs filter");
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
        let down = |v: i64| -> i64 { v * ds.num as i64 / ds.den as i64 };
        let cb = |p: Params, params: &mut ae::Parameters<Params>| -> Result<bool, Error> {
            Ok(params.get(p)?.as_checkbox()?.value())
        };
        let pop = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(params.get(p)?.as_popup()?.value() as i64)
        };
        let sl = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(params.get(p)?.as_slider()?.value() as i64)
        };
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() as f64 / 100.0)
        };

        // 元の GetParams。
        let base_enabled = cb(Params::BaseEnabled, params)?;
        let base_opacity = pct(Params::BaseOpacity, params)?;
        let filter_opacity = pct(Params::FilterOpacity, params)?;
        let mut sets: Vec<SetInfo> = Vec::with_capacity(4);
        sets.push(SetInfo {
            enabled: cb(Params::Enabled0, params)?,
            extract_mode: pop(Params::Extract0, params)?,
            border_hi: pct(Params::BorderHi0, params)?,
            softness_hi: pct(Params::SoftHi0, params)?,
            border_lo: pct(Params::BorderLo0, params)?,
            softness_lo: pct(Params::SoftLo0, params)?,
            brightness: pct(Params::Bright0, params)?,
            minmax: down(sl(Params::MinMax0, params)?),
            max: down(sl(Params::Max0, params)?),
            blur: down(sl(Params::Blur0, params)?),
            opacity: (pct(Params::Opacity0, params)? * filter_opacity).clamp(0.0, 1.0),
            blend_mode: pop(Params::Blend0, params)?,
        });
        sets.push(SetInfo {
            enabled: cb(Params::Enabled1, params)?,
            extract_mode: pop(Params::Extract1, params)?,
            border_hi: pct(Params::BorderHi1, params)?,
            softness_hi: pct(Params::SoftHi1, params)?,
            border_lo: pct(Params::BorderLo1, params)?,
            softness_lo: pct(Params::SoftLo1, params)?,
            brightness: pct(Params::Bright1, params)?,
            minmax: down(sl(Params::MinMax1, params)?),
            max: down(sl(Params::Max1, params)?),
            blur: down(sl(Params::Blur1, params)?),
            opacity: (pct(Params::Opacity1, params)? * filter_opacity).clamp(0.0, 1.0),
            blend_mode: pop(Params::Blend1, params)?,
        });
        sets.push(SetInfo {
            enabled: cb(Params::Enabled2, params)?,
            extract_mode: pop(Params::Extract2, params)?,
            border_hi: pct(Params::BorderHi2, params)?,
            softness_hi: pct(Params::SoftHi2, params)?,
            border_lo: pct(Params::BorderLo2, params)?,
            softness_lo: pct(Params::SoftLo2, params)?,
            brightness: pct(Params::Bright2, params)?,
            minmax: down(sl(Params::MinMax2, params)?),
            max: down(sl(Params::Max2, params)?),
            blur: down(sl(Params::Blur2, params)?),
            opacity: (pct(Params::Opacity2, params)? * filter_opacity).clamp(0.0, 1.0),
            blend_mode: pop(Params::Blend2, params)?,
        });
        sets.push(SetInfo {
            enabled: cb(Params::Enabled3, params)?,
            extract_mode: pop(Params::Extract3, params)?,
            border_hi: pct(Params::BorderHi3, params)?,
            softness_hi: pct(Params::SoftHi3, params)?,
            border_lo: pct(Params::BorderLo3, params)?,
            softness_lo: pct(Params::SoftLo3, params)?,
            brightness: pct(Params::Bright3, params)?,
            minmax: down(sl(Params::MinMax3, params)?),
            max: down(sl(Params::Max3, params)?),
            blur: down(sl(Params::Blur3, params)?),
            opacity: (pct(Params::Opacity3, params)? * filter_opacity).clamp(0.0, 1.0),
            blend_mode: pop(Params::Blend3, params)?,
        });
        let noise = pct(Params::Noise, params)?;
        let alpha_on = cb(Params::AlphaOn, params)?;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }
        // 元の Exec: 乱数の種はフレーム番号。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = FsRand::new(frame);

        let any = sets.iter().any(|s| s.enabled);
        let noise16 = (MAX16 as f64 * noise) as i32;
        let noise8 = (MAX8 as f64 * noise) as i32;

        macro_rules! run {
            ($rd:ident, $wr:ident, $max:expr, $base:expr, $to16:expr, $from16:expr,
             $noise_amount:expr, $round:expr) => {{
                // 1. 下敷き。入っていなければ出力は手つかず (元のまま)。
                if base_enabled {
                    for y in 0..h {
                        for x in 0..w {
                            let p = *in_layer.$rd(x, y);
                            *out_layer.$wr(x, y) = $base(&p);
                        }
                    }
                }
                // 2-3. 組ごとに作業用の世界を作って合成。
                if any {
                    for si in &sets {
                        if !si.enabled || si.opacity <= 0.0 {
                            continue;
                        }
                        let mut work = W16::new(w, h);
                        for y in 0..h {
                            for x in 0..w {
                                work.set(x, y, $to16(in_layer.$rd(x, y)));
                            }
                        }
                        build_set(&mut work, si);
                        let par = (MAX16 as f64 * si.opacity + 0.5) as i32;
                        for y in 0..h {
                            for x in 0..w {
                                let under = $to16(out_layer.$rd(x, y));
                                let r = blend_pixel(under, work.at(x, y), par, si.blend_mode);
                                *out_layer.$wr(x, y) = $from16(&r);
                            }
                        }
                    }
                }
                // 4. ノイズ。
                if noise > 0.0 {
                    let n: i64 = $noise_amount;
                    for y in 0..h {
                        for x in 0..w {
                            let mut d = [0i64; 3];
                            for k in 0..3 {
                                d[k] = n - 2 * n * rng.next() / 0x7FFF;
                            }
                            let p = out_layer.$wr(x, y);
                            $round(p, d);
                        }
                    }
                }
                // 5. アルファを立てるか、乗算を戻す。
                for y in 0..h {
                    for x in 0..w {
                        let p = out_layer.$wr(x, y);
                        if alpha_on {
                            p.alpha = $max;
                        } else {
                            from_premultiply_px(p);
                        }
                    }
                }
            }};
        }

        match depth {
            8 => {
                fn from_premultiply_px(p: &mut ae::Pixel8) {
                    let a = p.alpha as f64;
                    if a == 0.0 {
                        *p = ae::Pixel8 {
                            alpha: 0,
                            red: 0,
                            green: 0,
                            blue: 0,
                        };
                    } else {
                        p.red = rbyte_f(p.red as f64 * MAX8 as f64 / a) as u8;
                        p.green = rbyte_f(p.green as f64 * MAX8 as f64 / a) as u8;
                        p.blue = rbyte_f(p.blue as f64 * MAX8 as f64 / a) as u8;
                    }
                }
                run!(
                    as_pixel8,
                    as_pixel8_mut,
                    255u8,
                    |p: &ae::Pixel8| -> ae::Pixel8 {
                        // 元の BaseCopy8。
                        let a = p.alpha as f64 * base_opacity;
                        let a = a.clamp(0.0, MAX8 as f64);
                        if a <= 0.0 {
                            ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        } else if a >= MAX8 as f64 {
                            *p
                        } else {
                            ae::Pixel8 {
                                alpha: a as i64 as u8,
                                red: rbyte_f(p.red as f64 * a / MAX8 as f64) as u8,
                                green: rbyte_f(p.green as f64 * a / MAX8 as f64) as u8,
                                blue: rbyte_f(p.blue as f64 * a / MAX8 as f64) as u8,
                            }
                        }
                    },
                    |p: &ae::Pixel8| P16 {
                        a: c8to16(p.alpha as i32),
                        r: c8to16(p.red as i32),
                        g: c8to16(p.green as i32),
                        b: c8to16(p.blue as i32),
                    },
                    |p: &P16| ae::Pixel8 {
                        alpha: c16to8(p.a) as u8,
                        red: c16to8(p.r) as u8,
                        green: c16to8(p.g) as u8,
                        blue: c16to8(p.b) as u8,
                    },
                    noise8 as i64,
                    |p: &mut ae::Pixel8, d: [i64; 3]| {
                        p.red = rbyte(p.red as i32 + d[0] as i32) as u8;
                        p.green = rbyte(p.green as i32 + d[1] as i32) as u8;
                        p.blue = rbyte(p.blue as i32 + d[2] as i32) as u8;
                    }
                )
            }
            16 => {
                fn from_premultiply_px(p: &mut ae::Pixel16) {
                    let a = p.alpha as f64;
                    if a == 0.0 {
                        *p = ae::Pixel16 {
                            alpha: 0,
                            red: 0,
                            green: 0,
                            blue: 0,
                        };
                    } else {
                        p.red = rshort_f(p.red as f64 * MAX16 as f64 / a) as u16;
                        p.green = rshort_f(p.green as f64 * MAX16 as f64 / a) as u16;
                        p.blue = rshort_f(p.blue as f64 * MAX16 as f64 / a) as u16;
                    }
                }
                run!(
                    as_pixel16,
                    as_pixel16_mut,
                    32768u16,
                    |p: &ae::Pixel16| -> ae::Pixel16 {
                        let a = (p.alpha as f64 * base_opacity).clamp(0.0, MAX16 as f64);
                        if a <= 0.0 {
                            ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        } else if a >= MAX16 as f64 {
                            *p
                        } else {
                            ae::Pixel16 {
                                alpha: a as i64 as u16,
                                red: rshort_f(p.red as f64 * a / MAX16 as f64) as u16,
                                green: rshort_f(p.green as f64 * a / MAX16 as f64) as u16,
                                blue: rshort_f(p.blue as f64 * a / MAX16 as f64) as u16,
                            }
                        }
                    },
                    |p: &ae::Pixel16| P16 {
                        a: p.alpha as i32,
                        r: p.red as i32,
                        g: p.green as i32,
                        b: p.blue as i32,
                    },
                    |p: &P16| ae::Pixel16 {
                        alpha: p.a as u16,
                        red: p.r as u16,
                        green: p.g as u16,
                        blue: p.b as u16,
                    },
                    noise16 as i64,
                    |p: &mut ae::Pixel16, d: [i64; 3]| {
                        p.red = rshort(p.red as i32 + d[0] as i32) as u16;
                        p.green = rshort(p.green as i32 + d[1] as i32) as u16;
                        p.blue = rshort(p.blue as i32 + d[2] as i32) as u16;
                    }
                )
            }
            _ => {
                // 元は 32bit で何もしていなかった (直したところ 1)。
                if FAITHFUL {
                    return Ok(());
                }
                fn from_premultiply_px(p: &mut ae::PixelF32) {
                    let a = p.alpha as f64;
                    if a == 0.0 {
                        *p = ae::PixelF32 {
                            alpha: 0.0,
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                        };
                    } else {
                        p.red = libs::utils::round_fp_short_double(p.red as f64 / a);
                        p.green = libs::utils::round_fp_short_double(p.green as f64 / a);
                        p.blue = libs::utils::round_fp_short_double(p.blue as f64 / a);
                    }
                }
                // 32bit は 16bit を経由する (元の CONV32TO16 / CONV16TO32)。
                run!(
                    as_pixel32,
                    as_pixel32_mut,
                    1.0f32,
                    |p: &ae::PixelF32| -> ae::PixelF32 {
                        let a = (p.alpha as f64 * base_opacity).clamp(0.0, 1.0);
                        if a <= 0.0 {
                            ae::PixelF32 {
                                alpha: 0.0,
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                            }
                        } else if a >= 1.0 {
                            *p
                        } else {
                            ae::PixelF32 {
                                alpha: a as f32,
                                red: libs::utils::round_fp_short_double(p.red as f64 * a),
                                green: libs::utils::round_fp_short_double(p.green as f64 * a),
                                blue: libs::utils::round_fp_short_double(p.blue as f64 * a),
                            }
                        }
                    },
                    |p: &ae::PixelF32| {
                        let c = libs::utils::conv_32_to_16(&ae::PixelF32 {
                            alpha: p.alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        });
                        P16 {
                            a: c.alpha as i32,
                            r: c.red as i32,
                            g: c.green as i32,
                            b: c.blue as i32,
                        }
                    },
                    |p: &P16| {
                        let c = libs::utils::conv_16_to_32(&ae::Pixel16 {
                            alpha: p.a as u16,
                            red: p.r as u16,
                            green: p.g as u16,
                            blue: p.b as u16,
                        });
                        ae::PixelF32 {
                            alpha: c.alpha,
                            red: c.red,
                            green: c.green,
                            blue: c.blue,
                        }
                    },
                    noise16 as i64,
                    |p: &mut ae::PixelF32, d: [i64; 3]| {
                        let f = |v: f32, n: i64| -> f32 {
                            libs::utils::round_fp_short_double(
                                v as f64 + n as f64 / MAX16 as f64,
                            )
                        };
                        p.red = f(p.red, d[0]);
                        p.green = f(p.green, d[1]);
                        p.blue = f(p.blue, d[2]);
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

    fn px(v: i32, a: i32) -> P16 {
        P16 { a, r: v, g: v, b: v }
    }

    fn set() -> SetInfo {
        SetInfo {
            enabled: true,
            extract_mode: 1,
            border_hi: 0.7,
            softness_hi: 0.2,
            border_lo: 0.3,
            softness_lo: 0.2,
            brightness: 0.0,
            minmax: 0,
            max: 0,
            blur: 0,
            opacity: 0.5,
            blend_mode: 1,
        }
    }

    /// 明るいところの抜き出し: しきい値より下は消え、上は残る。
    #[test]
    fn the_hi_extract_keeps_the_bright_part() {
        let mut w = W16::new(3, 1);
        // 帯は 0.5..0.7。0.6 を帯の中に置く。
        let mid = MAX16 * 3 / 5;
        w.px = vec![px(1000, MAX16), px(mid, MAX16), px(MAX16, MAX16)];
        extract(&mut w, 0.7, 0.2, true);
        assert_eq!(w.px[0], P16::default(), "暗いところは消える");
        assert_eq!(w.px[2].r, MAX16, "しきい値より上はそのまま");
        assert!(w.px[1].r > 0 && w.px[1].r < mid, "{:?}", w.px[1]);
    }

    /// 暗いところの抜き出しはその逆。
    #[test]
    fn the_lo_extract_keeps_the_dark_part() {
        let mut w = W16::new(3, 1);
        w.px = vec![px(100, MAX16), px(MAX16 * 2 / 5, MAX16), px(MAX16, MAX16)];
        extract(&mut w, 0.3, 0.2, false);
        assert_eq!(w.px[0].r, 100, "暗いところはそのまま");
        assert_eq!(w.px[2], P16::default(), "明るいところは消える");
        assert!(w.px[1].r > 0 && w.px[1].r < MAX16 * 2 / 5);
    }

    /// softness 0 なら帯がないので 0 か素のままの2値になる。
    #[test]
    fn no_softness_is_a_hard_cut() {
        let mut w = W16::new(2, 1);
        w.px = vec![px(MAX16 / 2, MAX16), px(MAX16, MAX16)];
        extract(&mut w, 0.7, 0.0, true);
        assert_eq!(w.px[0], P16::default());
        assert_eq!(w.px[1].r, MAX16);
    }

    /// 縮めて戻すと、だいたい元の大きさの絵が返る (細かいところは失われる)。
    #[test]
    fn half_and_double_come_back_to_size() {
        let mut w = W16::new(8, 4);
        for i in 0..32 {
            w.px[i] = px(MAX16, MAX16);
        }
        to_half(&mut w);
        // 左上の 1/4 に詰まって、残りは 0。
        assert_eq!(w.at(0, 0).r, MAX16);
        assert_eq!(w.at(7, 3), P16::default());
        to_double(&mut w);
        assert_eq!(w.at(0, 0).r, MAX16);
        assert_eq!(w.at(7, 3).r, MAX16);
    }

    /// 膨張は明るい画素を半径ぶん広げる (縮めた側で動く)。
    #[test]
    fn the_max_spreads_the_bright_pixel() {
        let mut w = W16::new(16, 2);
        // 縮めた側の幅は 8。index 4 を明るくする。
        w.px[4] = px(MAX16, MAX16);
        max16(&mut w, 2);
        for x in 2..=6 {
            assert_eq!(w.at(x, 0).r, MAX16, "x = {x}");
        }
        assert_eq!(w.at(1, 0), P16::default());
    }

    /// ぼかしの重みの表は真ん中がいちばん大きく、外へ向かって減る。
    #[test]
    fn the_blur_table_falls_off() {
        let t = blur_table(6);
        assert_eq!(t.len(), 7);
        assert_eq!(t[0], 256);
        for i in 1..7 {
            assert!(t[i] < t[i - 1], "i = {i}");
        }
    }

    /// ぼかしは明るい画素をにじませる。
    #[test]
    fn the_blur_spreads_the_light() {
        let mut w = W16::new(16, 4);
        w.px[4] = px(MAX16, MAX16);
        blur16(&mut w, 3);
        assert!(w.at(4, 0).r < MAX16);
        assert!(w.at(5, 0).r > 0);
    }

    /// 通常合成は不透明度どおりに混ざる。
    #[test]
    fn the_normal_blend_honours_the_opacity() {
        let under = px(0, MAX16);
        let upper = px(MAX16, MAX16);
        let full = blend_pixel(under, upper, MAX16, 1);
        assert_eq!(full.r, MAX16);
        let half = blend_pixel(under, upper, MAX16 / 2, 1);
        assert!(half.r > MAX16 / 2 - 100 && half.r < MAX16 / 2 + 100, "{half:?}");
    }

    /// 不透明度 0 や上が透明なら下地がそのまま返る。
    #[test]
    fn nothing_blends_without_opacity_or_alpha() {
        let under = px(1234, MAX16);
        assert_eq!(blend_pixel(under, px(MAX16, MAX16), 0, 1), under);
        assert_eq!(blend_pixel(under, px(MAX16, 0), MAX16, 1), under);
    }

    /// 加算・乗算・スクリーンの向き。
    #[test]
    fn the_blend_modes_go_the_right_way() {
        let under = px(MAX16 / 2, MAX16);
        let upper = px(MAX16 / 2, MAX16);
        let add = blend_pixel(under, upper, MAX16, 2);
        let mult = blend_pixel(under, upper, MAX16, 6);
        let screen = blend_pixel(under, upper, MAX16, 3);
        assert!(add.r > under.r);
        assert!(mult.r < under.r);
        assert!(screen.r > under.r && screen.r < add.r);
    }

    /// LighterColor / DarkerColor は明るさの合計で画素ごと選ぶ。
    #[test]
    fn the_colour_modes_pick_a_whole_pixel() {
        let dark = P16 { a: MAX16, r: 100, g: 100, b: 100 };
        let bright = P16 { a: MAX16, r: 9000, g: 100, b: 100 };
        assert_eq!(blend_pixel(dark, bright, MAX16, 5).r, 9000);
        assert_eq!(blend_pixel(bright, dark, MAX16, 5).r, 9000);
        assert_eq!(blend_pixel(dark, bright, MAX16, 8).r, 100);
    }

    /// 下地が透明でも LighterColor に不透明度が効く (直したところ 2)。
    #[test]
    fn the_colour_modes_honour_the_opacity_over_nothing() {
        let under = P16 { a: 0, r: 0, g: 0, b: 0 };
        let upper = px(MAX16, MAX16);
        let half = blend_pixel(under, upper, MAX16 / 2, 5);
        assert!(half.a < MAX16, "下地が透明なとき不透明度が効いていない");
        assert!(half.a > 0);
    }

    /// 組を1つ通しても落ちない (抜き出し・膨張・ぼかしの全部を通る)。
    #[test]
    fn a_whole_set_runs_through() {
        let mut w = W16::new(16, 8);
        for i in 0..w.px.len() {
            w.px[i] = px(MAX16 * (i as i32 % 3) / 2, MAX16);
        }
        let mut si = set();
        si.extract_mode = 2;
        si.minmax = 1;
        si.max = 2;
        si.blur = 3;
        si.brightness = 0.1;
        build_set(&mut w, &si);
        assert_eq!(w.px.len(), 16 * 8);
    }

    /// 8bit と 16bit の行き来は値を大きく変えない。
    #[test]
    fn the_depth_round_trip_is_close() {
        for v in [0, 1, 64, 128, 200, 255] {
            assert_eq!(c16to8(c8to16(v)), v, "v = {v}");
        }
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }
}
