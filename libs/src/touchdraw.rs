//! F's TouchDraw と F's TouchDrawStraght が分け合っているところ。
//!
//! 元は両方の `TouchDraw/` と `TouchDrawStraght/` に同じ `FsTD*.cpp` が
//! 置いてあって、違うのは**タッチ1本の引き方**だけ (中心へ向かうか、
//! 決まった角度へ伸ばすか)。ここには共通のぶんだけ入れてある:
//!
//! - `find_target_color` / `find_edge`: 対象の色を探して縁に旗を立てる
//! - `find_target_delta`: 隣との差が大きいところに旗を立てる
//! - `block_draw`: 旗を四角くランダムに消す (タッチを間引く)
//! - `pset_alpha`: 小数の位置へタッチ1点を書く (緑に貯める)
//! - `green_blur`: 緑を 3×3 でぼかしてアルファに入れる
//! - `copy_alpha`: アルファを指定色に付けて、元の絵に重ねる
//!
//! 旗の持ちかたは元のまま、出力の画素の**チャンネルを間借り**している
//! (赤: 対象色と同じか、青: 縁かどうか、緑: タッチの濃さ)。
//!
//! 直したバグは `green_blur` と `copy_alpha` の注を参照。`faithful` を
//! 立てると元に戻る。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/TouchDraw

use crate::fs_graphics::XorShift;

/// 深さに依らない画素。値はその深さの生の目盛り。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深さごとの丸めと上限。
#[derive(Clone, Copy, Debug)]
pub struct Ops {
    pub max: f64,
    pub float: bool,
}

impl Ops {
    /// 元の RoundByteLong / RoundShort / RoundFpShortDouble。
    pub fn clip(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 32.0)
        } else {
            v.clamp(0.0, self.max).trunc()
        }
    }

    /// 元の mixCh8 / mixCh16 / mixCh32。スクリーン合成だが、整数の深さは
    /// 割る数が目盛りでなく 2 の冪 (8bit は 256、16bit は 65536) なので、
    /// 16bit では効きが半分になる。元のまま。
    pub fn mix(&self, c0: f64, c1: f64) -> f64 {
        if self.float {
            (c0 + c1 - c0 * c1).max(0.0)
        } else {
            let shift = if self.max > 255.0 { 16 } else { 8 };
            (c0 + c1 - ((c0 as i64 * c1 as i64) >> shift) as f64).clamp(0.0, self.max)
        }
    }

    /// 画素を 8bit の目盛りに直す (元の CONV16TO8 / CONV32TO8)。
    pub fn to8(&self, p: &Px) -> (i64, i64, i64) {
        if self.float {
            let q = crate::utils::conv_32_to_8(&after_effects::sys::PF_PixelFloat {
                alpha: p.a as f32,
                red: p.r as f32,
                green: p.g as f32,
                blue: p.b as f32,
            });
            (q.red as i64, q.green as i64, q.blue as i64)
        } else if self.max > 255.0 {
            let q = crate::utils::conv_16_to_8(&after_effects::sys::PF_Pixel16 {
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

/// 画素の並び。元の GInfo / CFsGraph。
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
}

/// 元の FindTargetColor。対象の色と同じところの赤に旗を立てて、数を返す。
pub fn find_target_color(inp: &Buf, out: &mut Buf, colour: (i64, i64, i64), range: i64, ops: &Ops) -> i64 {
    // 元: RoundByteLong(PF_MAX_CHAN8 * color_range >> 16)。
    let lv = ((255 * range) >> 16).clamp(0, 255);
    let mut count = 0;
    for i in 0..out.px.len() {
        let (r, g, b) = ops.to8(&inp.px[i]);
        let hit = (b - colour.2).abs() <= lv && (g - colour.1).abs() <= lv && (r - colour.0).abs() <= lv;
        out.px[i].r = if hit {
            count += 1;
            ops.max
        } else {
            0.0
        };
    }
    count
}

/// 元の FindEdge。赤の旗が立っているところのうち、まわりに旗の無い画素が
/// あるものを「縁」として青に旗を立てる。旗の無いところはアルファを 0 に。
///
/// 縁の確かめは元のまま、右と下は `幅 - 2` までしか見ない (いちばん右の
/// 列といちばん下の行は、そちら側の隣を見ないまま「内側」とみなす)。
pub fn find_edge(out: &mut Buf, ops: &Ops) -> i64 {
    let mx = ops.max;
    let red: Vec<bool> = out.px.iter().map(|p| p.r >= mx).collect();
    let (w, h) = (out.w, out.h);
    let on = |x: i64, y: i64| red[(x + y * w) as usize];
    let mut count = 0;
    for y in 0..h {
        for x in 0..w {
            let i = (x + y * w) as usize;
            if !red[i] {
                out.px[i].a = 0.0;
                continue;
            }
            let mut r = true;
            if x >= 1 && y >= 1 && !on(x - 1, y - 1) {
                r = false;
            }
            if r && y >= 1 && !on(x, y - 1) {
                r = false;
            }
            if r && x <= w - 2 && y >= 1 && !on(x + 1, y - 1) {
                r = false;
            }
            if r && x >= 1 && !on(x - 1, y) {
                r = false;
            }
            if r && x <= w - 2 && !on(x + 1, y) {
                r = false;
            }
            if r && x >= 1 && y <= h - 2 && !on(x - 1, y + 1) {
                r = false;
            }
            if r && y <= h - 2 && !on(x, y + 1) {
                r = false;
            }
            if r && x <= w - 2 && y <= h - 2 && !on(x + 1, y + 1) {
                r = false;
            }
            if r {
                out.px[i].b = 0.0;
            } else {
                out.px[i].b = mx;
                count += 1;
            }
        }
    }
    count
}

/// 元の FindTarget。対象色と同じところが1つも無ければ出力を消す。
pub fn find_target(inp: &Buf, out: &mut Buf, colour: (i64, i64, i64), range: i64, ops: &Ops) {
    if find_target_color(inp, out, colour, range, ops) > 0 {
        find_edge(out, ops);
    } else {
        out.px.iter_mut().for_each(|p| *p = Px::default());
    }
}

/// 元の SubtrucPixel / SubtrucPixelAlpha。隣との差を 8bit の目盛りで返す。
fn delta(s: &Px, d: &Px, is_alpha: bool, ops: &Ops) -> i64 {
    if is_alpha {
        if ops.float {
            let v = (s.a - d.a).abs();
            crate::utils::round_byte_fp_long((255.0 * v) as f32 + 0.5) as i64
        } else if ops.max > 255.0 {
            // 元: FS_CONVERT16TO8(|差|)。
            let v = (s.a - d.a).abs();
            (((v as i64 * 255) + 16384) / 32768).clamp(0, 255)
        } else {
            (s.a - d.a).abs() as i64
        }
    } else if ops.float {
        let mut v = 0.0f64;
        for (a, b) in [(s.r, d.r), (s.g, d.g), (s.b, d.b)] {
            let mut t = (a * s.a - b * d.a).abs();
            if t > 1.0 {
                t = 1.0;
            }
            v += t;
        }
        crate::utils::round_byte_fp_long((255.0 * (v / 3.0)) as f32 + 0.5) as i64
    } else if ops.max > 255.0 {
        let m = 32768i64;
        let mut v = 0i64;
        for (a, b) in [(s.r, d.r), (s.g, d.g), (s.b, d.b)] {
            v += ((a as i64 * s.a as i64) / m - (b as i64 * d.a as i64) / m).abs();
        }
        ((((v / 3) * 255) + 16384) / 32768).min(255)
    } else {
        let mut v = 0i64;
        for (a, b) in [(s.r, d.r), (s.g, d.g), (s.b, d.b)] {
            v += (a as i64 * s.a as i64 - b as i64 * d.a as i64).abs() / 255;
        }
        (v / 3).min(255)
    }
}

/// 元の FindTargetDeltaBright / FindTargetDeltaAlpha。右と下の隣との差が
/// `range` を超えたところの青に旗を立てる。
pub fn find_target_delta(inp: &Buf, out: &mut Buf, range: i64, is_alpha: bool, ops: &Ops) {
    let lv = (255 * range) >> 16;
    for y in 0..out.h {
        for x in 0..out.w {
            let s = inp.at(x, y);
            let mut mv = 0;
            if x <= out.w - 2 {
                mv = mv.max(delta(&s, &inp.at(x + 1, y), is_alpha, ops));
            }
            if mv < 255 && y <= out.h - 2 {
                mv = mv.max(delta(&s, &inp.at(x, y + 1), is_alpha, ops));
            }
            if mv > lv {
                let mut p = out.at(x, y);
                p.b = ops.max;
                out.set(x, y, p);
            }
        }
    }
}

/// 元の blockDraw。四角をランダムに置いて、そこの青の旗を消す。
pub fn block_draw(out: &mut Buf, b_size: i64, b_count: i64, rng: &mut XorShift) {
    let bs0 = (b_size / 2).max(2);
    for _ in 0..b_count {
        let w = rng.rand2(bs0, b_size);
        let h = rng.rand2(bs0, b_size);
        let x = rng.fm_rand(out.w - w);
        let y = rng.fm_rand(out.h - h);
        for j in y..(y + h) {
            for i in x..(x + w) {
                if i >= 0 && i < out.w && j >= 0 && j < out.h {
                    let mut p = out.at(i, j);
                    p.b = 0.0;
                    out.set(i, j, p);
                }
            }
        }
    }
}

/// 元の PsetAlpha。固定小数の位置 (16.16) へ、濃さ `opacity` (16.16 で
/// 1.0 = 65536) のタッチ1点を4画素に分けて緑へ足す。書けたら `true`。
pub fn pset_alpha(buf: &mut Buf, pos_x: i64, pos_y: i64, opacity: i64, ops: &Ops) -> bool {
    let v = if ops.float {
        let v = opacity as f32 / 65536.0;
        if v <= 0.0 {
            return false;
        }
        v as f64
    } else {
        let o = opacity.clamp(0, 65536);
        let v = (ops.max as i64 * o) >> 16;
        if v <= 0 {
            return false;
        }
        v as f64
    };

    let (w, h) = (buf.w, buf.h);
    let x1 = pos_x >> 16;
    let y1 = pos_y >> 16;
    let x2 = x1 + 1;
    let y2 = y1 + 1;
    if x1 >= w || y1 >= h || x2 < 0 || y2 < 0 {
        return false;
    }
    let xx2 = (pos_x & 0xFFFF) as f32 / 65536.0;
    let yy2 = (pos_y & 0xFFFF) as f32 / 65536.0;
    let xx1 = 1.0f32 - xx2;
    let yy1 = 1.0f32 - yy2;

    let mut ret = false;
    let put = |buf: &mut Buf, x: i64, y: i64, k: f32| {
        let vv = if ops.float {
            (v as f32 * k) as f64
        } else {
            (v as f32 * k) as i64 as f64
        };
        let mut p = buf.at(x, y);
        p.g = ops.mix(p.g, vv);
        buf.set(x, y, p);
    };
    if xx2 == 0.0 && yy2 == 0.0 {
        if x1 >= 0 && y1 >= 0 {
            put(buf, x1, y1, 1.0);
            ret = true;
        }
    } else {
        if x1 >= 0 && y1 >= 0 {
            put(buf, x1, y1, xx1 * yy1);
            ret = true;
        }
        if x2 < w && y1 >= 0 {
            put(buf, x2, y1, xx2 * yy1);
            ret = true;
        }
        if x2 < w && y2 < h {
            put(buf, x2, y2, xx2 * yy2);
            ret = true;
        }
        if x1 >= 0 && y2 < h {
            put(buf, x1, y2, xx1 * yy2);
            ret = true;
        }
    }
    ret
}

/// 元の greenBlur。緑を 3×3 (重み 2,4,2 / 4,8,4 / 2,4,2) でぼかして
/// アルファに入れる。内側は 32 で割り、辺は 24、角は 18。
///
/// 元と違うところ (直したバグ):
///
/// 1. **四隅のうち3つが読み書きの位置を1行 (1列) ずれている**。右上は
///    `data[w-2]` へ書いて `w-2, w-1` 列を読んでいた (本当は `w-1` へ
///    書いて `w-2, w-1`... ではなく、いちばん右の列 `w-1` を含む 2×2)。
///    左下・右下も同じ。画面の角だけぼかしが掛からず、代わりに内側の
///    画素が二度書きされていた
/// 2. **右の辺で、下の行の2つだけ1列ぶん内側を読んでいる**
///    (`adr2 += (wt - 2)` が `wt - 1` のつもり)
///
/// どちらも TouchDrawCenter の `AlphaBlur` と同じ間違い。
pub fn green_blur(buf: &mut Buf, ops: &Ops, faithful: bool) {
    blur_to_alpha(buf, false, ops, faithful)
}

/// `green_blur` と同じもので、読むチャンネルを選べるようにしたもの。
/// NamiGarasu の `AlphaBlur` (赤を読む) が元のソースでも同じ中身なので、
/// `from_red` で切り替えて分け合っている。
pub fn blur_to_alpha(buf: &mut Buf, from_red: bool, ops: &Ops, faithful: bool) {
    let (w, h) = (buf.w, buf.h);
    if w < 2 || h < 2 {
        return;
    }
    let g = |x: i64, y: i64| {
        let p = buf.at(x, y);
        if from_red {
            p.r
        } else {
            p.g
        }
    };
    // 元はその場でアルファに書き込むので、書かないところ (元の間違いで
    // 残る四隅) は前の値のままになる。
    let mut alpha: Vec<f64> = buf.px.iter().map(|p| p.a).collect();
    // 内側。
    for y in 1..(h - 1) {
        for x in 1..(w - 1) {
            let v = g(x - 1, y - 1) * 2.0
                + g(x, y - 1) * 4.0
                + g(x + 1, y - 1) * 2.0
                + g(x - 1, y) * 4.0
                + g(x, y) * 8.0
                + g(x + 1, y) * 4.0
                + g(x - 1, y + 1) * 2.0
                + g(x, y + 1) * 4.0
                + g(x + 1, y + 1) * 2.0;
            alpha[(x + y * w) as usize] = if ops.float {
                v / 32.0
            } else {
                ((v as i64) >> 5) as f64
            };
        }
    }
    let div = |v: f64, d: f64| -> f64 {
        if ops.float {
            v / d
        } else {
            ((v as i64) / d as i64) as f64
        }
    };
    // 上と下の辺。
    for x in 1..(w - 1) {
        let v = g(x - 1, 0) * 4.0
            + g(x, 0) * 8.0
            + g(x + 1, 0) * 4.0
            + g(x - 1, 1) * 2.0
            + g(x, 1) * 4.0
            + g(x + 1, 1) * 2.0;
        alpha[x as usize] = div(v, 24.0);
        let v = g(x - 1, h - 1) * 4.0
            + g(x, h - 1) * 8.0
            + g(x + 1, h - 1) * 4.0
            + g(x - 1, h - 2) * 2.0
            + g(x, h - 2) * 4.0
            + g(x + 1, h - 2) * 2.0;
        alpha[(x + (h - 1) * w) as usize] = div(v, 24.0);
    }
    // 左と右の辺。
    for y in 1..(h - 1) {
        let v = g(0, y - 1) * 4.0
            + g(1, y - 1) * 2.0
            + g(0, y) * 8.0
            + g(1, y) * 4.0
            + g(0, y + 1) * 4.0
            + g(1, y + 1) * 2.0;
        alpha[(y * w) as usize] = div(v, 24.0);
        // 元は下の行だけ1列内側を読む (直したバグ 2)。
        let lo = if faithful { w - 3 } else { w - 2 };
        let v = g(w - 2, y - 1) * 2.0
            + g(w - 1, y - 1) * 4.0
            + g(w - 2, y) * 4.0
            + g(w - 1, y) * 8.0
            + g(lo.max(0), y + 1) * 2.0
            + g((lo + 1).min(w - 1), y + 1) * 4.0;
        alpha[(w - 1 + y * w) as usize] = div(v, 24.0);
    }
    // 四隅。元はここが1行 (1列) ずれている (直したバグ 1)。
    let corner = |ax: i64, ay: i64, bx: i64, by: i64| -> f64 {
        div(
            g(ax, ay) * 8.0 + g(bx, ay) * 4.0 + g(ax, by) * 4.0 + g(bx, by) * 2.0,
            18.0,
        )
    };
    alpha[0] = corner(0, 0, 1, 1);
    if faithful {
        // 元のずれは1行 (1列) 内側を読み書きするので、幅か高さが 3 に
        // 足りないと絵の外へ出てしまう (元はワールドの外を触っている)。
        // そこは再現できないので何も書かない。
        if w >= 3 && h >= 3 {
            // 右上: 元の `w` は幅-1 なので、`data[w-2]` は右から3つめの列。
            let v = g(w - 3, 0) * 4.0 + g(w - 2, 0) * 8.0 + g(w - 3, 1) * 2.0 + g(w - 2, 1) * 4.0;
            alpha[(w - 3) as usize] = div(v, 18.0);
            // 左下。
            let v = g(0, h - 3) * 4.0 + g(1, h - 3) * 2.0 + g(0, h - 2) * 8.0 + g(1, h - 2) * 4.0;
            alpha[((h - 2) * w) as usize] = div(v, 18.0);
            // 右下。
            let v = g(w - 2, h - 3) * 2.0 + g(w - 1, h - 3) * 4.0 + g(w - 2, h - 2) * 4.0
                + g(w - 1, h - 2) * 8.0;
            alpha[(w - 2 + (h - 2) * w) as usize] = div(v, 18.0);
        }
    } else {
        alpha[(w - 1) as usize] = corner(w - 1, 0, w - 2, 1);
        alpha[((h - 1) * w) as usize] = corner(0, h - 1, 1, h - 2);
        alpha[(w - 1 + (h - 1) * w) as usize] = corner(w - 1, h - 1, w - 2, h - 2);
    }

    for i in 0..buf.px.len() {
        buf.px[i].a = ops.clip(alpha[i]);
    }
}

/// 元の PixelBlend。`dst` の下に `src` を敷く。
pub fn pixel_blend(src: &Px, dst: &Px, ops: &Ops) -> Px {
    if src.a == 0.0 || dst.a >= ops.max {
        return *dst;
    }
    if dst.a == 0.0 {
        return *src;
    }
    if ops.float {
        let as2 = (1.0 - dst.a) * src.a;
        let ad2 = (dst.a + as2).min(1.0);
        if ad2 <= 0.0 {
            return Px::default();
        }
        let mix = |d: f64, s: f64| ((d * dst.a + s * as2) / ad2).min(1.0);
        Px {
            a: ad2,
            r: mix(dst.r, src.r),
            g: mix(dst.g, src.g),
            b: mix(dst.b, src.b),
        }
    } else {
        let m = ops.max as i64;
        let shift = if m > 255 { 15 } else { 8 };
        let top = if m > 255 { m } else { m + 1 };
        let as2 = ((top - dst.a as i64) * src.a as i64) >> shift;
        let ad2 = (dst.a as i64 + as2).min(m);
        if ad2 <= 0 {
            return Px::default();
        }
        let mix = |d: f64, s: f64| {
            (((d as i64 * dst.a as i64 + s as i64 * as2) / ad2).min(m)) as f64
        };
        Px {
            a: ad2 as f64,
            r: mix(dst.r, src.r),
            g: mix(dst.g, src.g),
            b: mix(dst.b, src.b),
        }
    }
}

/// 元の copyAlpha。ぼかしたアルファを指定色に付ける。`org` が立っていれば
/// 入力の上に重ねる。
///
/// 元と違うところ (直したバグ): **元は緑をアルファとして使っていた**
/// (`#define TARGET_CHANNEL green`)。直前の `greenBlur` が緑をぼかして
/// アルファに入れているので、元はぼかす前の緑をそのまま使っていて、
/// ぼかしが一度も絵に出てこない。TouchDrawCenter の同じ並び
/// (`AlphaBlur` → `CopyImage`) はアルファを使うので、ここも
/// アルファにした。`faithful` で元に戻る。
pub fn copy_alpha(inp: &Buf, out: &mut Buf, colour: &Px, org: bool, ops: &Ops, faithful: bool) {
    for i in 0..out.px.len() {
        let mut p = *colour;
        p.a = if faithful { out.px[i].g } else { out.px[i].a };
        out.px[i] = if org {
            pixel_blend(&inp.px[i], &p, ops)
        } else {
            p
        };
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

    fn px(a: f64, r: f64, g: f64, b: f64) -> Px {
        Px { a, r, g, b }
    }

    /// 対象色と同じところだけ赤に旗が立つ。
    #[test]
    fn the_target_colour_raises_the_flag() {
        let o = ops8();
        let mut inp = Buf::new(3, 1);
        inp.px[0] = px(255.0, 0.0, 0.0, 0.0);
        inp.px[1] = px(255.0, 255.0, 255.0, 255.0);
        inp.px[2] = px(255.0, 4.0, 0.0, 0.0);
        let mut out = Buf::new(3, 1);
        // 範囲 0 ならぴったり同じものだけ。
        let n = find_target_color(&inp, &mut out, (0, 0, 0), 0, &o);
        assert_eq!(n, 1);
        assert_eq!(out.px[0].r, 255.0);
        assert_eq!(out.px[2].r, 0.0);
        // 範囲を広げると近い色も入る (5/255 ぶん)。
        let n = find_target_color(&inp, &mut out, (0, 0, 0), 65536 * 5 / 255, &o);
        assert_eq!(n, 2);
    }

    /// まわりが全部旗なら内側、1つでも欠けていれば縁。
    #[test]
    fn the_edge_is_where_the_flag_runs_out() {
        let o = ops8();
        let mut out = Buf::new(5, 5);
        for y in 1..4 {
            for x in 1..4 {
                out.px[(x + y * 5) as usize].r = 255.0;
            }
        }
        find_edge(&mut out, &o);
        assert_eq!(out.at(2, 2).b, 0.0, "まわりが全部旗なら内側");
        assert_eq!(out.at(1, 1).b, 255.0, "隣が欠けるところは縁");
        assert_eq!(out.at(0, 0).b, 0.0, "旗の無いところは縁にならない");
        assert_eq!(out.at(0, 0).a, 0.0, "旗の無いところはアルファが 0");
    }

    /// 隣と差があるところに旗が立つ。
    #[test]
    fn the_delta_raises_the_flag() {
        let o = ops8();
        let mut inp = Buf::new(4, 1);
        inp.px[0] = px(255.0, 0.0, 0.0, 0.0);
        inp.px[1] = px(255.0, 255.0, 255.0, 255.0);
        inp.px[2] = px(255.0, 255.0, 255.0, 255.0);
        inp.px[3] = px(255.0, 255.0, 255.0, 255.0);
        let mut out = Buf::new(4, 1);
        find_target_delta(&inp, &mut out, 32768, false, &o);
        assert_eq!(out.px[0].b, 255.0, "段差の手前に立つ");
        assert_eq!(out.px[1].b, 0.0, "平らなところには立たない");
    }

    /// 四角を置くと旗が消える。
    #[test]
    fn the_blocks_wipe_the_flag() {
        let mut out = Buf::new(32, 32);
        out.px.iter_mut().for_each(|p| p.b = 255.0);
        let mut rng = XorShift::new(0);
        block_draw(&mut out, 8, 20, &mut rng);
        let left = out.px.iter().filter(|p| p.b > 0.0).count();
        assert!(left > 0 && left < out.px.len(), "一部だけ消える: {left}");
    }

    /// ぴったり格子の上なら1画素、ずれていれば4画素に分かれる。
    #[test]
    fn one_touch_lands_on_four_pixels() {
        let o = ops8();
        let mut b = Buf::new(4, 4);
        pset_alpha(&mut b, 1 << 16, 1 << 16, 65536, &o);
        assert_eq!(b.at(1, 1).g, 255.0);
        assert_eq!(b.at(2, 1).g, 0.0);
        let mut b = Buf::new(4, 4);
        pset_alpha(&mut b, (1 << 16) + 32768, 1 << 16, 65536, &o);
        assert!(b.at(1, 1).g > 0.0 && b.at(2, 1).g > 0.0, "横に分かれる");
        assert_eq!(b.at(1, 2).g, 0.0, "縦はずれていないので下の行には乗らない");
    }

    /// 画面の外へは書かない。
    #[test]
    fn a_touch_outside_writes_nothing() {
        let o = ops8();
        let mut b = Buf::new(4, 4);
        assert!(!pset_alpha(&mut b, -4 << 16, 0, 65536, &o));
        assert!(b.px.iter().all(|p| p.g == 0.0));
    }

    /// ぼかすと緑がアルファに移って、まわりへ少し広がる。
    #[test]
    fn the_blur_moves_green_into_alpha() {
        let o = ops8();
        let mut b = Buf::new(5, 5);
        b.px[(2 + 2 * 5) as usize].g = 255.0;
        green_blur(&mut b, &o, false);
        assert_eq!(b.at(2, 2).a, (255.0 * 8.0 / 32.0f64).trunc());
        assert!(b.at(1, 2).a > 0.0, "隣へ広がる");
        assert_eq!(b.at(0, 0).a, 0.0);
    }

    /// 直した四隅は、角の画素にもぼかしが掛かる。
    #[test]
    fn the_fixed_corners_cover_the_corner_pixel() {
        let o = ops8();
        let mut b = Buf::new(5, 5);
        b.px.iter_mut().for_each(|p| p.g = 255.0);
        let mut fixed = Buf::new(5, 5);
        fixed.px.copy_from_slice(&b.px);
        green_blur(&mut fixed, &o, false);
        let mut faith = Buf::new(5, 5);
        faith.px.copy_from_slice(&b.px);
        green_blur(&mut faith, &o, true);
        assert!(fixed.at(4, 0).a > 0.0, "直した版は右上の角も塗る");
        assert_eq!(faith.at(4, 0).a, 0.0, "元は右上の角に何も書かない");
        assert!(fixed.at(4, 4).a > 0.0);
        assert!(fixed.at(0, 4).a > 0.0);
    }

    /// `org` が立っていれば入力の上に重なる。
    #[test]
    fn the_original_shows_through() {
        let o = ops8();
        let mut inp = Buf::new(1, 1);
        inp.px[0] = px(255.0, 255.0, 0.0, 0.0);
        let mut out = Buf::new(1, 1);
        out.px[0].a = 128.0;
        let col = px(255.0, 0.0, 0.0, 255.0);
        copy_alpha(&inp, &mut out, &col, true, &o, false);
        assert_eq!(out.px[0].a, 255.0, "下が不透明なので不透明になる");
        assert!(out.px[0].r > 0.0 && out.px[0].b > 0.0, "赤と青が混ざる");
        // org なしなら指定色そのまま。
        let mut out = Buf::new(1, 1);
        out.px[0].a = 128.0;
        copy_alpha(&inp, &mut out, &col, false, &o, false);
        assert_eq!(out.px[0], px(128.0, 0.0, 0.0, 255.0));
    }
}
