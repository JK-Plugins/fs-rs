//! F's LineDetection の移植。
//!
//! 絵の中の「線になりそうなところ」を拾って、そこを指定色で塗った
//! 一枚の絵にする。拾い方は2つあり、どちらも**まわりの8画素との差**を
//! 見るだけ。
//!
//! - RGB の差: 自分の明るさ `rgbMax` と、8近傍それぞれの `rgbMax` の差の
//!   うち**まわりが明るいほう**の最大 (`MAX(近傍 - 自分, 0)`)。暗い側に
//!   印が付くので、線そのものが拾われる
//! - アルファの差: 向きが逆で `MAX(自分 - 近傍, 0)`。切り抜きの
//!   **内側**に印が付く
//!
//! 出力は全画素が「指定色の RGB + 拾った濃さのアルファ」になる
//! (元の絵は残らない)。両方 OFF なら入力をそのままコピーする。
//!
//! 元から写した細かいところ:
//!
//! - `rgbMax` は深度ごとに式が違う。8bit は `(max+min) * (α+1) >> 9`、
//!   16bit は `((max+min) >> 1) * α / PF_MAX_CHAN16` を 0..MAX で挟む、
//!   32bit は `(max+min)/2 * α` を下だけ 0 で挟む。上を挟まないので
//!   32bit は 1.0 を超えうる
//! - 濃度は `GetFIXED_PER2ONE`、つまり固定小数を**整数のまま 100 で
//!   割る**。100% がちょうど 1.0 (65536)
//! - 掛け方も深度ごとに違う。8bit は `差 * lv >> 16`、16bit は
//!   `lv` を先に `>> 4` してから `差 * lv >> 16` ではなく `>> 12`
//!   (16bit の差が 32768 まであるので、掛ける前に目盛りを落として
//!   桁あふれを避けている)、32bit は実数のまま掛ける
//! - 両方 ON のときの重ね方はスクリーン。`p0 + p1 - (p0 * p1 >> 8)`
//!   (16bit は `>> 15`、32bit は実数の積)。8bit の `>> 8` は
//!   PF_MAX_CHAN8 が 255 なのに 256 で割る、元のライブラリ全体で
//!   共通の近似
//! - 16bit の `p0 * p1` は濃度を上げると `long` (32bit) からあふれる。
//!   出荷ビルド (MSVC) と同じになるよう、あふれをそのまま回している
//! - 差を見るのは**入力ワールド**なので、書き込みの順は結果に効かない
//! - 画面の端は、はみ出す向きの近傍だけを見ない (外側を 0 として
//!   扱うのではない)
//!
//! 元と違えたところ:
//!
//! - パラメータ名を英語にした。元はすべて日本語で、出荷 .aex では
//!   "RGBo" / "RGBoZx" / "Alphao" / "AlphaoZx" / "oF" と文字化けしている
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/LineDetection

use after_effects::{self as ae, MAX_CHANNEL16};
use libs::utils::{
    conv_8_to_16, conv_8_to_32, flt2fix, round_byte_long, round_fp_short_double, round_short,
};

/// まわりの8画素。元は向きごとに別の `if` で書いてあるが、どれも
/// 「はみ出すなら見ない」なので同じ。最大を取るだけなので順も効かない。
const NEIGHBORS: [(i64, i64); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    DeltaOn,
    DeltaOpacity,
    AlphaOn,
    AlphaOpacity,
    LineColor,
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
        // 元: PF_ADD_CHECKBOX("RGB差分検出", "ON", TRUE, ..)
        params.add(
            Params::DeltaOn,
            "RGB_Detect",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("RGB差分検出の濃度", 0, 500, 0, 300, 100, 1, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        params.add(
            Params::DeltaOpacity,
            "RGB_Detect_Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(300.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("Alpha差分検出", "ON", FALSE, ..)
        params.add(
            Params::AlphaOn,
            "Alpha_Detect",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::AlphaOpacity,
            "Alpha_Detect_Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(300.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("検出した線の色", 0, 0, 0)
        params.add(
            Params::LineColor,
            "Line_Color",
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
                out_data.set_return_msg("fs-rs linedetection");
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
        let delta_on = params.get(Params::DeltaOn)?.as_checkbox()?.value();
        let alpha_on = params.get(Params::AlphaOn)?.as_checkbox()?.value();
        // 元の GetFIXED_PER2ONE: 固定小数のまま 100 で割る (整数の割り算)。
        let lv_d = flt2fix(params.get(Params::DeltaOpacity)?.as_float_slider()?.value()) / 100;
        let lv_a = flt2fix(params.get(Params::AlphaOpacity)?.as_float_slider()?.value()) / 100;
        let color = params.get(Params::LineColor)?.as_color()?.value();

        // 元の Exec: 両方 OFF なら入力をそのままコピーして終わり。
        if !delta_on && !alpha_on {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        let (iw, ih) = (in_layer.width() as i64, in_layer.height() as i64);
        let depth = out_layer.bit_depth();

        // 元の pixDelta*: 自分とまわりの差のうち、向きの合うほうの最大。
        // `rgb` が真なら明るさの差 (まわり - 自分)、偽ならアルファの差
        // (自分 - まわり)。
        macro_rules! delta {
            ($x:expr, $y:expr, $get:expr, $rgb:expr, $zero:expr) => {{
                let pv = $get($x as i64, $y as i64);
                let mut v1 = $zero;
                for (dx, dy) in NEIGHBORS {
                    let (nx, ny) = ($x as i64 + dx, $y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= iw || ny >= ih {
                        continue;
                    }
                    let nv = $get(nx, ny);
                    let d = if $rgb { nv - pv } else { pv - nv };
                    if d > v1 {
                        v1 = d;
                    }
                }
                v1
            }};
        }

        match depth {
            8 => {
                // 元の rgbMax8。
                let bright = |x: i64, y: i64| -> i32 {
                    let c = in_layer.as_pixel8(x as usize, y as usize);
                    if c.alpha == 0 {
                        return 0;
                    }
                    let m0 = c.red.max(c.green).max(c.blue) as i32;
                    let m1 = c.red.min(c.green).min(c.blue) as i32;
                    ((m0 + m1) * (c.alpha as i32 + 1)) >> 9
                };
                let opacity =
                    |x: i64, y: i64| -> i32 { in_layer.as_pixel8(x as usize, y as usize).alpha as i32 };
                for y in 0..h {
                    for x in 0..w {
                        let a = if delta_on && alpha_on {
                            let p0 = (delta!(x, y, bright, true, 0) * lv_d) >> 16;
                            let p1 = (delta!(x, y, opacity, false, 0) * lv_a) >> 16;
                            round_byte_long(p0 + p1 - ((p0 * p1) >> 8))
                        } else if delta_on {
                            round_byte_long((delta!(x, y, bright, true, 0) * lv_d) >> 16)
                        } else {
                            round_byte_long((delta!(x, y, opacity, false, 0) * lv_a) >> 16)
                        };
                        *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 { alpha: a, ..color };
                    }
                }
            }
            16 => {
                // 元の rgbMax16。8bit と違い `>> 1` を先にして、アルファは
                // 掛けてから PF_MAX_CHAN16 で割る (整数の割り算)。
                let bright = |x: i64, y: i64| -> i32 {
                    let c = in_layer.as_pixel16(x as usize, y as usize);
                    if c.alpha == 0 {
                        return 0;
                    }
                    let m0 = c.red.max(c.green).max(c.blue) as i32;
                    let m1 = c.red.min(c.green).min(c.blue) as i32;
                    let mp = ((m0 + m1) >> 1) * c.alpha as i32 / MAX_CHANNEL16 as i32;
                    mp.clamp(0, MAX_CHANNEL16 as i32)
                };
                let opacity = |x: i64, y: i64| -> i32 {
                    in_layer.as_pixel16(x as usize, y as usize).alpha as i32
                };
                // 元: 掛ける前に目盛りを 4bit 落とし、積を `>> 12` で戻す。
                let (lv_d, lv_a) = (lv_d >> 4, lv_a >> 4);
                let col = conv_8_to_16(&color);
                for y in 0..h {
                    for x in 0..w {
                        let a = if delta_on && alpha_on {
                            let p0 = (delta!(x, y, bright, true, 0) * lv_d) >> 12;
                            let p1 = (delta!(x, y, opacity, false, 0) * lv_a) >> 12;
                            // 元は `long` の積なので、濃度を上げるとあふれる。
                            // 出荷ビルドと同じになるようあふれを回す。
                            round_short(p0 + p1 - (p0.wrapping_mul(p1) >> 15))
                        } else if delta_on {
                            round_short((delta!(x, y, bright, true, 0) * lv_d) >> 12)
                        } else {
                            round_short((delta!(x, y, opacity, false, 0) * lv_a) >> 12)
                        };
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 { alpha: a, ..col };
                    }
                }
            }
            _ => {
                // 元の rgbMax32。上を挟まないので 1.0 を超えうる。
                let bright = |x: i64, y: i64| -> f32 {
                    let c = in_layer.as_pixel32(x as usize, y as usize);
                    if c.alpha == 0.0 {
                        return 0.0;
                    }
                    let m0 = c.red.max(c.green).max(c.blue);
                    let m1 = c.red.min(c.green).min(c.blue);
                    let mp = (m0 + m1) / 2.0 * c.alpha;
                    if mp < 0.0 {
                        0.0
                    } else {
                        mp
                    }
                };
                let opacity =
                    |x: i64, y: i64| -> f32 { in_layer.as_pixel32(x as usize, y as usize).alpha };
                let lv_d = lv_d as f32 / 65536.0;
                let lv_a = lv_a as f32 / 65536.0;
                let col = conv_8_to_32(&color);
                for y in 0..h {
                    for x in 0..w {
                        let a = if delta_on && alpha_on {
                            let p0 = delta!(x, y, bright, true, 0.0) * lv_d;
                            let p1 = delta!(x, y, opacity, false, 0.0) * lv_a;
                            round_fp_short_double((p0 + p1 - p0 * p1) as f64)
                        } else if delta_on {
                            round_fp_short_double((delta!(x, y, bright, true, 0.0) * lv_d) as f64)
                        } else {
                            round_fp_short_double((delta!(x, y, opacity, false, 0.0) * lv_a) as f64)
                        };
                        *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 { alpha: a, ..col };
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_percentage_becomes_a_fixed_point_one() {
        // 元の GetFIXED_PER2ONE: 固定小数を整数のまま 100 で割る。
        assert_eq!(flt2fix(100.0) / 100, 65536);
        assert_eq!(flt2fix(50.0) / 100, 32768);
        assert_eq!(flt2fix(500.0) / 100, 327680);
        assert_eq!(flt2fix(0.0) / 100, 0);
    }

    #[test]
    fn the_8bit_brightness_spans_the_whole_range() {
        // 元: (max + min) * (α + 1) >> 9。白で不透明なら 255、
        // 透明なら中身に関係なく 0。
        let bright = |a: u8, r: u8, g: u8, b: u8| -> i32 {
            if a == 0 {
                return 0;
            }
            let m0 = r.max(g).max(b) as i32;
            let m1 = r.min(g).min(b) as i32;
            ((m0 + m1) * (a as i32 + 1)) >> 9
        };
        assert_eq!(bright(255, 255, 255, 255), 255);
        assert_eq!(bright(255, 0, 0, 0), 0);
        assert_eq!(bright(0, 255, 255, 255), 0);
        // 彩度のある色は max と min の平均なので、赤でも半分に届かない。
        assert_eq!(bright(255, 255, 0, 0), 127);
    }

    #[test]
    fn the_16bit_shifts_cancel_out() {
        // 元: lv を `>> 4` してから積を `>> 12`。合わせて `/ 65536`
        // と同じになる。100% なら差がそのまま通る。
        let lv = (flt2fix(100.0) / 100) >> 4;
        for d in [0, 1, 1000, 32768] {
            assert_eq!((d * lv) >> 12, d, "delta {d}");
        }
    }

    #[test]
    fn the_two_directions_are_opposite() {
        // RGB はまわりが明るいほうを、アルファは自分が濃いほうを拾う。
        // どちらも下は 0 で止まる。
        let rgb = |near: i32, me: i32| (near - me).max(0);
        let alpha = |near: i32, me: i32| (me - near).max(0);
        assert_eq!(rgb(200, 50), 150);
        assert_eq!(rgb(50, 200), 0);
        assert_eq!(alpha(50, 200), 150);
        assert_eq!(alpha(200, 50), 0);
    }

    #[test]
    fn the_screen_blend_saturates_at_the_top() {
        // 元: p0 + p1 - (p0 * p1 >> 8)。どちらかが 255 なら 255 のまま。
        let screen = |p0: i32, p1: i32| round_byte_long(p0 + p1 - ((p0 * p1) >> 8));
        assert_eq!(screen(0, 0), 0);
        assert_eq!(screen(255, 0), 255);
        assert_eq!(screen(0, 255), 255);
        assert_eq!(screen(255, 255), 255);
        assert_eq!(screen(128, 128), 192);
    }
}
