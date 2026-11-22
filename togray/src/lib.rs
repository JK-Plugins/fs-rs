//! F's ToGray の移植。
//!
//! 8通りのグレー化 (YUV 輝度 / HLS の L / RGB 平均 / R・G・B 単独 /
//! RGB 最大値 / Lab の L) のどれかでグレーを作り、元の色と混ぜる。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ToGray

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::lab::rgb_to_lab;
use libs::utils::{
    conv_16_to_32_channel, conv_8_to_32_channel, round_byte_fp_long, round_fp_short,
    round_short_fp_long,
};

/// 元の M_MODE 相当。ポップアップは 1 始まり。
const MODE_YUV: i32 = 1;
const MODE_HLS: i32 = 2;
const MODE_RGB_AVG: i32 = 3;
const MODE_R: i32 = 4;
const MODE_G: i32 = 5;
const MODE_B: i32 = 6;
const MODE_RGB_MAX: i32 = 7;
const MODE_LAB: i32 = 8;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
    OrgBlend,
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
        // 元: PF_ADD_POPUP(STR_GRAY_MODE_POP1, 8, 1,
        //       "(Y)uv|h(L)s|(R+G+B)/3|(R)gb|r(G)b|rg(B)|RGBMax|(L)ab", ID_GRAY_MODE)
        params.add(
            Params::Mode,
            "mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "(Y)uv",
                    "h(L)s",
                    "(R+G+B)/3",
                    "(R)gb",
                    "r(G)b",
                    "rg(B)",
                    "RGBMax",
                    "(L)ab",
                ]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FIXED(STR_ORG_BLEND, 0, 100, 0, 100, 0, 2, 0, 0, ID_ORG_BLEND)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーに
        // している (AE は型が変わっても保存値を復元する)。
        params.add(
            Params::OrgBlend,
            "blend original(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(2);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
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
                self.about(&mut out_data);
            }
            ae::Command::GlobalSetup => {
                self.global_setup(&in_data)?;
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.legacy_render(&in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { extra } => {
                self.smart_pre_render(&in_data, extra, params)?;
            }
            ae::Command::SmartRender { extra } => {
                self.smart_render(&in_data, extra, params)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn about(&mut self, out_data: &mut OutData) {
        out_data.set_return_msg("fs-rs togray");
    }

    fn global_setup(&mut self, in_data: &InData) -> Result<(), ae::Error> {
        win_dbg_logger::DEBUGGER_LOGGER.set_force_log_without_debugger(true);
        log::info!("GlobalSetup");
        // For Premiere - declare supported pixel formats
        if in_data.is_premiere() {
            let suite = ae::pf::suites::PixelFormat::new()?;

            suite.clear_supported_pixel_formats(in_data.effect_ref())?;
            let formats = [
                ae::pr::PixelFormat::Bgra4444_8u,
                ae::pr::PixelFormat::Bgra4444_16u,
                ae::pr::PixelFormat::Bgra4444_32f,
            ];
            for x in formats {
                suite.add_supported_pixel_format(in_data.effect_ref(), x)?;
            }
        }
        Ok(())
    }

    fn legacy_render(
        &mut self,
        in_data: &InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        if !in_data.is_premiere() {
            // We don't support non-SmartFX unless it's Premiere
            return Err(Error::BadCallbackParameter);
        }

        self.do_render(in_data, in_layer, out_layer, params)?;

        Ok(())
    }

    fn smart_pre_render(
        &mut self,
        in_data: &InData,
        mut extra: ae::PreRenderExtra,
        _params: &mut ae::Parameters<Params>,
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

    fn smart_render(
        &mut self,
        in_data: &InData,
        extra: ae::SmartRenderExtra,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        let cb = extra.callbacks();
        let Some(input_world) = cb.checkout_layer_pixels(0)? else {
            return Ok(());
        };

        if let Ok(Some(output_world)) = cb.checkout_output() {
            self.do_render(in_data, input_world, output_world, params)?;
        }

        cb.checkin_layer_pixels(0)?;
        Ok(())
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let mode = params.get(Params::Mode)?.as_popup()?.value();
        // 元の GetParams は GetFIXED2FpShort_P1 で /100 する。
        let blend = params.get(Params::OrgBlend)?.as_float_slider()?.value() as f32 / 100.0;

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        let progress_final = out_layer.height() as _;
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |_x: i32,
             _y: i32,
             pixel: ae::GenericPixel,
             out_pixel: ae::GenericPixelMut|
             -> Result<(), Error> {
                match (pixel, out_pixel) {
                    (ae::GenericPixel::Pixel8(pixel), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        let m = MAX_CHANNEL8 as i32;
                        let (r, g, b) = (pixel.red as i32, pixel.green as i32, pixel.blue as i32);
                        // 元はモードごとに別関数。v は深度そのままのスケール。
                        let v = match mode {
                            // 元は double で計算して A_long へキャストするので切り捨て。
                            MODE_YUV => {
                                (0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64)
                                    as i32
                            }
                            // 元は >>1 (算術シフト)。
                            MODE_HLS => (r.max(g).max(b) + r.min(g).min(b)) >> 1,
                            MODE_RGB_AVG => (r + g + b) / 3,
                            MODE_R => r,
                            MODE_G => g,
                            MODE_B => b,
                            MODE_RGB_MAX => r.max(g).max(b),
                            MODE_LAB => {
                                // 元は出力画素 (コピー済みなので入力と同じ) を
                                // 32bit に変換してから Lab にする。
                                // 元は conv_8_to_32_channel8TO32 / conv_8_to_32_channel16TO32 を通すので、
                                // Lab に渡る値は小数4桁に量子化されている。
                                let lab = rgb_to_lab(
                                    conv_8_to_32_channel(pixel.red) as f64,
                                    conv_8_to_32_channel(pixel.green) as f64,
                                    conv_8_to_32_channel(pixel.blue) as f64,
                                    1.0,
                                );
                                // 元は double のまま *最大値 +0.5 して A_long に
                                // キャストする。f32 に落とすと丸め境界で 1 ずれる
                                // ので f64 のまま計算する。
                                (lab.l * m as f64 + 0.5) as i32
                            }
                            _ => 0,
                        };
                        // 元の GMIX: v を 0..最大値 にクランプし、blend<=0 なら
                        // グレーをそのまま、そうでなければ元の色と混ぜる。
                        let v = v.clamp(0, m);
                        out_pixel.alpha = pixel.alpha;
                        if blend <= 0.0 {
                            out_pixel.red = v as u8;
                            out_pixel.green = v as u8;
                            out_pixel.blue = v as u8;
                        } else {
                            let d1 = 1.0 - blend;
                            let mix = |c: i32| round_byte_fp_long(c as f32 * blend + v as f32 * d1);
                            out_pixel.red = mix(r);
                            out_pixel.green = mix(g);
                            out_pixel.blue = mix(b);
                        }
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as i32;
                        let (r, g, b) = (pixel.red as i32, pixel.green as i32, pixel.blue as i32);
                        // 元はモードごとに別関数。v は深度そのままのスケール。
                        let v = match mode {
                            // 元は double で計算して A_long へキャストするので切り捨て。
                            MODE_YUV => {
                                (0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64)
                                    as i32
                            }
                            // 元は >>1 (算術シフト)。
                            MODE_HLS => (r.max(g).max(b) + r.min(g).min(b)) >> 1,
                            MODE_RGB_AVG => (r + g + b) / 3,
                            MODE_R => r,
                            MODE_G => g,
                            MODE_B => b,
                            MODE_RGB_MAX => r.max(g).max(b),
                            MODE_LAB => {
                                // 元は出力画素 (コピー済みなので入力と同じ) を
                                // 32bit に変換してから Lab にする。
                                // 元は conv_16_to_32_channel8TO32 / conv_16_to_32_channel16TO32 を通すので、
                                // Lab に渡る値は小数4桁に量子化されている。
                                let lab = rgb_to_lab(
                                    conv_16_to_32_channel(pixel.red) as f64,
                                    conv_16_to_32_channel(pixel.green) as f64,
                                    conv_16_to_32_channel(pixel.blue) as f64,
                                    1.0,
                                );
                                // 元は double のまま *最大値 +0.5 して A_long に
                                // キャストする。f32 に落とすと丸め境界で 1 ずれる
                                // ので f64 のまま計算する。
                                (lab.l * m as f64 + 0.5) as i32
                            }
                            _ => 0,
                        };
                        // 元の GMIX: v を 0..最大値 にクランプし、blend<=0 なら
                        // グレーをそのまま、そうでなければ元の色と混ぜる。
                        let v = v.clamp(0, m);
                        out_pixel.alpha = pixel.alpha;
                        if blend <= 0.0 {
                            out_pixel.red = v as u16;
                            out_pixel.green = v as u16;
                            out_pixel.blue = v as u16;
                        } else {
                            let d1 = 1.0 - blend;
                            let mix =
                                |c: i32| round_short_fp_long(c as f32 * blend + v as f32 * d1);
                            out_pixel.red = mix(r);
                            out_pixel.green = mix(g);
                            out_pixel.blue = mix(b);
                        }
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let (r, g, b) = (pixel.red, pixel.green, pixel.blue);
                        let v = match mode {
                            MODE_YUV => {
                                (0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64)
                                    as f32
                            }
                            // 32bit だけ >>1 ではなく /2。
                            MODE_HLS => (r.max(g).max(b) + r.min(g).min(b)) / 2.0,
                            MODE_RGB_AVG => (r + g + b) / 3.0,
                            MODE_R => r,
                            MODE_G => g,
                            MODE_B => b,
                            MODE_RGB_MAX => r.max(g).max(b),
                            MODE_LAB => rgb_to_lab(r as f64, g as f64, b as f64, 1.0).l as f32,
                            _ => 0.0,
                        };
                        let v = v.clamp(0.0, 1.0);
                        out_pixel.alpha = pixel.alpha;
                        if blend <= 0.0 {
                            out_pixel.red = v;
                            out_pixel.green = v;
                            out_pixel.blue = v;
                        } else {
                            let d1 = 1.0 - blend;
                            let mix = |c: f32| round_fp_short(c * blend + v * d1);
                            out_pixel.red = mix(r);
                            out_pixel.green = mix(g);
                            out_pixel.blue = mix(b);
                        }
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
