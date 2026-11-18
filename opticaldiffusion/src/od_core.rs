// OpticalDiffusion の処理本体（AE に依存しない）
// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/OpticalDiffusion/DF8.cpp

use libs::fs_graphics::{
    blend_darken, blend_lighten, blend_multiply, blend_normal, blend_opacity, blend_screen, Image,
    PixelF, ScanLine, TargetChannel, XorShift,
};
use libs::fs_graphics8::{
    conv8to32, darken_blend8, float_to_fixed, lighten_blend8, multiply_blend8, normal_blend8,
    percent_blend8, round_byte_long, screen_blend8, Image8, Px8,
};
use libs::hls::HLSA;

/// Popup の値（AE の Popup は 1 始まり）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendMode {
    None = 1,
    Normal,
    Lighten,
    Darken,
    Screen,
    Multiply,
}

impl BlendMode {
    pub const NAMES: [&'static str; 6] =
        ["None", "Normal", "Lighten", "Darken", "Screen", "Multiply"];
    pub const DEFAULT: i32 = BlendMode::Normal as i32;

    pub fn from_popup(v: i32) -> Self {
        match v {
            2 => Self::Normal,
            3 => Self::Lighten,
            4 => Self::Darken,
            5 => Self::Screen,
            6 => Self::Multiply,
            _ => Self::None,
        }
    }
}

/// パラメータ。値の単位は UI と同じ（Black/White Point などは 0..255、Opacity/Noise は %）。
#[derive(Debug, Clone)]
pub struct OdParams {
    pub extract_enabled: bool,
    pub black_point: i32,
    pub white_point: i32,
    pub black_softness: i32,
    pub white_softness: i32,
    pub invert: bool,
    /// 抽出色の範囲（元は PF_Fixed。H/S の差がこの値以下なら対象）
    pub color_range: f64,
    /// RGB (0.0..=1.0)。Use Count の数だけ入れる
    pub extract_colors: Vec<[f64; 3]>,
    pub minimax_1st: i32,
    pub minimax_2nd: i32,
    pub blur: f64,
    pub blend_mode: BlendMode,
    pub blend_opacity: f64,
    pub noise: f64,
}

impl Default for OdParams {
    fn default() -> Self {
        Self {
            extract_enabled: true,
            black_point: 64,
            white_point: 128,
            black_softness: 128,
            white_softness: 64,
            invert: true,
            color_range: 0.5,
            extract_colors: Vec::new(),
            minimax_1st: -3,
            minimax_2nd: 3,
            blur: 1.0,
            blend_mode: BlendMode::Normal,
            blend_opacity: 70.0,
            noise: 1.0,
        }
    }
}

/// GetParams 相当の正規化をした値
struct Resolved {
    black_point: f32,
    white_point: f32,
    black_softness: f32,
    white_softness: f32,
    colors: Vec<HLSA>,
    opacity: f32,
    noise: f32,
}

fn resolve(p: &OdParams) -> Resolved {
    let mut black_point = p.black_point;
    let mut black_softness = p.black_softness;
    if black_point > p.white_point {
        let tmp = black_point - p.white_point;
        black_point = (black_point - tmp).max(0);
        black_softness = (black_softness - tmp).max(0);
    }

    let opacity = (p.blend_opacity / 100.0).clamp(0.0, 1.0) as f32;
    let mut noise = (p.noise / 100.0).max(0.0) as f32;
    if opacity <= 0.0 {
        noise = 0.0;
    } else if opacity < 0.25 {
        noise = noise * opacity / 0.25;
    }

    Resolved {
        black_point: black_point as f32 / 255.0,
        white_point: p.white_point as f32 / 255.0,
        black_softness: black_softness as f32 / 255.0,
        white_softness: p.white_softness as f32 / 255.0,
        colors: p
            .extract_colors
            .iter()
            .map(|c| HLSA::from_rgb(c[0], c[1], c[2], 1.0))
            .collect(),
        opacity,
        noise,
    }
}

/// 明るさ（と指定色）でアルファを作る。元の extract8。
fn extract(input: PixelF, r: &Resolved, p: &OdParams) -> PixelF {
    let a = input.alpha;
    let v = input.luma() * a;

    let b0 = r.black_point - r.black_softness;
    let b1 = r.black_point;
    let w0 = r.white_point;
    let w1 = r.white_point + r.white_softness;

    let mut v2 = if v < b0 {
        0.0
    } else if v < b1 {
        (v - b0) / (b1 - b0)
    } else if v <= w0 {
        1.0
    } else if v < w1 {
        1.0 - (v - w0) / (w1 - w0)
    } else {
        0.0
    };
    if p.invert {
        v2 = 1.0 - v2;
    }

    let mut out = input;
    out.alpha = (a * v2).clamp(0.0, 1.0);

    // 指定した色を残す
    if !r.colors.is_empty() && out.alpha < 1.0 {
        let hlsa = HLSA::from_rgb(
            input.red as f64,
            input.green as f64,
            input.blue as f64,
            input.alpha as f64,
        );
        for c in &r.colors {
            let ah = (hlsa.hue - c.hue).abs();
            let as_ = (hlsa.saturation - c.saturation).abs();
            if ah <= p.color_range && as_ <= p.color_range {
                let al = (hlsa.lightness - c.lightness).abs();
                if al < 0.25 {
                    let a2 = ((0.25 - al) / 0.25) as f32;
                    if out.alpha < a2 {
                        out.alpha = a2;
                    }
                    break;
                }
            }
        }
    }
    out
}

/// OpticalDiffusion を適用する。`seed` はノイズ用（元はフレーム番号）。
pub fn process(input: &Image, p: &OdParams, seed: u32) -> Image {
    let r = resolve(p);
    let mut out = input.clone();

    if r.opacity <= 0.0 {
        return out;
    }

    // 画像抽出
    if p.extract_enabled {
        for px in &mut out.data {
            *px = extract(*px, &r, p);
        }
    }

    // 最大・最小
    out.minimax(
        p.minimax_1st,
        ScanLine::HorAndVer,
        TargetChannel::RgbAndAlpha,
    );
    out.minimax(
        p.minimax_2nd,
        ScanLine::HorAndVer,
        TargetChannel::RgbAndAlpha,
    );

    // ぼかし
    out.gaussian_blur(p.blur, ScanLine::HorAndVer);

    // ブレンド
    let blend: Option<fn(PixelF, PixelF) -> PixelF> = match p.blend_mode {
        BlendMode::None => None,
        BlendMode::Normal => Some(blend_normal),
        BlendMode::Lighten => Some(blend_lighten),
        BlendMode::Darken => Some(blend_darken),
        BlendMode::Screen => Some(blend_screen),
        BlendMode::Multiply => Some(blend_multiply),
    };
    if let Some(f) = blend {
        for (o, i) in out.data.iter_mut().zip(&input.data) {
            *o = f(*i, *o);
        }
    }
    if r.opacity < 1.0 {
        for (o, i) in out.data.iter_mut().zip(&input.data) {
            *o = blend_opacity(*i, *o, r.opacity);
        }
    }

    // デバンド対策のノイズ
    if r.noise > 0.0 {
        let mut rng = XorShift::new(seed);
        let max = XorShift::RAND_MAX as f32;
        for o in &mut out.data {
            let rnd = r.noise * (1.0 - 2.0 * rng.next() as f32 / max);
            if rnd != 0.0 && o.alpha > 0.0 {
                o.red = (o.red + rnd).max(0.0);
                o.green = (o.green + rnd).max(0.0);
                o.blue = (o.blue + rnd).max(0.0);
            }
        }
    }

    out
}

// ==================================================================== 8bit
//
// 8bit は元の DF8.cpp と出力がビット単位で一致するよう、整数演算のまま移植している。
// 元の .aex では UI が PF_ADD_FIXED（固定小数）なので、Range / Opacity / Noise は
// 一度 Fixed に変換してから元と同じ型・順序で計算する。

/// GetParams 相当（8bit 用）
struct Resolved8 {
    black_point: i32,
    white_point: i32,
    black_softness: i32,
    white_softness: i32,
    color_range: i32,
    colors: Vec<HLSA>,
    opacity: f32,
    noise: f32,
}

fn fixed_to_short(v: f64) -> f32 {
    float_to_fixed(v) as f32 / 65536.0
}

fn resolve8(p: &OdParams) -> Resolved8 {
    let mut black_point = p.black_point;
    let mut black_softness = p.black_softness;
    if black_point > p.white_point {
        let tmp = black_point - p.white_point;
        black_point = (black_point - tmp).max(0);
        black_softness = (black_softness - tmp).max(0);
    }

    let mut opacity = fixed_to_short(p.blend_opacity) / 100.0;
    opacity = opacity.clamp(0.0, 1.0);

    let mut noise = fixed_to_short(p.noise) / 100.0;
    if noise < 0.0 {
        noise = 0.0;
    }
    if opacity <= 0.0 {
        noise = 0.0;
    } else if opacity < 0.25 {
        noise = ((noise * opacity) as f64 / 0.25) as f32;
    }

    Resolved8 {
        black_point,
        white_point: p.white_point,
        black_softness,
        white_softness: p.white_softness,
        color_range: float_to_fixed(p.color_range),
        colors: p
            .extract_colors
            .iter()
            .map(|c| {
                // 色パラメータは 8bit。CONV8TO32 してから HLS にする
                let q = |v: f64| conv8to32((v * 255.0).round().clamp(0.0, 255.0) as u8) as f64;
                HLSA::from_rgb(q(c[0]), q(c[1]), q(c[2]), 1.0)
            })
            .collect(),
        opacity,
        noise,
    }
}

/// extract8
fn extract8(input: Px8, r: &Resolved8, p: &OdParams) -> Px8 {
    let a = input.alpha as i32;
    let mut out = input;

    let lum =
        0.29891 * input.red as f64 + 0.58661 * input.green as f64 + 0.11448 * input.blue as f64;
    let v = (lum * (a as f64 / 255.0) + 0.5) as i32;

    let b0 = r.black_point - r.black_softness;
    let b1 = r.black_point;
    let w0 = r.white_point;
    let w1 = r.white_point + r.white_softness;
    let mut v2 = if v < b0 {
        0
    } else if v < b1 {
        255 * (v - b0) / (b1 - b0)
    } else if v <= w0 {
        255
    } else if v < w1 {
        255 - (255 * (v - w0) / (w1 - w0))
    } else {
        0
    };
    if p.invert {
        v2 = 255 - v2;
    }
    out.alpha = round_byte_long(a * v2 / 255);

    if !r.colors.is_empty() && out.alpha < 255 {
        let hlsa = HLSA::from_rgb(
            conv8to32(input.red) as f64,
            conv8to32(input.green) as f64,
            conv8to32(input.blue) as f64,
            conv8to32(input.alpha) as f64,
        );
        for c in &r.colors {
            let ah = ((hlsa.hue - c.hue).abs() * 65536.0) as i32;
            let as_ = ((hlsa.saturation - c.saturation).abs() * 65536.0) as i32;
            if ah <= r.color_range && as_ <= r.color_range {
                let al = ((hlsa.lightness - c.lightness).abs() * 65536.0) as i32;
                if al < 0x4000 {
                    let a2 = round_byte_long(255 * (0x4000 - al) / 0x4000);
                    if out.alpha < a2 {
                        out.alpha = a2;
                    }
                    break;
                }
            }
        }
    }
    out
}

/// DF8 相当。`frame` はノイズのシード（元は current_time / time_step）。
///
/// 注意: 元はノイズを AE の iterate8 の中でグローバルな乱数から取っているため、
/// AE がマルチスレッドで回すと元の .aex 自体の出力が毎回変わる。
/// ここでは 1 スレッドで左上から順に回したときと同じ順序で乱数を使う。
pub fn process8(input: &Image8, p: &OdParams, frame: u32) -> Image8 {
    let r = resolve8(p);
    let mut out = input.clone();

    if r.opacity <= 0.0 {
        return out;
    }

    if p.extract_enabled {
        for px in &mut out.data {
            *px = extract8(*px, &r, p);
        }
    }

    out.minimax8(p.minimax_1st);
    out.minimax8(p.minimax_2nd);

    if p.blur > 0.0 {
        out.gblur8(p.blur);
    }

    let blend: Option<fn(Px8, Px8) -> Px8> = match p.blend_mode {
        BlendMode::None => None,
        BlendMode::Normal => Some(normal_blend8),
        BlendMode::Lighten => Some(lighten_blend8),
        BlendMode::Darken => Some(darken_blend8),
        BlendMode::Screen => Some(screen_blend8),
        BlendMode::Multiply => Some(multiply_blend8),
    };
    if let Some(f) = blend {
        for (o, i) in out.data.iter_mut().zip(&input.data) {
            *o = f(*i, *o);
        }
    }
    if r.opacity < 1.0 {
        for (o, i) in out.data.iter_mut().zip(&input.data) {
            *o = percent_blend8(*i, *o, r.opacity);
        }
    }

    if r.noise > 0.0 {
        let mut rng = XorShift::new(frame);
        let v = (r.noise * 255.0) as i64;
        for o in &mut out.data {
            let rnd = (v - (v * 2 * rng.next() as i64 / XorShift::RAND_MAX as i64)) as i32;
            if rnd != 0 && o.alpha > 0 {
                o.red = round_byte_long(o.red as i32 + rnd);
                o.green = round_byte_long(o.green as i32 + rnd);
                o.blue = round_byte_long(o.blue as i32 + rnd);
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize) -> Image {
        let mut img = Image::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = x as f32 / (w - 1) as f32;
                *img.get_mut(x, y) = PixelF::new(1.0, v, v * 0.5, 1.0 - v);
            }
        }
        img
    }

    #[test]
    fn zero_opacity_returns_input() {
        let img = gradient(16, 8);
        let p = OdParams {
            blend_opacity: 0.0,
            ..Default::default()
        };
        assert_eq!(process(&img, &p, 0), img);
    }

    #[test]
    fn extract_thresholds() {
        let p = OdParams {
            invert: false,
            ..Default::default()
        };
        let r = resolve(&p);
        let gray = |v: f32| PixelF::new(1.0, v, v, v);
        // black_point(64) ～ white_point(128) は残る
        assert!((extract(gray(100.0 / 255.0), &r, &p).alpha - 1.0).abs() < 1e-4);
        // white_point + white_softness(192) 以上は消える
        assert_eq!(extract(gray(1.0), &r, &p).alpha, 0.0);
        // 途中はなだらか
        let a = extract(gray(160.0 / 255.0), &r, &p).alpha;
        assert!(a > 0.0 && a < 1.0);
        // 透明ピクセルは透明のまま
        assert_eq!(extract(PixelF::new(0.0, 0.5, 0.5, 0.5), &r, &p).alpha, 0.0);
    }

    #[test]
    fn extract_color_keeps_target() {
        let p = OdParams {
            invert: false,
            color_range: 0.05,
            extract_colors: vec![[1.0, 0.0, 0.0]],
            ..Default::default()
        };
        let r = resolve(&p);
        // 純粋な赤は輝度では消える範囲だが、指定色なので残る
        let red = PixelF::new(1.0, 1.0, 0.0, 0.0);
        assert!((extract(red, &r, &p).alpha - 1.0).abs() < 1e-6);
        // 白は指定色ではないので輝度のまま消える
        let white = PixelF::new(1.0, 1.0, 1.0, 1.0);
        assert_eq!(extract(white, &r, &p).alpha, 0.0);
    }

    #[test]
    fn black_point_above_white_point_is_clamped() {
        let p = OdParams {
            black_point: 128,
            white_point: 100,
            black_softness: 10,
            ..Default::default()
        };
        let r = resolve(&p);
        assert!((r.black_point - 100.0 / 255.0).abs() < 1e-6);
        assert_eq!(r.black_softness, 0.0);
    }

    #[test]
    fn noise_is_deterministic_per_seed() {
        let img = gradient(16, 8);
        let p = OdParams::default();
        assert_eq!(process(&img, &p, 3), process(&img, &p, 3));
        assert_ne!(process(&img, &p, 3), process(&img, &p, 4));
    }

    #[test]
    fn output_is_finite_for_all_modes() {
        let img = gradient(32, 16);
        for mode in 1..=6 {
            let p = OdParams {
                blend_mode: BlendMode::from_popup(mode),
                blend_opacity: 50.0,
                blur: 4.5,
                ..Default::default()
            };
            let out = process(&img, &p, 1);
            assert_eq!((out.width, out.height), (32, 16));
            for px in &out.data {
                for c in [px.alpha, px.red, px.green, px.blue] {
                    assert!(c.is_finite() && c >= 0.0, "mode {mode}: {px:?}");
                }
            }
        }
    }

    fn gradient8(w: usize, h: usize) -> Image8 {
        let mut img = Image8::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let v = (x * 255 / (w - 1)) as u8;
                *img.get_mut(x, y) = Px8::new(255, v, v / 2, 255 - v);
            }
        }
        img
    }

    #[test]
    fn process8_zero_opacity_returns_input() {
        let img = gradient8(16, 8);
        let p = OdParams {
            blend_opacity: 0.0,
            ..Default::default()
        };
        assert_eq!(process8(&img, &p, 0), img);
    }

    #[test]
    fn process8_runs_all_modes() {
        let img = gradient8(32, 16);
        for mode in 1..=6 {
            let p = OdParams {
                blend_mode: BlendMode::from_popup(mode),
                blend_opacity: 50.0,
                blur: 4.5,
                extract_colors: vec![[1.0, 0.0, 0.0]],
                ..Default::default()
            };
            let out = process8(&img, &p, 1);
            assert_eq!(out.data.len(), 32 * 16);
            assert_eq!(out, process8(&img, &p, 1));
        }
    }

    #[test]
    fn process8_and_float_agree_roughly() {
        // 不透明な画像なら、8bit 版と float 版の差は丸め誤差だけ
        // （半透明部分は 8bit 版が元のバグを再現しているので大きくずれる）
        let img8 = gradient8(32, 16);
        let mut imgf = Image::new(32, 16);
        for (d, s) in imgf.data.iter_mut().zip(&img8.data) {
            *d = PixelF::new(
                s.alpha as f32 / 255.0,
                s.red as f32 / 255.0,
                s.green as f32 / 255.0,
                s.blue as f32 / 255.0,
            );
        }
        let p = OdParams {
            extract_enabled: false,
            noise: 0.0,
            ..Default::default()
        };
        let o8 = process8(&img8, &p, 0);
        let of = process(&imgf, &p, 0);
        let mut max_diff = 0i32;
        for (a, b) in o8.data.iter().zip(&of.data) {
            let q = |c: f32| (c.clamp(0.0, 1.0) * 255.0).round() as i32;
            for (x, y) in [
                (a.alpha, b.alpha),
                (a.red, b.red),
                (a.green, b.green),
                (a.blue, b.blue),
            ] {
                max_diff = max_diff.max((x as i32 - q(y)).abs());
            }
        }
        assert!(max_diff <= 2, "max diff {max_diff}");
    }
}
