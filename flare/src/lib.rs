//! F's Flare の移植。
//!
//! 絵の明るいところ (または白地・黒地のマスク、アルファ) からにじむ光を
//! 作る。光は6段ぶん別々に作れて、それぞれ「膨らませる量 (`max_`)」と
//! 「ぼかす量 (`blur_`)」「色」「重ねかた」を持つ。
//!
//! 手順 (元の Exec08):
//!
//! 1. 出力と下書きを消す
//! 2. `baseOn` が入っていれば、`mode` に応じて元の絵を `baseColor` で
//!    染めたものを出力に置く (光の下地)
//! 3. 光を6段ぶん、上から順に作って重ねる。1段ぶんは:
//!    1. `mode` ごとに入力から下書きを作る (白地なら明るさを反転、
//!       黒地ならそのまま、アルファならアルファ、画像なら色ごと)
//!    2. **下書きを縦横半分に縮める** (ここから先は半分の大きさで動く)
//!    3. `reverse_` が `none` 以外なら赤を反転する
//!    4. `border_` が入っていて `reverse_` が `none` なら、`inside_` と
//!       `outside_` のぶんだけ膨らませた差を取って**縁だけ**にする
//!    5. `max_` のぶん膨らませて、`blur_` のぶんガウスでぼかす
//!    6. `reverseAndOriginalAlpha` なら元のアルファで抜く
//!    7. 赤の濃さを `color_` に載せる
//!    8. 元の大きさへ戻して (倍にして) 出力へ重ねる
//! 4. 乗算を戻す
//!
//! 元から写した細かいところ:
//!
//! - 縮めるのは隣2つの平均 (切り捨て)。戻すのは「自分」と「自分と次の
//!   平均」を交互に置くやりかた
//! - 膨らませるのは「半径のあいだでいちばん明るい値」で、行 (列) の
//!   最大値に届いたら打ち切る。縁は押さえない (範囲の外は見ない)
//! - ぼかしはガウス `exp(-i²/(2(幅/3)²)) × 256` の重み。**値が 0 の
//!   画素は足さずに重みだけ数える**ので、縁がすこし暗くなる
//! - 縁 (`border_`) は「外へ膨らませたもの」から「内へ縮めたもの」を
//!   引いた差。`BorderFin` は緑 - 赤
//! - 重ねかたは screen / add / normal。normal だけ `opacity_` が
//!   もう一度掛かる (下書きを作るときにも掛かっている)
//! - `max_`・`blur_`・`inside_`・`outside_` には downsample が掛かる
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **32bit では何もしていなかった** (`Exec` の分岐が
//!    `case PF_PixelFormat_ARGB128: break;`)。8/16bit と同じ道を通すように
//!    した
//! 2. 元の `FI_*` の既定値の表は `PCOUNT` (6) 個ぶん宣言してあるのに
//!    中身が5個しかない。6段目の既定値は 0 埋めになり、`reverse_6` と
//!    `blend_6` は**ポップアップなのに 0** (1 から始まるはずの番号) に
//!    なる。出荷された .aex もそうなっているので、表示だけの違いとして
//!    そのまま写してある (`blend` が 0 のときは normal 扱い)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Flare

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 光の段の数 (元の PCOUNT)。
const PCOUNT: usize = 6;

/// 1画素。深さに依らず生の目盛りで持つ。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深さごとの目盛りと丸め。
#[derive(Clone, Copy, Debug)]
pub struct Ops {
    pub max: f64,
    pub float: bool,
}

impl Ops {
    /// 元の RoundByteDouble / RoundByteLong (切り捨てて 0..最大値)。
    pub fn clip(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 1.0)
        } else {
            v.clamp(0.0, self.max).trunc()
        }
    }

    /// 元の整数の割り算 (`/ 2`、`/ 3` など)。
    fn div(&self, v: f64, d: f64) -> f64 {
        if self.float {
            v / d
        } else {
            ((v as i64) / (d as i64)) as f64
        }
    }

    /// 2つの平均 (縮める・戻すときの `(c0 + c1) / 2`)。
    fn mid(&self, a: &Px, b: &Px) -> Px {
        Px {
            a: self.div(a.a + b.a, 2.0),
            r: self.div(a.r + b.r, 2.0),
            g: self.div(a.g + b.g, 2.0),
            b: self.div(a.b + b.b, 2.0),
        }
    }
}

/// 画素の並び。半分に縮めたあとも同じ並びのまま左上だけを使う。
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

    fn clear(&mut self) {
        self.px.iter_mut().for_each(|p| *p = Px::default());
    }
}

/// `mode`。元の MODE。
pub const MODE_WHITE: i32 = 1;
pub const MODE_BLACK: i32 = 2;
pub const MODE_ALPHA: i32 = 3;
pub const MODE_IMAGE: i32 = 4;

/// `reverse_`。元の REVMODE。
pub const REV_NONE: i32 = 1;
pub const REV_REVERSE: i32 = 2;
pub const REV_REV_ALPHA: i32 = 3;

/// `blend_`。元の BLEND。
pub const BLEND_SCREEN: i32 = 1;
pub const BLEND_ADD: i32 = 2;
pub const BLEND_NORMAL: i32 = 3;

/// 光1段ぶんのパラメータ。元の FlareInfo。
#[derive(Clone, Copy, Debug, Default)]
pub struct Flare {
    pub enabled: bool,
    pub rev: i32,
    pub border: bool,
    pub inside: i64,
    pub outside: i64,
    pub max: i64,
    pub blur: i64,
    pub colour: Px,
    pub blend: i32,
    pub opacity: f64,
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Clone, Debug)]
pub struct Info {
    pub mode: i32,
    pub base_on: bool,
    pub base_colour: Px,
    pub base_opacity: f64,
    pub flare: [Flare; PCOUNT],
}

/// 元の ToHarfSize。縦横を半分にして、余ったところを消す。
fn to_half(buf: &mut Buf, ops: &Ops) {
    let (ow, oh) = (buf.w, buf.h);
    let (wh, hh) = (ow / 2, oh / 2);
    for y in 0..oh {
        for x in 0..wh {
            let c = ops.mid(&buf.at(2 * x, y), &buf.at(2 * x + 1, y));
            buf.set(x, y, c);
        }
        for x in wh..ow {
            buf.set(x, y, Px::default());
        }
    }
    for x in 0..ow {
        for y in 0..hh {
            let c = ops.mid(&buf.at(x, 2 * y), &buf.at(x, 2 * y + 1));
            buf.set(x, y, c);
        }
        for y in hh..oh {
            buf.set(x, y, Px::default());
        }
    }
}

/// 元の ToDoubleSize。半分の絵を元の大きさへ戻す。
fn to_double(buf: &mut Buf, ops: &Ops) {
    let (ow, oh) = (buf.w, buf.h);
    let (hw, hh) = (ow / 2, oh / 2);
    for y in 0..oh {
        let line: Vec<Px> = (0..hw).map(|x| buf.at(x, y)).collect();
        for x in 0..hw {
            let x1 = x.min(hw - 1);
            let x2 = (x + 1).min(hw - 1);
            let c0 = line[x1 as usize];
            let c2 = if x1 == x2 {
                c0
            } else {
                ops.mid(&c0, &line[x2 as usize])
            };
            buf.set(2 * x, y, c0);
            if 2 * x + 1 < ow {
                buf.set(2 * x + 1, y, c2);
            }
        }
    }
    for x in 0..ow {
        let line: Vec<Px> = (0..hh).map(|y| buf.at(x, y)).collect();
        for y in 0..hh {
            let y1 = y.min(hh - 1);
            let y2 = (y + 1).min(hh - 1);
            let c0 = line[y1 as usize];
            let c2 = if y1 == y2 {
                c0
            } else {
                ops.mid(&c0, &line[y2 as usize])
            };
            buf.set(x, 2 * y, c0);
            if 2 * y + 1 < oh {
                buf.set(x, 2 * y + 1, c2);
            }
        }
    }
}

/// 半分の大きさ (元の `width()/2`, `height()/2`)。
fn half_size(buf: &Buf) -> (i64, i64) {
    (buf.w / 2, buf.h / 2)
}

/// 元の RevRedChannel。赤だけ反転する。
fn rev_red(buf: &mut Buf, ops: &Ops) {
    let (w, h) = half_size(buf);
    for y in 0..h {
        for x in 0..w {
            let mut p = buf.at(x, y);
            p.r = ops.max - p.r;
            buf.set(x, y, p);
        }
    }
}

/// 元の RedToGreen。赤と緑を入れ替える。
fn red_to_green(buf: &mut Buf) {
    let (w, h) = half_size(buf);
    for y in 0..h {
        for x in 0..w {
            let mut p = buf.at(x, y);
            std::mem::swap(&mut p.r, &mut p.g);
            buf.set(x, y, p);
        }
    }
}

/// 元の MaskedApha。元のアルファ (緑に控えてある) のぶんだけ赤を削る。
fn masked_alpha(buf: &mut Buf, ops: &Ops) {
    let (w, h) = half_size(buf);
    for y in 0..h {
        for x in 0..w {
            let mut p = buf.at(x, y);
            let a = ops.max - p.g;
            p.r = ops.clip(p.r - a);
            buf.set(x, y, p);
        }
    }
}

/// 元の Colored。赤の濃さを色に載せる。
fn coloured(buf: &mut Buf, colour: &Px, ops: &Ops) {
    let (w, h) = half_size(buf);
    for y in 0..h {
        for x in 0..w {
            let p = buf.at(x, y);
            let aa = p.r / ops.max;
            buf.set(
                x,
                y,
                Px {
                    a: p.r,
                    r: ops.clip(colour.r * aa),
                    g: ops.clip(colour.g * aa),
                    b: ops.clip(colour.b * aa),
                },
            );
        }
    }
}

/// 元の BorderFin。緑 - 赤。
fn border_fin(buf: &mut Buf, ops: &Ops) {
    let (w, h) = half_size(buf);
    for y in 0..h {
        for x in 0..w {
            let mut p = buf.at(x, y);
            p.r = ops.clip(p.g - p.r);
            buf.set(x, y, p);
        }
    }
}

/// 元の MaxRedHor / MaxRedVur。半径のあいだでいちばん明るい赤。
fn max_red(buf: &mut Buf, max: i64, vert: bool) {
    let (w, h) = half_size(buf);
    let (outer, inner) = if vert { (w, h) } else { (h, w) };
    for o in 0..outer {
        let line: Vec<f64> = (0..inner)
            .map(|i| if vert { buf.at(o, i).r } else { buf.at(i, o).r })
            .collect();
        let mv = line.iter().cloned().fold(0.0f64, f64::max);
        if mv <= 0.0 {
            continue;
        }
        for i in 0..inner {
            let mut t = 0.0f64;
            for k in -max..=max {
                let j = i + k;
                if vert {
                    // 元の縦は範囲の外を読み飛ばす。
                    if j < 0 || j >= inner {
                        continue;
                    }
                } else {
                    // 元の横は左は読み飛ばし、右に出たら打ち切る。
                    if j < 0 {
                        continue;
                    }
                    if j >= inner {
                        break;
                    }
                }
                if t < line[j as usize] {
                    t = line[j as usize];
                }
                if t >= mv {
                    break;
                }
            }
            let (x, y) = if vert { (o, i) } else { (i, o) };
            let mut p = buf.at(x, y);
            p.r = t;
            buf.set(x, y, p);
        }
    }
}

/// 元の MaxAllHor / MaxAllVur。明るさで選んで画素ごと置き換える。
fn max_all(buf: &mut Buf, max: i64, vert: bool, ops: &Ops) {
    let (w, h) = half_size(buf);
    let (outer, inner) = if vert { (w, h) } else { (h, w) };
    for o in 0..outer {
        let line: Vec<Px> = (0..inner)
            .map(|i| if vert { buf.at(o, i) } else { buf.at(i, o) })
            .collect();
        let lv: Vec<f64> = line
            .iter()
            .map(|p| ops.clip(ops.div(p.r + p.g + p.b, 3.0)))
            .collect();
        let mv = lv.iter().cloned().fold(0.0f64, f64::max);
        if mv <= 0.0 {
            continue;
        }
        for i in 0..inner {
            let mut t = 0.0f64;
            let mut c = Px::default();
            for k in -max..=max {
                let j = i + k;
                if vert {
                    if j < 0 || j >= inner {
                        continue;
                    }
                } else {
                    if j < 0 {
                        continue;
                    }
                    if j >= inner {
                        break;
                    }
                }
                if t < lv[j as usize] {
                    t = lv[j as usize];
                    c = line[j as usize];
                }
                if t >= mv {
                    break;
                }
            }
            let (x, y) = if vert { (o, i) } else { (i, o) };
            buf.set(x, y, c);
        }
    }
}

/// 元の MakeTbl。ガウスの重み (0..256)。
fn make_tbl(blur: i64) -> Vec<f64> {
    let zone = blur as f64 / 3.0;
    (0..=blur)
        .map(|i| ((-(i * i) as f64 / (2.0 * zone * zone)).exp() * 256.0) as i64 as f64)
        .collect()
}

/// 元の BlurRedHor / BlurRedVur。赤だけぼかす。
fn blur_red(buf: &mut Buf, blur: i64, tbl: &[f64], vert: bool, ops: &Ops) {
    let (w, h) = half_size(buf);
    let (outer, inner) = if vert { (w, h) } else { (h, w) };
    for o in 0..outer {
        let line: Vec<f64> = (0..inner)
            .map(|i| if vert { buf.at(o, i).r } else { buf.at(i, o).r })
            .collect();
        for i in 0..inner {
            let mut count = 0.0f64;
            let mut r = 0.0f64;
            let g0 = tbl[0];
            if line[i as usize] > 0.0 {
                r += line[i as usize] * g0;
            }
            count += g0;
            for k in 1..=blur {
                let gauss = tbl[k as usize];
                let il = i - k;
                if il >= 0 {
                    if line[il as usize] > 0.0 {
                        r += line[il as usize] * gauss;
                    }
                    count += gauss;
                }
                let ir = i + k;
                if ir < inner {
                    if line[ir as usize] > 0.0 {
                        r += line[ir as usize] * gauss;
                    }
                    count += gauss;
                }
            }
            if count > 0.0 {
                let (x, y) = if vert { (o, i) } else { (i, o) };
                let mut p = buf.at(x, y);
                p.r = ops.clip(ops.div(r, count));
                buf.set(x, y, p);
            }
        }
    }
}

/// 元の BlurAllHor / BlurAllVur。画素ごとぼかす (アルファが 0 のところは
/// 足さずに重みだけ数える)。
fn blur_all(buf: &mut Buf, blur: i64, tbl: &[f64], vert: bool, ops: &Ops) {
    let (w, h) = half_size(buf);
    let (outer, inner) = if vert { (w, h) } else { (h, w) };
    for o in 0..outer {
        let line: Vec<Px> = (0..inner)
            .map(|i| if vert { buf.at(o, i) } else { buf.at(i, o) })
            .collect();
        for i in 0..inner {
            let mut count = 0.0f64;
            let mut s = Px::default();
            let add = |p: &Px, gauss: f64, s: &mut Px| {
                if p.a > 0.0 {
                    s.r += p.r * gauss;
                    s.g += p.g * gauss;
                    s.b += p.b * gauss;
                    s.a += p.a * gauss;
                }
            };
            add(&line[i as usize], tbl[0], &mut s);
            count += tbl[0];
            for k in 1..=blur {
                let gauss = tbl[k as usize];
                let il = i - k;
                if il >= 0 {
                    add(&line[il as usize], gauss, &mut s);
                    count += gauss;
                }
                let ir = i + k;
                if ir < inner {
                    add(&line[ir as usize], gauss, &mut s);
                    count += gauss;
                }
            }
            if count > 0.0 {
                let (x, y) = if vert { (o, i) } else { (i, o) };
                buf.set(
                    x,
                    y,
                    Px {
                        a: ops.clip(ops.div(s.a, count)),
                        r: ops.clip(ops.div(s.r, count)),
                        g: ops.clip(ops.div(s.g, count)),
                        b: ops.clip(ops.div(s.b, count)),
                    },
                );
            }
        }
    }
}

/// 元の Border08。外へ膨らませたものから内へ縮めたものを引く。
fn border(buf: &mut Buf, f: &Flare, ops: &Ops) {
    if f.outside > 0 {
        max_red(buf, f.outside, false);
        max_red(buf, f.outside, true);
    }
    red_to_green(buf);
    if f.inside > 0 {
        rev_red(buf, ops);
        max_red(buf, f.inside, false);
        max_red(buf, f.inside, true);
        rev_red(buf, ops);
    }
    border_fin(buf, ops);
}

/// 下地 (元の BaseCopy*)。`mode` ごとに入力を染める。
fn base_copy(inp: &Px, info: &Info, ops: &Ops) -> Px {
    let m = ops.max;
    match info.mode {
        MODE_IMAGE => {
            let a = (inp.a / m) * info.base_opacity;
            Px {
                a: ops.clip(m * a),
                r: ops.clip(inp.r * a),
                g: ops.clip(inp.g * a),
                b: ops.clip(inp.b * a),
            }
        }
        MODE_ALPHA => {
            if inp.a <= 0.0 {
                Px::default()
            } else if inp.a >= m && info.base_opacity >= 1.0 {
                info.base_colour
            } else {
                let a = (inp.a / m) * info.base_opacity;
                Px {
                    a: ops.clip(m * a),
                    r: ops.clip(info.base_colour.r * a),
                    g: ops.clip(info.base_colour.g * a),
                    b: ops.clip(info.base_colour.b * a),
                }
            }
        }
        _ => {
            // 白地と黒地。白地は明るさを反転する。
            if inp.a <= 0.0 {
                return Px::default();
            }
            let a = ((inp.a / m) * info.base_opacity).clamp(0.0, 1.0);
            let mut rgb = ops.div(inp.r + inp.g + inp.b, 3.0);
            if info.mode != MODE_BLACK {
                rgb = m - rgb;
            }
            rgb = rgb.clamp(0.0, m);
            if a >= 1.0 {
                Px {
                    a: rgb,
                    r: ops.div(rgb * info.base_colour.r, m),
                    g: ops.div(rgb * info.base_colour.g, m),
                    b: ops.div(rgb * info.base_colour.b, m),
                }
            } else {
                let v = rgb * a;
                Px {
                    a: ops.clip(v),
                    r: ops.clip(v * info.base_colour.r / m),
                    g: ops.clip(v * info.base_colour.g / m),
                    b: ops.clip(v * info.base_colour.b / m),
                }
            }
        }
    }
}

/// 1段ぶんの下書き (元の WhiteBase / BlackBase / AlphaBase / ImageBase)。
fn flare_base(inp: &Px, mode: i32, f: &Flare, ops: &Ops) -> Px {
    let m = ops.max;
    match mode {
        MODE_IMAGE => {
            let a = (inp.a / m) * f.opacity;
            if a <= 0.0 {
                Px::default()
            } else {
                Px {
                    a: ops.clip(inp.a * a),
                    r: ops.clip(inp.r * a),
                    g: ops.clip(inp.g * a),
                    b: ops.clip(inp.b * a),
                }
            }
        }
        MODE_ALPHA => {
            if inp.a <= 0.0 {
                Px::default()
            } else if inp.a >= m && f.opacity >= 1.0 {
                Px {
                    a: m,
                    r: m,
                    g: m,
                    b: m,
                }
            } else {
                let a = ops.clip(inp.a * f.opacity);
                Px {
                    a,
                    r: a,
                    g: a,
                    b: a,
                }
            }
        }
        _ => {
            if inp.a <= 0.0 {
                return Px::default();
            }
            let a = ((inp.a / m) * f.opacity).clamp(0.0, 1.0);
            let mut rgb = ops.div(inp.r + inp.g + inp.b, 3.0);
            if mode != MODE_BLACK {
                rgb = m - rgb;
            }
            rgb = rgb.clamp(0.0, m);
            let v = if a >= 1.0 { rgb } else { ops.clip(rgb * a) };
            Px {
                a: v,
                r: v,
                g: v,
                b: v,
            }
        }
    }
}

/// 元の BlendScreen / BlendAdd / BlendNormal。下書きを出力へ重ねる。
fn blend(out: &mut Px, src: &Px, f: &Flare, ops: &Ops) {
    let m = ops.max;
    match f.blend {
        BLEND_SCREEN => {
            let scr = |s: f64, d: f64| ops.clip(s + d - ops.div(s * d, m));
            out.r = scr(src.r, out.r);
            out.g = scr(src.g, out.g);
            out.b = scr(src.b, out.b);
            out.a = scr(src.a, out.a);
        }
        BLEND_ADD => {
            out.r = ops.clip(src.r + out.r);
            out.g = ops.clip(src.g + out.g);
            out.b = ops.clip(src.b + out.b);
            out.a = ops.clip(src.a + out.a);
        }
        _ => {
            // normal。元は opacity をもう一度掛ける。
            let a = src.a * f.opacity;
            if a == 0.0 {
                return;
            }
            if a >= m {
                *out = *src;
            } else {
                let k = 1.0 - a / m;
                out.a = ops.clip(out.a * k + a);
                out.r = ops.clip(out.r * k + src.r * f.opacity);
                out.g = ops.clip(out.g * k + src.g * f.opacity);
                out.b = ops.clip(out.b * k + src.b * f.opacity);
            }
        }
    }
}

/// 元の fromBlackMat。黒マットの乗算を戻す。
fn from_black_mat(out: &mut Buf, ops: &Ops) {
    let m = ops.max;
    for p in out.px.iter_mut() {
        if p.a >= m {
            continue;
        }
        if p.a <= 0.0 {
            p.r = 0.0;
            p.g = 0.0;
            p.b = 0.0;
        } else if ops.float {
            p.r = ops.clip(p.r / p.a);
            p.g = ops.clip(p.g / p.a);
            p.b = ops.clip(p.b / p.a);
        } else {
            let mm = m as i64;
            let a = p.a as i64;
            p.r = ((p.r as i64 * mm) / a).clamp(0, mm) as f64;
            p.g = ((p.g as i64 * mm) / a).clamp(0, mm) as f64;
            p.b = ((p.b as i64 * mm) / a).clamp(0, mm) as f64;
        }
    }
}

/// 元の Exec08。手順ぜんぶ。
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    out.clear();
    let mut tmp = Buf::new(inp.w, inp.h);
    if info.base_on {
        for i in 0..out.px.len() {
            out.px[i] = base_copy(&inp.px[i], info, ops);
        }
    }
    for f in info.flare.iter() {
        tmp.clear();
        if !f.enabled || f.opacity <= 0.0 {
            continue;
        }
        for i in 0..tmp.px.len() {
            tmp.px[i] = flare_base(&inp.px[i], info.mode, f, ops);
        }
        to_half(&mut tmp, ops);
        if f.rev != REV_NONE && info.mode != MODE_IMAGE {
            rev_red(&mut tmp, ops);
        }
        if f.border && info.mode != MODE_IMAGE && f.rev == REV_NONE {
            border(&mut tmp, f, ops);
        }
        if info.mode == MODE_IMAGE {
            if f.max != 0 {
                max_all(&mut tmp, f.max, false, ops);
                max_all(&mut tmp, f.max, true, ops);
            }
            if f.blur != 0 {
                let b = f.blur.max(1);
                let tbl = make_tbl(b);
                blur_all(&mut tmp, b, &tbl, false, ops);
                blur_all(&mut tmp, b, &tbl, true, ops);
            }
        } else {
            if f.max != 0 {
                max_red(&mut tmp, f.max, false);
                max_red(&mut tmp, f.max, true);
            }
            if f.blur != 0 {
                let b = f.blur.max(1);
                let tbl = make_tbl(b);
                blur_red(&mut tmp, b, &tbl, false, ops);
                blur_red(&mut tmp, b, &tbl, true, ops);
            }
            if f.rev == REV_REV_ALPHA {
                masked_alpha(&mut tmp, ops);
            }
            coloured(&mut tmp, &f.colour, ops);
        }
        to_double(&mut tmp, ops);
        for i in 0..out.px.len() {
            let s = tmp.px[i];
            blend(&mut out.px[i], &s, f, ops);
        }
    }
    from_black_mat(out, ops);
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
    BaseOn,
    BaseColor,
    BaseOpacity,
    // 6段ぶん × 14。
    Topic(u8),
    Enabled(u8),
    Rev(u8),
    BorderTopic(u8),
    Border(u8),
    Inside(u8),
    Outside(u8),
    BorderTopicEnd(u8),
    Max(u8),
    Blur(u8),
    Color(u8),
    Blend(u8),
    Opacity(u8),
    TopicEnd(u8),
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// 元の FI_* の既定値の表。6段目は元が 0 埋めのまま。
const FI_ENABLED: [bool; PCOUNT] = [false, false, false, false, false, false];
const FI_REV: [i32; PCOUNT] = [1, 1, 1, 1, 1, 0];
const FI_BORDER: [bool; PCOUNT] = [false, false, false, true, false, false];
const FI_INSIDE: [i32; PCOUNT] = [1, 1, 1, 2, 1, 0];
const FI_OUTSIDE: [i32; PCOUNT] = [1, 1, 1, 1, 1, 0];
const FI_MAX: [i32; PCOUNT] = [10, 3, 1, 0, 0, 0];
const FI_BLUR: [i32; PCOUNT] = [70, 20, 5, 8, 0, 0];
/// (使わないアルファ, 赤, 緑, 青)。
const FI_COLOR: [(u8, u8, u8, u8); PCOUNT] = [
    (0xFF, 0xFE, 0xFF, 0xA2),
    (0xFF, 0xFF, 0xF5, 0x51),
    (0xFF, 0xFF, 0xF5, 0x51),
    (0xFF, 0xCC, 0xFF, 0x1A),
    (0xFF, 0xFF, 0xFF, 0xFF),
    (0, 0, 0, 0),
];
const FI_BLEND: [i32; PCOUNT] = [1, 1, 1, 3, 1, 0];
const FI_OPACITY: [f64; PCOUNT] = [70.0, 80.0, 100.0, 50.0, 100.0, 0.0];

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
        // 元: PF_ADD_POPUP("mode", 4, 3, "WhitebackMask|BlackbackMask|Alpha|Image")
        params.add(
            Params::Mode,
            "mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["WhitebackMask", "BlackbackMask", "Alpha", "Image"]);
                f.set_default(3);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::BaseOn,
            "baseOn",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("baseColor", 0xF4, 0xFF, 0x95)
        params.add(
            Params::BaseColor,
            "baseColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xF4,
                    green: 0xFF,
                    blue: 0x95,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::BaseOpacity,
            "baseOpacity",
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
        for i in 0..PCOUNT {
            let n = i as u8;
            let j = i + 1;
            params.add_group(
                Params::Topic(n),
                Params::TopicEnd(n),
                &format!("flare_{j}"),
                true,
                |p| {
                    let en = FI_ENABLED[i];
                    p.add(
                        Params::Enabled(n),
                        &format!("enabled_{j}"),
                        ae::CheckBoxDef::setup(move |f| {
                            f.set_label("on");
                            f.set_default(en);
                            f.set_value(f.default());
                        }),
                    )?;
                    let rv = FI_REV[i];
                    p.add(
                        Params::Rev(n),
                        &format!("reverse(except_image)_{j}"),
                        ae::PopupDef::setup(move |f| {
                            f.set_options(&["none", "reverse", "reverseAndOriginalAlpha"]);
                            f.set_default(rv);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add_group(
                        Params::BorderTopic(n),
                        Params::BorderTopicEnd(n),
                        &format!("border(except_image)_{j}"),
                        true,
                        |q| {
                            let bd = FI_BORDER[i];
                            q.add(
                                Params::Border(n),
                                &format!("border_{j}"),
                                ae::CheckBoxDef::setup(move |f| {
                                    f.set_label("on");
                                    f.set_default(bd);
                                    f.set_value(f.default());
                                }),
                            )?;
                            let ins = FI_INSIDE[i];
                            q.add(
                                Params::Inside(n),
                                &format!("inside_{j}"),
                                ae::SliderDef::setup(move |f| {
                                    f.set_valid_min(0);
                                    f.set_valid_max(20);
                                    f.set_slider_min(0);
                                    f.set_slider_max(10);
                                    f.set_default(ins);
                                    f.set_value(f.default());
                                }),
                            )?;
                            let out = FI_OUTSIDE[i];
                            q.add(
                                Params::Outside(n),
                                &format!("outside_{j}"),
                                ae::SliderDef::setup(move |f| {
                                    f.set_valid_min(0);
                                    f.set_valid_max(20);
                                    f.set_slider_min(0);
                                    f.set_slider_max(20);
                                    f.set_default(out);
                                    f.set_value(f.default());
                                }),
                            )?;
                            Ok(())
                        },
                    )?;
                    let mx = FI_MAX[i];
                    p.add(
                        Params::Max(n),
                        &format!("max_{j}"),
                        ae::SliderDef::setup(move |f| {
                            f.set_valid_min(0);
                            f.set_valid_max(100);
                            f.set_slider_min(0);
                            f.set_slider_max(20);
                            f.set_default(mx);
                            f.set_value(f.default());
                        }),
                    )?;
                    let bl = FI_BLUR[i];
                    p.add(
                        Params::Blur(n),
                        &format!("blur_{j}"),
                        ae::SliderDef::setup(move |f| {
                            f.set_valid_min(0);
                            f.set_valid_max(100);
                            f.set_slider_min(0);
                            f.set_slider_max(50);
                            f.set_default(bl);
                            f.set_value(f.default());
                        }),
                    )?;
                    let c = FI_COLOR[i];
                    p.add(
                        Params::Color(n),
                        &format!("color(except_image)_{j}"),
                        ae::ColorDef::setup(move |f| {
                            // 元の PF_ADD_COLOR は RGB だけを取るので、
                            // アルファはいつも 255 になる (6段目も同じ)。
                            f.set_default(ae::Pixel8 {
                                alpha: 0xFF,
                                red: c.1,
                                green: c.2,
                                blue: c.3,
                            });
                            f.set_value(f.default());
                        }),
                    )?;
                    let bn = FI_BLEND[i];
                    p.add(
                        Params::Blend(n),
                        &format!("blend_{j}"),
                        ae::PopupDef::setup(move |f| {
                            f.set_options(&["screen", "add", "normal"]);
                            f.set_default(bn);
                            f.set_value(f.default());
                        }),
                    )?;
                    let op = FI_OPACITY[i];
                    p.add(
                        Params::Opacity(n),
                        &format!("opacity_{j}"),
                        ae::FloatSliderDef::setup(move |f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(op);
                            f.set_value(f.default());
                        }),
                    )?;
                    Ok(())
                },
            )?;
        }
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
                out_data.set_return_msg("fs-rs flare");
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
        if FAITHFUL && depth > 16 {
            // 元は 32bit で何もしない。
            return Ok(());
        }
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };
        let ds = in_data.downsample_x();
        // 元の downScaleNoClip。
        let down = |v: i64| -> i64 { v * ds.num as i64 / ds.den as i64 };
        let to_depth = |c: ae::Pixel8| -> Px {
            match depth {
                8 => Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                },
                16 => {
                    let q = libs::utils::conv_8_to_16(&c);
                    Px {
                        a: q.alpha as f64,
                        r: q.red as f64,
                        g: q.green as f64,
                        b: q.blue as f64,
                    }
                }
                _ => {
                    let q = libs::utils::conv_8_to_32(&c);
                    Px {
                        a: q.alpha as f64,
                        r: q.red as f64,
                        g: q.green as f64,
                        b: q.blue as f64,
                    }
                }
            }
        };

        // 元の GetParams。
        let mode = params.get(Params::Mode)?.as_popup()?.value();
        let base_on = params.get(Params::BaseOn)?.as_checkbox()?.value();
        let base_colour = to_depth(params.get(Params::BaseColor)?.as_color()?.value());
        let base_opacity = (params.get(Params::BaseOpacity)?.as_float_slider()?.value() as f64
            / 100.0)
            .clamp(0.0, 1.0);
        let mut flare = [Flare::default(); PCOUNT];
        for i in 0..PCOUNT {
            let n = i as u8;
            flare[i] = Flare {
                enabled: params.get(Params::Enabled(n))?.as_checkbox()?.value(),
                rev: params.get(Params::Rev(n))?.as_popup()?.value(),
                border: params.get(Params::Border(n))?.as_checkbox()?.value(),
                inside: down(params.get(Params::Inside(n))?.as_slider()?.value() as i64),
                outside: down(params.get(Params::Outside(n))?.as_slider()?.value() as i64),
                max: down(params.get(Params::Max(n))?.as_slider()?.value() as i64),
                blur: down(params.get(Params::Blur(n))?.as_slider()?.value() as i64),
                colour: to_depth(params.get(Params::Color(n))?.as_color()?.value()),
                blend: params.get(Params::Blend(n))?.as_popup()?.value(),
                opacity: (params.get(Params::Opacity(n))?.as_float_slider()?.value() as f64
                    / 100.0)
                    .clamp(0.0, 1.0),
            };
        }
        let info = Info {
            mode,
            base_on,
            base_colour,
            base_opacity,
            flare,
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
                exec(&inp, &mut out, &info, &ops);
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

    fn white(v: f64) -> Px {
        Px {
            a: 255.0,
            r: v,
            g: v,
            b: v,
        }
    }

    fn info(mode: i32) -> Info {
        Info {
            mode,
            base_on: false,
            base_colour: white(255.0),
            base_opacity: 1.0,
            flare: [Flare::default(); PCOUNT],
        }
    }

    fn one(mode: i32, f: Flare) -> Info {
        let mut i = info(mode);
        i.flare[0] = f;
        i
    }

    fn flare() -> Flare {
        Flare {
            enabled: true,
            rev: REV_NONE,
            border: false,
            inside: 0,
            outside: 0,
            max: 0,
            blur: 0,
            colour: white(255.0),
            blend: BLEND_SCREEN,
            opacity: 1.0,
        }
    }

    /// 半分に縮めて戻すと、だいたい元の大きさに収まる。
    #[test]
    fn the_half_and_double_round_trip() {
        let o = ops8();
        let mut b = Buf::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                b.set(x, y, white(if x < 4 { 255.0 } else { 0.0 }));
            }
        }
        to_half(&mut b, &o);
        assert_eq!(b.at(0, 0).r, 255.0);
        assert_eq!(b.at(3, 0).r, 0.0, "右半分は 0 のまま");
        to_double(&mut b, &o);
        assert_eq!(b.at(0, 0).r, 255.0);
        assert_eq!(b.at(7, 0).r, 0.0);
    }

    /// 膨らませると明るいところが広がる。
    #[test]
    fn the_max_spreads_the_bright_spot() {
        let mut b = Buf::new(16, 16);
        b.set(4, 4, white(255.0));
        max_red(&mut b, 2, false);
        assert_eq!(b.at(6, 4).r, 255.0, "横に広がる");
        assert_eq!(b.at(4, 6).r, 0.0, "縦はまだ");
        max_red(&mut b, 2, true);
        assert_eq!(b.at(4, 6).r, 255.0, "縦にも広がる");
    }

    /// ぼかすと広がって薄くなる。
    #[test]
    fn the_blur_softens_the_spot() {
        let o = ops8();
        let mut b = Buf::new(16, 16);
        b.set(4, 4, white(255.0));
        let tbl = make_tbl(3);
        blur_red(&mut b, 3, &tbl, false, &o);
        assert!(b.at(4, 4).r < 255.0 && b.at(4, 4).r > 0.0);
        assert!(b.at(5, 4).r > 0.0, "隣へ漏れる");
        assert_eq!(b.at(4, 5).r, 0.0, "縦はまだ");
    }

    /// 白地のときは明るいところではなく**暗いところ**が光る。
    #[test]
    fn the_white_base_lights_the_dark_side() {
        let o = ops8();
        let f = flare();
        let dark = flare_base(&white(0.0), MODE_WHITE, &f, &o);
        let light = flare_base(&white(255.0), MODE_WHITE, &f, &o);
        assert_eq!(dark.r, 255.0);
        assert_eq!(light.r, 0.0);
    }

    /// 黒地はそのまま、アルファはアルファだけを見る。
    #[test]
    fn the_other_modes_read_what_they_say() {
        let o = ops8();
        let f = flare();
        assert_eq!(flare_base(&white(255.0), MODE_BLACK, &f, &o).r, 255.0);
        let half = Px {
            a: 128.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        assert_eq!(flare_base(&half, MODE_ALPHA, &f, &o).r, 128.0);
        assert_eq!(flare_base(&half, MODE_BLACK, &f, &o).r, 0.0);
    }

    /// 光は色に染まる。
    #[test]
    fn the_flare_takes_its_colour() {
        let o = ops8();
        let mut b = Buf::new(4, 4);
        b.set(0, 0, white(255.0));
        coloured(
            &mut b,
            &Px {
                a: 255.0,
                r: 255.0,
                g: 128.0,
                b: 0.0,
            },
            &o,
        );
        assert_eq!(b.at(0, 0).r, 255.0);
        assert_eq!(b.at(0, 0).g, 128.0);
        assert_eq!(b.at(0, 0).b, 0.0);
        assert_eq!(b.at(0, 0).a, 255.0);
    }

    /// ひととおり通して、光が出力に乗る。
    #[test]
    fn the_whole_run_puts_light_on_the_output() {
        let o = ops8();
        let mut f = flare();
        f.max = 2;
        f.blur = 2;
        let i = one(MODE_BLACK, f);
        let mut inp = Buf::new(32, 32);
        for y in 14..18 {
            for x in 14..18 {
                inp.set(x, y, white(255.0));
            }
        }
        let mut out = Buf::new(32, 32);
        exec(&inp, &mut out, &i, &o);
        assert!(out.at(16, 16).r > 0.0, "真ん中は光る");
        assert!(out.at(20, 16).r > 0.0, "まわりへも広がる");
        assert_eq!(out.at(0, 0).r, 0.0, "遠くは暗い");
    }

    /// 段を切ってあれば何も出ない。
    #[test]
    fn a_disabled_flare_draws_nothing() {
        let o = ops8();
        let mut f = flare();
        f.enabled = false;
        let i = one(MODE_BLACK, f);
        let mut inp = Buf::new(16, 16);
        inp.set(8, 8, white(255.0));
        let mut out = Buf::new(16, 16);
        exec(&inp, &mut out, &i, &o);
        assert!(out.px.iter().all(|p| p.a == 0.0));
    }
}
