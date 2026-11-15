//! F's RgbToAlpha の移植。
//!
//! 輝度をアルファに移し、RGB は指定色で塗りつぶす。元実装
//! (Legacy/Project/RgbToAlpha/RgbToAlpha.cpp) は深度ごとに式の組み立てが
//! 微妙に違うので、それぞれ書かれたとおりに再現している。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/RgbToAlpha

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_fp_long, round_short};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    FillMode,
    CustomColor,
    Reverse,
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
        // 元: PF_ADD_POPUP(STR_POP1, 3, 1, "白|黒|指定色", ID_POP)
        // 元のラベルは "塗りつぶし色" で項目は 白 / 黒 / 指定色。PF のパラメータ名は
        // ASCII でないと AE 側で空文字になるので英語にしている。
        params.add(
            Params::FillMode,
            "Fill Color",
            ae::PopupDef::setup(|f| {
                f.set_options(&["White", "Black", "Custom"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_COLOR, 0xFF, 0x00, 0x00, ID_COLOR) — 既定は赤。
        // 元のラベルは "指定色"。
        params.add(
            Params::CustomColor,
            "Custom Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    red: 0xFF,
                    green: 0x00,
                    blue: 0x00,
                    alpha: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_CHECKBOX(STR_REV1, STR_REV2, FALSE, 0, ID_CB)
        // 元のラベルは "Alphaを反転"、チェックボックス横の文字は "Rev"。
        params.add(
            Params::Reverse,
            "Invert Alpha",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("Rev");
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
        out_data.set_return_msg("fs-rs rgbtoalpha");
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
        // 元の GetParams: ポップアップ 1=白 2=黒 3=指定色。
        let col = match params.get(Params::FillMode)?.as_popup()?.value() {
            1 => ae::Pixel8 { alpha: 0xFF, red: 0xFF, green: 0xFF, blue: 0xFF },
            2 => ae::Pixel8 { alpha: 0xFF, red: 0x00, green: 0x00, blue: 0x00 },
            _ => params.get(Params::CustomColor)?.as_color()?.value(),
        };
        let rev = params.get(Params::Reverse)?.as_checkbox()?.value();

        // 16/32bit は深度ごとの最大値に合わせた色が必要。
        let col16 = libs::utils::conv_8_to_16(&col);
        let col32 = libs::utils::conv_8_to_32(&col);

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
                    (ae::GenericPixel::Pixel8(p), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        // 元: v = ((r*77) + (g*151) + (b*28)) * alpha / (256 * 255)
                        // 一息に整数で割るので、16bit 版とは丸めが違う。
                        let v = (p.red as i32 * 77 + p.green as i32 * 151 + p.blue as i32 * 28)
                            * p.alpha as i32
                            / (256 * MAX_CHANNEL8 as i32);
                        out_pixel.red = col.red;
                        out_pixel.green = col.green;
                        out_pixel.blue = col.blue;
                        let a = round_byte_fp_long(v as f32);
                        // 元は XOR。0..255 では 255 - v と同じ。
                        out_pixel.alpha = if rev { a ^ MAX_CHANNEL8 as u8 } else { a };
                    }
                    (ae::GenericPixel::Pixel16(p), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        // 元: v = ((r*77) + (g*151) + (b*28)) / 256 を先に切り捨ててから
                        // v * alpha / MAX_CHAN16。8bit 版と割る順序が違う。
                        let v = (p.red as i64 * 77 + p.green as i64 * 151 + p.blue as i64 * 28) / 256;
                        let v = v * p.alpha as i64 / MAX_CHANNEL16 as i64;
                        out_pixel.red = col16.red;
                        out_pixel.green = col16.green;
                        out_pixel.blue = col16.blue;
                        let a = round_short(v as i32);
                        out_pixel.alpha = if rev { MAX_CHANNEL16 as u16 - a } else { a };
                    }
                    (ae::GenericPixel::PixelF32(p), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        // 元: v = (r*77/256 + g*151/256 + b*28/256) * alpha
                        let v = (p.red * 77.0 / 256.0
                            + p.green * 151.0 / 256.0
                            + p.blue * 28.0 / 256.0)
                            * p.alpha;
                        out_pixel.red = col32.red;
                        out_pixel.green = col32.green;
                        out_pixel.blue = col32.blue;
                        out_pixel.alpha = if rev { 1.0 - v } else { v };
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
