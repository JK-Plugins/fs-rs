// F's Plugin の CFsGraph (FsGraphics.h / FsMiniMax8.cpp / FsBlur.cpp / FsMat.cpp) の移植。
// ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/FsLibrary
//
// AE の型に依存しないよう、ビット深度に関係なく 0.0..=1.0 の f32 (ストレートアルファ) で処理する。
// 8bit/16bit/32bit の変換はプラグイン側で行う。

/// ストレートアルファのピクセル。各チャンネルは 0.0..=1.0（32bit では 1.0 を超えることもある）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PixelF {
    pub alpha: f32,
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl PixelF {
    pub const fn new(alpha: f32, red: f32, green: f32, blue: f32) -> Self {
        Self {
            alpha,
            red,
            green,
            blue,
        }
    }

    /// 元プラグインと同じ係数の輝度
    pub fn luma(&self) -> f32 {
        0.29891 * self.red + 0.58661 * self.green + 0.11448 * self.blue
    }

    fn map_rgb(self, f: impl Fn(f32) -> f32) -> Self {
        Self {
            alpha: self.alpha,
            red: f(self.red),
            green: f(self.green),
            blue: f(self.blue),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanLine {
    HorAndVer,
    Hor,
    Ver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetChannel {
    RgbAndAlpha,
    Rgb,
    Alpha,
}

/// 作業用の画像バッファ
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub data: Vec<PixelF>,
}

impl Image {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![PixelF::default(); width * height],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> PixelF {
        self.data[y * self.width + x]
    }

    #[inline]
    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut PixelF {
        &mut self.data[y * self.width + x]
    }

    // ---------------------------------------------------------------- mat

    /// 黒マット（乗算済み）にする
    pub fn to_black_mat(&mut self) {
        for p in &mut self.data {
            if p.alpha <= 0.0 {
                *p = PixelF::new(p.alpha, 0.0, 0.0, 0.0);
            } else if p.alpha < 1.0 {
                let a = p.alpha;
                *p = p.map_rgb(|c| c * a);
            }
        }
    }

    pub fn from_black_mat(&mut self) {
        for p in &mut self.data {
            if p.alpha <= 0.0 {
                *p = PixelF::new(p.alpha, 0.0, 0.0, 0.0);
            } else if p.alpha < 1.0 {
                let a = p.alpha;
                *p = p.map_rgb(|c| (c / a).clamp(0.0, 1.0));
            }
        }
    }

    /// 白マットにする
    ///
    /// 元実装の `toWhiteMat32` は blue の行で `p.blue` ではなく（代入済みの）`p.green`
    /// を読んでいて、青に変換が二重にかかる。ここでは正しく blue を使う。
    /// 8bit 側 ([`super::fs_graphics8::Image8::to_white_mat8`]) も同じ判断で揃えている。
    pub fn to_white_mat(&mut self) {
        for p in &mut self.data {
            if p.alpha <= 0.0 {
                *p = PixelF::new(p.alpha, 1.0, 1.0, 1.0);
            } else if p.alpha < 1.0 {
                let a = p.alpha;
                *p = p.map_rgb(|c| c * a + 1.0 - a);
            }
        }
    }

    pub fn from_white_mat(&mut self) {
        for p in &mut self.data {
            if p.alpha <= 0.0 {
                *p = PixelF::new(p.alpha, 0.0, 0.0, 0.0);
            } else if p.alpha < 1.0 {
                let a = p.alpha;
                *p = p.map_rgb(|c| ((c - 1.0 + a) / a).clamp(0.0, 1.0));
            }
        }
    }

    // ---------------------------------------------------------------- minimax

    /// 最大・最小フィルタ。value > 0 で膨張（明るい方を採用）、value < 0 で収縮（暗い方を採用）。
    /// RGB は輝度で、アルファはアルファ値で比較する。
    pub fn minimax(&mut self, value: i32, mode: ScanLine, target: TargetChannel) {
        if value == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let radius = value.unsigned_abs() as usize;
        let minus = value < 0;

        if minus {
            self.to_white_mat();
        } else {
            self.to_black_mat();
        }

        if matches!(target, TargetChannel::RgbAndAlpha | TargetChannel::Rgb) {
            self.minimax_channel(
                radius,
                minus,
                mode,
                |p| p.luma(),
                |dst, src| {
                    dst.red = src.red;
                    dst.green = src.green;
                    dst.blue = src.blue;
                },
            );
        }
        if matches!(target, TargetChannel::RgbAndAlpha | TargetChannel::Alpha) {
            self.minimax_channel(
                radius,
                minus,
                mode,
                |p| p.alpha,
                |dst, src| {
                    dst.alpha = src.alpha;
                },
            );
        }

        if minus {
            self.from_white_mat();
        } else {
            self.from_black_mat();
        }
    }

    /// 元の `MinimaxA16` / `MinimaxA32`。アルファだけを対象にした最大・最小
    /// フィルタで、[`Image::minimax`] と違って**マット処理をしない**
    /// (元の `MinimaxA*` は `Minimax_alpha*` を直接呼ぶだけ)。RGB には触らない。
    pub fn minimax_alpha(&mut self, value: i32, mode: ScanLine) {
        if value == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        self.minimax_channel(
            value.unsigned_abs() as usize,
            value < 0,
            mode,
            |p| p.alpha,
            |dst, src| dst.alpha = src.alpha,
        );
    }

    fn minimax_channel(
        &mut self,
        radius: usize,
        minus: bool,
        mode: ScanLine,
        level: impl Fn(&PixelF) -> f32,
        apply: impl Fn(&mut PixelF, &PixelF),
    ) {
        let (w, h) = (self.width, self.height);
        let mut scanline = vec![PixelF::default(); w.max(h)];
        let mut lv = vec![0.0f32; w.max(h)];

        if matches!(mode, ScanLine::HorAndVer | ScanLine::Hor) {
            for y in 0..h {
                for x in 0..w {
                    scanline[x] = self.get(x, y);
                    lv[x] = level(&scanline[x]);
                }
                let picks = minimax_line(&lv[..w], radius, minus);
                for x in 0..w {
                    apply(self.get_mut(x, y), &scanline[picks[x]]);
                }
            }
        }
        if matches!(mode, ScanLine::HorAndVer | ScanLine::Ver) {
            for x in 0..w {
                for y in 0..h {
                    scanline[y] = self.get(x, y);
                    lv[y] = level(&scanline[y]);
                }
                let picks = minimax_line(&lv[..h], radius, minus);
                for y in 0..h {
                    apply(self.get_mut(x, y), &scanline[picks[y]]);
                }
            }
        }
    }

    // ---------------------------------------------------------------- blur

    /// ガウスぼかし（元の gblur8 と同じく、範囲 = ceil(value)、σ = value / 3）
    pub fn gaussian_blur(&mut self, value: f64, mode: ScanLine) {
        if value <= 0.0 || self.width == 0 || self.height == 0 {
            return;
        }
        let range = (value.ceil() as usize).max(1);
        let zone = value / 3.0;
        let table: Vec<f64> = (0..=range)
            .map(|i| (-((i * i) as f64) / (2.0 * zone * zone)).exp())
            .collect();

        let (w, h) = (self.width, self.height);
        let mut scanline = vec![PixelF::default(); w.max(h)];

        if matches!(mode, ScanLine::HorAndVer | ScanLine::Hor) {
            for y in 0..h {
                for x in 0..w {
                    scanline[x] = premultiply(self.get(x, y));
                }
                for x in 0..w {
                    *self.get_mut(x, y) = blur_at(&scanline[..w], x, &table);
                }
            }
        }
        if matches!(mode, ScanLine::HorAndVer | ScanLine::Ver) {
            for x in 0..w {
                for y in 0..h {
                    scanline[y] = premultiply(self.get(x, y));
                }
                for y in 0..h {
                    *self.get_mut(x, y) = blur_at(&scanline[..h], y, &table);
                }
            }
        }
    }
}

/// 各位置について、窓 [i - r, i + r - 1] の中で最大（minus なら最小）の要素の添字を返す。
/// 同値の場合は後ろの要素を採用する（元実装と同じ）。
fn minimax_line(lv: &[f32], radius: usize, minus: bool) -> Vec<usize> {
    let n = lv.len();
    // better(cur, cand): cand に置き換えるべきか
    let better = |cur: f32, cand: f32| if minus { cur >= cand } else { cur <= cand };
    let mut picks = vec![0; n];
    let mut best: Option<usize> = None;
    for i in 0..n {
        let k0 = i.saturating_sub(radius);
        let k1 = (i + radius).saturating_sub(1).min(n - 1).max(k0);
        match best {
            Some(b) if b >= k0 => {
                if better(lv[b], lv[k1]) {
                    best = Some(k1);
                }
            }
            _ => {
                let mut b = k0;
                for k in k0..=k1 {
                    if better(lv[b], lv[k]) {
                        b = k;
                    }
                }
                best = Some(b);
            }
        }
        picks[i] = best.unwrap();
    }
    picks
}

fn premultiply(p: PixelF) -> PixelF {
    if p.alpha <= 0.0 {
        PixelF::new(p.alpha, 0.0, 0.0, 0.0)
    } else if p.alpha < 1.0 {
        let a = p.alpha;
        p.map_rgb(|c| c * a)
    } else {
        p
    }
}

fn blur_at(line: &[PixelF], i: usize, table: &[f64]) -> PixelF {
    let (mut r, mut g, mut b, mut a, mut count) = (0.0f64, 0.0, 0.0, 0.0, 0.0);
    let mut add = |p: &PixelF, w: f64| {
        if p.alpha > 0.0 {
            r += p.red as f64 * w;
            g += p.green as f64 * w;
            b += p.blue as f64 * w;
            a += p.alpha as f64 * w;
        }
        count += w;
    };
    add(&line[i], table[0]);
    for (d, &w) in table.iter().enumerate().skip(1) {
        if i >= d {
            add(&line[i - d], w);
        }
        if i + d < line.len() {
            add(&line[i + d], w);
        }
    }
    if count <= 0.0 {
        return line[i];
    }
    let alpha = (a / count) as f32;
    if alpha <= 0.0 {
        PixelF::new(0.0, 0.0, 0.0, 0.0)
    } else {
        let div = count * alpha as f64;
        PixelF::new(
            alpha.min(1.0),
            (r / div) as f32,
            (g / div) as f32,
            (b / div) as f32,
        )
    }
}

// -------------------------------------------------------------------- blend

/// dst の下に src を敷く合成（元の PixelBlend8(src, dst)）
pub fn pixel_blend(src: PixelF, dst: PixelF) -> PixelF {
    if src.alpha <= 0.0 || dst.alpha >= 1.0 {
        return dst;
    }
    if dst.alpha <= 0.0 {
        return src;
    }
    let as2 = (1.0 - dst.alpha) * src.alpha;
    let ad2 = (dst.alpha + as2).min(1.0);
    if ad2 <= 0.0 {
        return PixelF::default();
    }
    let mix = |d: f32, s: f32| ((d * dst.alpha + s * as2) / ad2).min(1.0);
    PixelF::new(
        ad2,
        mix(dst.red, src.red),
        mix(dst.green, src.green),
        mix(dst.blue, src.blue),
    )
}

/// 通常
pub fn blend_normal(i: PixelF, o: PixelF) -> PixelF {
    pixel_blend(i, o)
}

/// 比較（明）
pub fn blend_lighten(i: PixelF, o: PixelF) -> PixelF {
    if i.alpha <= 0.0 {
        return o;
    }
    let o = pixel_blend(i, o);
    PixelF::new(
        o.alpha,
        o.red.max(i.red),
        o.green.max(i.green),
        o.blue.max(i.blue),
    )
}

/// 比較（暗）
pub fn blend_darken(i: PixelF, o: PixelF) -> PixelF {
    if i.alpha <= 0.0 {
        return o;
    }
    let o = pixel_blend(i, o);
    PixelF::new(
        o.alpha,
        o.red.min(i.red),
        o.green.min(i.green),
        o.blue.min(i.blue),
    )
}

/// スクリーン
pub fn blend_screen(i: PixelF, o: PixelF) -> PixelF {
    if i.alpha <= 0.0 {
        return o;
    }
    if o.alpha <= 0.0 {
        return i;
    }
    let a = (i.alpha + o.alpha - i.alpha * o.alpha).clamp(0.0, 1.0);
    if a <= 0.0 {
        return PixelF::default();
    }
    let (pi, po) = (premultiply(i), premultiply(o));
    let scr = |s: f32, d: f32| ((s + d - s * d) / a).clamp(0.0, 1.0);
    PixelF::new(
        a,
        scr(pi.red, po.red),
        scr(pi.green, po.green),
        scr(pi.blue, po.blue),
    )
}

/// 乗算
pub fn blend_multiply(i: PixelF, o: PixelF) -> PixelF {
    if i.alpha <= 0.0 {
        return o;
    }
    if o.alpha <= 0.0 {
        return i;
    }
    let a = (i.alpha + o.alpha - i.alpha * o.alpha).clamp(0.0, 1.0);
    if a <= 0.0 {
        return PixelF::default();
    }
    // 白マットへ
    let white = |c: f32, al: f32| c * al + 1.0 - al;
    let mul = |s: f32, d: f32| {
        let m = white(s, i.alpha) * white(d, o.alpha);
        ((m - 1.0 + a) / a).clamp(0.0, 1.0)
    };
    PixelF::new(
        a,
        mul(i.red, o.red),
        mul(i.green, o.green),
        mul(i.blue, o.blue),
    )
}

/// 不透明度で元画像 i と処理結果 o を混ぜる（opacity: 0.0..=1.0）
/// （元の persentBlend8 は新しいアルファで乗算していたが、ここでは各ピクセル自身のアルファで乗算する）
pub fn blend_opacity(i: PixelF, o: PixelF, opacity: f32) -> PixelF {
    if opacity >= 1.0 {
        return o;
    }
    if opacity <= 0.0 {
        return i;
    }
    let (r1, r2) = (opacity, 1.0 - opacity);
    let alpha = o.alpha * r1 + i.alpha * r2;
    if alpha <= 0.0 {
        return PixelF::new(0.0, 0.0, 0.0, 0.0);
    }
    let (pi, po) = (premultiply(i), premultiply(o));
    let mix = |d: f32, s: f32| ((d * r1 + s * r2) / alpha).clamp(0.0, 1.0);
    PixelF::new(
        alpha.min(1.0),
        mix(po.red, pi.red),
        mix(po.green, pi.green),
        mix(po.blue, pi.blue),
    )
}

// -------------------------------------------------------------------- random

/// 元プラグインの xorShift (F_SRAND / F_RAND) と同じ乱数列
#[derive(Debug, Clone)]
pub struct XorShift {
    seed: [u32; 4],
}

impl XorShift {
    pub const RAND_MAX: u32 = 0x7FFF;

    pub fn new(s: u32) -> Self {
        let mut seed = [0u32; 4];
        let mut ss = s.wrapping_add(100);
        for i in 1..=4u32 {
            ss = 1812433253u32.wrapping_mul(ss ^ (ss >> 30)).wrapping_add(i);
            seed[(i - 1) as usize] = ss;
        }
        Self { seed }
    }

    /// 0..=RAND_MAX
    pub fn next(&mut self) -> u32 {
        let t = self.seed[0] ^ (self.seed[0] << 11);
        self.seed[0] = self.seed[1];
        self.seed[1] = self.seed[2];
        self.seed[2] = self.seed[3];
        self.seed[3] = (self.seed[3] ^ (self.seed[3] >> 19)) ^ (t ^ (t >> 8));
        (self.seed[3] >> 16) & Self::RAND_MAX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque(v: f32) -> PixelF {
        PixelF::new(1.0, v, v, v)
    }

    fn row(values: &[f32]) -> Image {
        let mut img = Image::new(values.len(), 1);
        for (i, &v) in values.iter().enumerate() {
            img.data[i] = opaque(v);
        }
        img
    }

    #[test]
    fn minimax_line_window() {
        // 窓は [i - r, i + r - 1]
        let lv = [0.0, 0.0, 1.0, 0.0, 0.0, 0.0];
        let picks = minimax_line(&lv, 2, false);
        let got: Vec<f32> = picks.iter().map(|&k| lv[k]).collect();
        assert_eq!(got, vec![0.0, 1.0, 1.0, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn minimax_dilate_and_erode() {
        let mut img = row(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        img.minimax(2, ScanLine::Hor, TargetChannel::RgbAndAlpha);
        let got: Vec<f32> = img.data.iter().map(|p| p.red).collect();
        assert_eq!(got, vec![0.0, 1.0, 1.0, 1.0, 1.0, 0.0]);

        let mut img = row(&[1.0, 1.0, 0.0, 1.0, 1.0, 1.0]);
        img.minimax(-2, ScanLine::Hor, TargetChannel::RgbAndAlpha);
        let got: Vec<f32> = img.data.iter().map(|p| p.red).collect();
        assert_eq!(got, vec![1.0, 0.0, 0.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn minimax_zero_is_noop() {
        let mut img = row(&[0.1, 0.5, 0.9]);
        let before = img.clone();
        img.minimax(0, ScanLine::HorAndVer, TargetChannel::RgbAndAlpha);
        assert_eq!(img, before);
    }

    #[test]
    fn blur_keeps_flat_image() {
        let mut img = Image::new(8, 8);
        for p in &mut img.data {
            *p = PixelF::new(1.0, 0.25, 0.5, 0.75);
        }
        img.gaussian_blur(3.0, ScanLine::HorAndVer);
        for p in &img.data {
            assert!((p.red - 0.25).abs() < 1e-5);
            assert!((p.green - 0.5).abs() < 1e-5);
            assert!((p.blue - 0.75).abs() < 1e-5);
            assert!((p.alpha - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn blur_spreads_and_ignores_transparent_color() {
        // 透明ピクセルの色は混ざらない
        let mut img = Image::new(5, 1);
        img.data[2] = PixelF::new(1.0, 1.0, 0.0, 0.0);
        for i in [0, 1, 3, 4] {
            img.data[i] = PixelF::new(0.0, 0.0, 1.0, 0.0);
        }
        img.gaussian_blur(2.0, ScanLine::Hor);
        let p = img.data[1];
        assert!(p.alpha > 0.0 && p.alpha < 1.0);
        assert!((p.red - 1.0).abs() < 1e-5 && p.green.abs() < 1e-5);
    }

    #[test]
    fn mats_roundtrip() {
        let mut img = Image::new(1, 1);
        img.data[0] = PixelF::new(0.5, 0.2, 0.4, 0.8);
        let before = img.clone();
        img.to_white_mat();
        img.from_white_mat();
        let (a, b) = (img.data[0], before.data[0]);
        assert!((a.red - b.red).abs() < 1e-6 && (a.blue - b.blue).abs() < 1e-6);
        img.to_black_mat();
        img.from_black_mat();
        let a = img.data[0];
        assert!((a.green - b.green).abs() < 1e-6);
    }

    #[test]
    fn blends() {
        let i = PixelF::new(1.0, 0.2, 0.8, 0.5);
        let o = PixelF::new(1.0, 0.6, 0.4, 0.5);
        assert_eq!(blend_normal(i, o), o);
        assert_eq!(blend_lighten(i, o), PixelF::new(1.0, 0.6, 0.8, 0.5));
        assert_eq!(blend_darken(i, o), PixelF::new(1.0, 0.2, 0.4, 0.5));
        let s = blend_screen(i, o);
        assert!((s.red - (0.2 + 0.6 - 0.12)).abs() < 1e-6);
        let m = blend_multiply(i, o);
        assert!((m.red - 0.12).abs() < 1e-6);
        let h = blend_opacity(i, o, 0.5);
        assert!((h.red - 0.4).abs() < 1e-6 && (h.green - 0.6).abs() < 1e-6);
        assert_eq!(blend_opacity(i, o, 0.0), i);
        assert_eq!(blend_opacity(i, o, 1.0), o);
    }

    #[test]
    fn transparent_input_keeps_output() {
        let i = PixelF::default();
        let o = PixelF::new(0.5, 0.3, 0.3, 0.3);
        for f in [
            blend_normal,
            blend_lighten,
            blend_darken,
            blend_screen,
            blend_multiply,
        ] {
            assert_eq!(f(i, o), o);
        }
    }

    #[test]
    fn xorshift_is_deterministic_and_in_range() {
        let mut a = XorShift::new(10);
        let mut b = XorShift::new(10);
        for _ in 0..1000 {
            let v = a.next();
            assert_eq!(v, b.next());
            assert!(v <= XorShift::RAND_MAX);
        }
        assert_ne!(XorShift::new(1).next(), XorShift::new(2).next());
    }
}
