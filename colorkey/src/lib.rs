use after_effects::{self as ae};

use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32, round_byte_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Threshold,
    KeyColor,
    BackColor,
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
        params.add(
            Params::Threshold,
            "threshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_default(0.0);
                f.set_precision(1); // 小数点以下の桁数
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::KeyColor,
            "Key Color",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0xFF,
                    green: 0xFF,
                    blue: 0xFF,
                    alpha: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::BackColor,
            "Background Color",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0x00,
                    green: 0x00,
                    blue: 0x00,
                    alpha: 0xFF,
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
        out_data.set_return_msg("fs-rs colorkey");
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
        let threshold = params.get(Params::Threshold)?.as_float_slider()?.value();
        let threshold = (MAX_CHANNEL8 as f64 * threshold / 100.0) as u8;
        let key_color = params.get(Params::KeyColor)?.as_color()?.value();
        let back_color = params.get(Params::BackColor)?.as_color()?.value();

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
                        if threshold == 0 {
                            if pixel.red == key_color.red
                                && pixel.green == key_color.green
                                && pixel.blue == key_color.blue
                            {
                                out_pixel.red = back_color.red;
                                out_pixel.green = back_color.green;
                                out_pixel.blue = back_color.blue;
                                out_pixel.alpha = 0x00;
                            }
                        } else if pixel.red.abs_diff(key_color.red) <= threshold
                            && pixel.green.abs_diff(key_color.green) <= threshold
                            && pixel.blue.abs_diff(key_color.blue) <= threshold
                        {
                            out_pixel.red = back_color.red;
                            out_pixel.green = back_color.green;
                            out_pixel.blue = back_color.blue;
                            out_pixel.alpha = 0x00;
                        }
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let p = &conv_16_to_8(pixel);
                        let b = conv_8_to_16(&back_color);
                        if threshold == 0 {
                            if p.red == key_color.red
                                && p.green == key_color.green
                                && p.blue == key_color.blue
                            {
                                out_pixel.red = b.red;
                                out_pixel.green = b.green;
                                out_pixel.blue = b.blue;
                                out_pixel.alpha = 0x00;
                            }
                        } else if p.red.abs_diff(key_color.red) <= threshold
                            && p.green.abs_diff(key_color.green) <= threshold
                            && p.blue.abs_diff(key_color.blue) <= threshold
                        {
                            out_pixel.red = b.red;
                            out_pixel.green = b.green;
                            out_pixel.blue = b.blue;
                            out_pixel.alpha = 0x00;
                        }
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let p = &conv_32_to_8(pixel);
                        let b = conv_8_to_32(&back_color);
                        if threshold == 0 {
                            if p.red == key_color.red
                                && p.green == key_color.green
                                && p.blue == key_color.blue
                            {
                                out_pixel.red = b.red;
                                out_pixel.green = b.green;
                                out_pixel.blue = b.blue;
                                out_pixel.alpha = 0.0;
                            }
                        } else if p.red.abs_diff(key_color.red) <= threshold
                            && p.green.abs_diff(key_color.green) <= threshold
                            && p.blue.abs_diff(key_color.blue) <= threshold
                        {
                            out_pixel.red = b.red;
                            out_pixel.green = b.green;
                            out_pixel.blue = b.blue;
                            out_pixel.alpha = 0.0;
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
