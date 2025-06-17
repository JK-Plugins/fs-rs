mod utils;

use after_effects::{self as ae};

use crate::utils::{comp_pix8_lv, conv_16_to_8, conv_32_to_8};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    OptionsStart,
    OptionInvert,
    OptionsEnd,
    Target1Start,
    Target1Enabled,
    Target1Color,
    Target1End,
    Level,
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
        in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_group(
            Params::OptionsStart,
            Params::OptionsEnd,
            "Options",
            false,
            |params| {
                params.add(
                    Params::OptionInvert,
                    "Invert",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::Target1Start,
            Params::Target1End,
            "target1",
            true,
            |params| {
                params.add(
                    Params::Target1Enabled,
                    "Enabled",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_value(f.default());
                    }),
                )?;
                params.add(
                    Params::Target1Color,
                    "Target Color",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 0,
                            green: 255,
                            blue: 0,
                            alpha: 255,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add(
            Params::Level,
            "Level",
            ae::FloatSliderDef::setup(|f| {
                f.set_default(0.0);
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
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
        out_data
            .set_return_msg("SDK_Noise v5.6\rCopyright 2007-2023 Adobe Inc.\rSimple noise effect.");
    }

    fn global_setup(&mut self, in_data: &InData) -> Result<(), ae::Error> {
        win_dbg_logger::DEBUGGER_LOGGER.set_force_log_without_debugger(true);
        log::info!("GlobalSetup");
        // For Premiere - declare supported pixel formats
        if in_data.is_premiere() {
            let suite = ae::pf::suites::PixelFormat::new()?;

            // Add the pixel formats we support in order of preference.
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
        params: &mut ae::Parameters<Params>,
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
        let target1_enabled = params.get(Params::Target1Enabled)?.as_checkbox()?.value();
        let target1_color = params.get(Params::Target1Color)?.as_color()?.value();
        let invert = params.get(Params::OptionInvert)?.as_checkbox()?.value();
        let level = params.get(Params::Level)?.as_float_slider()?.value();

        let progress_final = out_layer.height() as _;
        ae::pf::suites::WorldTransform::new()?.copy_hq(in_data.effect_ref(), &in_layer, &mut out_layer, None, None)?;
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
                        let b = comp_pix8_lv(&target1_color, pixel, level as u8);
                        if b == invert {
                            out_pixel.alpha = 0;
                            out_pixel.red = MAX_CHANNEL8 as u8;
                            out_pixel.green = MAX_CHANNEL8 as u8;
                            out_pixel.blue = MAX_CHANNEL8 as u8;
                        }
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let b = comp_pix8_lv(&target1_color, &conv_16_to_8(pixel), level as u8);
                        if b == invert {
                            out_pixel.alpha = 0;
                            out_pixel.red = MAX_CHANNEL16 as u16;
                            out_pixel.green = MAX_CHANNEL16 as u16;
                            out_pixel.blue = MAX_CHANNEL16 as u16;
                        }
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let b = comp_pix8_lv(&target1_color, &conv_32_to_8(pixel), level as u8);
                        if b == invert {
                            out_pixel.alpha = 0.0;
                            out_pixel.red = 1.0;
                            out_pixel.green = 1.0;
                            out_pixel.blue = 1.0;
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
