// ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MaxBlur
//
//! `MaxBlur` / `InnerFlare` / `InnerFlareRot` が共通で持っている Tiny_* の
//! 部品: 乗算の往復、van Herk / Gil-Werman の最大最小、箱ぼかし3回、
//! アルファの双曲線カーブ。AE の型に依存しない (画素は f64 で持つ)。
//!
//! 元は3つのプロジェクトに同じファイルが複製されていて、**明るさの式から
//! `*10` が落ちている場所がプロジェクトごとに違う**。どこが落ちているかは
//! `Quirk` で選ぶ (元との画素比較のためだけにある)。

use crate::minmax_vhgw::{level, line_pass};

/// 元の `lineLevel` から `*10` を外したもの。
fn level_no10(r: f64, g: f64, b: f64, a: f32, max_chan: f32) -> f32 {
    ((r * 0.299 + g * 0.587 + b * 0.114) + (a / max_chan) as f64) as f32
}

/// 縦の最大最小で、元が `*10` を落としている場所。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quirk {
    /// 落ちていない (直した版)。
    None,
    /// `MaxBlur`: 窓が高さ以上の枝で、2番目以降だけ落ちている。
    /// 先頭だけ 10 倍のまま比べるので、ほぼ必ず列の先頭が勝つ。
    WholeNo10,
    /// `InnerFlare`: 窓が高さより狭い枝 (ふつうの経路) の明るさの表が
    /// 丸ごと落ちている。RGB の差が小さいところでアルファの重みが
    /// 10倍になり、勝つ画素が変わる。
    BlockNo10,
}

/// 1画素。深度に依らず f64 で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 画素の並び。
pub struct Buf {
    pub w: usize,
    pub h: usize,
    pub px: Vec<Px>,
    /// 8bit なら 255、16bit なら 32768、32bit なら 1.0。
    pub max: f64,
    pub float: bool,
}

impl Buf {
    pub fn at(&self, o: usize, i: usize, vert: bool) -> usize {
        if vert {
            i * self.w + o
        } else {
            o * self.w + i
        }
    }
    /// 元の static_cast<CH>(値) 相当。0..max で押さえてから落とす。
    pub fn cast(&self, v: f64) -> f64 {
        let v = v.clamp(0.0, self.max);
        if self.float {
            v
        } else {
            v.trunc()
        }
    }
}

/// 元の TinyMultM。`un` が立っていれば乗算を戻す。
pub fn mult(buf: &mut Buf, un: bool) {
    let max = buf.max;
    let float = buf.float;
    for p in buf.px.iter_mut() {
        if p.a <= 0.0 {
            p.r = 0.0;
            p.g = 0.0;
            p.b = 0.0;
            continue;
        }
        if p.a >= max {
            continue;
        }
        let v = if un { max / p.a } else { p.a / max };
        let f = |x: f64| {
            let x = (x * v).clamp(0.0, max);
            if float {
                x
            } else {
                x.trunc()
            }
        };
        p.r = f(p.r);
        p.g = f(p.g);
        p.b = f(p.b);
    }
}

/// 元の Max_SubH / Max_SubV (Tiny_MinMax)。`MaxFast` と同じ実装。
pub fn minmax(buf: &mut Buf, radius: i64, is_max: bool, quirk: Quirk) {
    if radius == 0 {
        return;
    }
    let radius = radius.abs();
    let max = buf.max as f32;
    for vert in [false, true] {
        let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
        let mut src: Vec<Px> = Vec::with_capacity(inner);
        let mut levels: Vec<f32> = Vec::with_capacity(inner);
        let mut dst: Vec<Px> = Vec::with_capacity(inner);
        for o in 0..outer {
            src.clear();
            levels.clear();
            for i in 0..inner {
                let p = buf.px[buf.at(o, i, vert)];
                levels.push(level(p.r, p.g, p.b, p.a as f32, max));
                src.push(p);
            }
            // 元が *10 を落としている場所を真似る (Quirk 参照)。
            if vert {
                match quirk {
                    Quirk::None => {}
                    Quirk::WholeNo10 if radius >= inner as i64 => {
                        for i in 1..inner {
                            let p = src[i];
                            levels[i] = level_no10(p.r, p.g, p.b, p.a as f32, max);
                        }
                    }
                    Quirk::BlockNo10 if radius < inner as i64 => {
                        for i in 0..inner {
                            let p = src[i];
                            levels[i] = level_no10(p.r, p.g, p.b, p.a as f32, max);
                        }
                    }
                    _ => {}
                }
            }
            dst.clear();
            dst.extend_from_slice(&src);
            line_pass(&src, &levels, &mut dst, radius, is_max);
            for i in 0..inner {
                let idx = buf.at(o, i, vert);
                buf.px[idx] = dst[i];
            }
        }
    }
}

/// 元の BoxBlurH / BoxBlurV。縁は端の画素を伸ばす。
pub fn box_pass(buf: &mut Buf, vert: bool, r: i64) {
    let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
    let n = inner as i64;
    let window = (r * 2 + 1) as f64;
    let mut tmp: Vec<Px> = Vec::with_capacity(inner);
    for o in 0..outer {
        let get = |i: i64| -> Px { buf.px[buf.at(o, i.clamp(0, n - 1) as usize, vert)] };
        let mut s = Px::default();
        for i in -r..=r {
            let p = get(i);
            s.a += p.a;
            s.r += p.r;
            s.g += p.g;
            s.b += p.b;
        }
        tmp.clear();
        for i in 0..n {
            tmp.push(Px {
                a: buf.cast(s.a / window),
                r: buf.cast(s.r / window),
                g: buf.cast(s.g / window),
                b: buf.cast(s.b / window),
            });
            let l = get(i - r);
            let rr = get(i + r + 1);
            s.a += rr.a - l.a;
            s.r += rr.r - l.r;
            s.g += rr.g - l.g;
            s.b += rr.b - l.b;
        }
        for i in 0..inner {
            let idx = buf.at(o, i, vert);
            buf.px[idx] = tmp[i];
        }
    }
}

/// 元の TinyBlueM。横→縦を3回。
pub fn box_blur(buf: &mut Buf, r: i64) {
    if r <= 0 {
        return;
    }
    for _ in 0..3 {
        box_pass(buf, false, r);
        box_pass(buf, true, r);
    }
}

/// 元の HyperbolicAlpha。正の `k` では反転してから掛けて戻す。
pub fn hyperbolic(buf: &mut Buf, k: f64) {
    if k == 0.0 {
        return;
    }
    let invert = k >= 0.0;
    let k = k.abs() as f32;
    let max = buf.max;
    let float = buf.float;
    for p in buf.px.iter_mut() {
        if p.a <= 0.0 || p.a >= max {
            continue;
        }
        let mut t = (p.a / max) as f32;
        t = t.clamp(0.0, 1.0);
        if invert {
            t = 1.0 - t;
        }
        let mut v = ((1.0 + k) * t) / (1.0 + k * t);
        v = v.clamp(0.0, 1.0);
        if invert {
            v = 1.0 - v;
        }
        p.a = if float {
            v as f64
        } else {
            (v as f64 * max).trunc()
        };
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn white() -> Px {
        Px { a: 255.0, r: 255.0, g: 255.0, b: 255.0 }
    }

    fn buf(w: usize, h: usize) -> Buf {
        Buf {
            w,
            h,
            px: vec![Px::default(); w * h],
            max: 255.0,
            float: false,
        }
    }

    /// 乗算と乗算戻し。アルファ 0 は RGB も 0 に。
    #[test]
    fn the_premultiply_clears_transparent_pixels() {
        let mut b = buf(2, 1);
        b.px[0] = Px {
            a: 0.0,
            r: 200.0,
            g: 100.0,
            b: 50.0,
        };
        b.px[1] = white();
        mult(&mut b, false);
        assert_eq!(b.px[0].r, 0.0);
        assert_eq!(b.px[1], white(), "不透明はそのまま");
    }

    /// 半透明は乗算で暗くなり、戻すとほぼ元へ (切り捨てぶんだけ下がる)。
    #[test]
    fn the_premultiply_round_trip_is_close() {
        let mut b = buf(1, 1);
        b.px[0] = Px {
            a: 128.0,
            r: 200.0,
            g: 200.0,
            b: 200.0,
        };
        mult(&mut b, false);
        assert!(b.px[0].r < 200.0);
        mult(&mut b, true);
        assert!(b.px[0].r <= 200.0 && b.px[0].r >= 196.0, "{:?}", b.px[0]);
    }

    /// 膨張は明るいところを広げる。
    #[test]
    fn the_max_spreads_the_light() {
        let mut b = buf(9, 1);
        b.px[4] = white();
        minmax(&mut b, 2, true, Quirk::None);
        for x in 2..=6 {
            assert_eq!(b.px[x].r, 255.0, "x = {x}");
        }
        assert_eq!(b.px[1], Px::default());
    }

    /// 収縮は暗いところを広げる。
    #[test]
    fn the_min_spreads_the_dark() {
        let mut b = buf(9, 1);
        for p in b.px.iter_mut() {
            *p = white();
        }
        b.px[4] = Px::default();
        minmax(&mut b, -2, false, Quirk::None);
        for x in 2..=6 {
            assert_eq!(b.px[x], Px::default(), "x = {x}");
        }
    }

    /// 箱ぼかしは縁で端の画素を伸ばすので、一様な絵は変わらない。
    #[test]
    fn the_box_blur_keeps_a_flat_image() {
        let mut b = buf(16, 8);
        for p in b.px.iter_mut() {
            *p = white();
        }
        box_blur(&mut b, 3);
        assert!(b.px.iter().all(|p| *p == white()), "{:?}", b.px[0]);
    }

    /// 箱ぼかしは点を広げる。3回掛けるので台形ではなく丸みのある形。
    #[test]
    fn the_box_blur_spreads_a_dot() {
        let mut b = buf(64, 1);
        b.px[32] = white();
        box_blur(&mut b, 4);
        assert!(b.px[32].r < 255.0);
        assert!(b.px[36].r > 0.0);
        // 3回ぶんの広がりは半径の3倍ほど。
        assert_eq!(b.px[20].r, 0.0);
    }

    /// 元の *10 落ちを真似ると、縦の全列の枝で列の先頭が勝ってしまう。
    #[test]
    fn the_whole_column_quirk_takes_the_top_pixel() {
        let mut b = Buf {
            w: 1,
            h: 4,
            px: vec![
                // 先頭が 10 倍のまま比べられるので、2番目が倍の明るさでも
                // 先頭が勝つ (100*10 > 200)。
                Px { a: 255.0, r: 100.0, g: 100.0, b: 100.0 },
                Px { a: 255.0, r: 200.0, g: 200.0, b: 200.0 },
                Px { a: 255.0, r: 100.0, g: 100.0, b: 100.0 },
                Px { a: 255.0, r: 100.0, g: 100.0, b: 100.0 },
            ],
            max: 255.0,
            float: false,
        };
        let mut fixed = Buf { w: 1, h: 4, px: b.px.clone(), max: 255.0, float: false };
        minmax(&mut fixed, 9, true, Quirk::None);
        assert_eq!(fixed.px[0].r, 200.0, "直した版は明るい画素が勝つ");
        minmax(&mut b, 9, true, Quirk::WholeNo10);
        assert_eq!(b.px[0].r, 100.0, "元は先頭が勝つ");
    }

    /// アルファのカーブ: 正の k は半端なアルファを薄くする
    /// (カーブは薄いほうを持ち上げる形だが、正では反転して掛けるため)。
    #[test]
    fn a_positive_k_thins_the_alpha() {
        let mut b = buf(1, 1);
        b.px[0] = Px {
            a: 128.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        hyperbolic(&mut b, 4.0);
        assert!(b.px[0].a < 128.0, "{:?}", b.px[0]);
    }

    /// 負の k は濃くする。
    #[test]
    fn a_negative_k_thickens_the_alpha() {
        let mut b = buf(1, 1);
        b.px[0] = Px {
            a: 128.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        hyperbolic(&mut b, -4.0);
        assert!(b.px[0].a > 128.0, "{:?}", b.px[0]);
    }

    /// 0 と最大のアルファは触らない。
    #[test]
    fn the_curve_leaves_the_ends_alone() {
        let mut b = buf(2, 1);
        b.px[1] = white();
        hyperbolic(&mut b, 9.0);
        assert_eq!(b.px[0].a, 0.0);
        assert_eq!(b.px[1].a, 255.0);
    }
}
