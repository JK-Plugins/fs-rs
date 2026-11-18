//! F's FillColor の移植。
//!
//! RGB を指定色で塗る。不透明度 100% なら色そのまま、それ未満は元の色と
//! 混ぜる。アルファは触らない。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/FillColor

use after_effects::{self as ae};

use libs::utils::{
    conv_8_to_16, conv_8_to_32, round_byte_fp_double, round_fp_short, round_short_fp_double,
};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Enabled,
    FillColor,
    Opacity,
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
        // 元: PF_ADD_CHECKBOX(STR_ENABLED_CB1, STR_ENABLED_CB2, FALSE, 0, ID_ENABLED_CB)
        //
        // 元の表示名は "実行する" / 色 / 不透明度 だが、AE は非 ASCII の
        // パラメータ名をラベル空で描くので英語にしてある。
        params.add(
            Params::Enabled,
            "Enable",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_COLOR, 0xff, 0x00, 0x00, ID_FILL_COLOR)
        params.add(
            Params::FillColor,
            "Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0x00,
                    blue: 0x00,
                });
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FIXED(STR_OPA_FIXED, 0, 100, 0, 100, 100, 1, 0, 0, ID_OPA_COLOR_FIXED)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている
        // (AE は型が変わっても保存値を復元する)。
        params.add(
            Params::Opacity,
            "Opacity",
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
        out_data.set_return_msg("fs-rs fillcolor");
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
        let enabled = params.get(Params::Enabled)?.as_checkbox()?.value();
        let col8 = params.get(Params::FillColor)?.as_color()?.value();
        // 元の GetParams: 負なら 0 にして /100。opa2 は 1 - opa。
        let raw = params.get(Params::Opacity)?.as_float_slider()?.value() as f32;
        let opa = if raw < 0.0 { 0.0 } else { raw } / 100.0;
        let opa2 = 1.0 - opa;

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: コピーしたあと、チェックが入っていて不透明度が 0 より
        // 大きいときだけ画素を走る。
        if !enabled || opa <= 0.0 {
            return Ok(());
        }

        // 元は 16/32bit では CONV8TO16 / CONV8TO32 した色を使う。
        let col16 = conv_8_to_16(&col8);
        let col32 = conv_8_to_32(&col8);

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
                    (ae::GenericPixel::Pixel8(_), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        if opa >= 1.0 {
                            out_pixel.red = col8.red;
                            out_pixel.green = col8.green;
                            out_pixel.blue = col8.blue;
                        } else {
                            // 元は PF_FpLong (double) で掛けて足す。f32 で
                            // 計算すると丸め境界で 1 ずれる。
                            let mix = |f: u8, c: u8| {
                                round_byte_fp_double(f as f64 * opa as f64 + c as f64 * opa2 as f64)
                            };
                            out_pixel.red = mix(col8.red, out_pixel.red);
                            out_pixel.green = mix(col8.green, out_pixel.green);
                            out_pixel.blue = mix(col8.blue, out_pixel.blue);
                        }
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        if opa >= 1.0 {
                            out_pixel.red = col16.red;
                            out_pixel.green = col16.green;
                            out_pixel.blue = col16.blue;
                        } else {
                            // 元は PF_FpLong (double) で掛けて足す。f32 で
                            // 計算すると丸め境界で 1 ずれる。
                            let mix = |f: u16, c: u16| {
                                round_short_fp_double(
                                    f as f64 * opa as f64 + c as f64 * opa2 as f64,
                                )
                            };
                            out_pixel.red = mix(col16.red, out_pixel.red);
                            out_pixel.green = mix(col16.green, out_pixel.green);
                            out_pixel.blue = mix(col16.blue, out_pixel.blue);
                        }
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        if opa >= 1.0 {
                            out_pixel.red = col32.red;
                            out_pixel.green = col32.green;
                            out_pixel.blue = col32.blue;
                        } else {
                            let mix = |f: f32, c: f32| round_fp_short(f * opa + c * opa2);
                            out_pixel.red = mix(col32.red, out_pixel.red);
                            out_pixel.green = mix(col32.green, out_pixel.green);
                            out_pixel.blue = mix(col32.blue, out_pixel.blue);
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
