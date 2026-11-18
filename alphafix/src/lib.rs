//! F's AlphaFix の移植。
//!
//! 入力を背景色の上に合成して、不透明な絵にする。アルファが 0 の画素は
//! 背景色そのものになる。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/AlphaFix

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_8_to_16, conv_8_to_32};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    BaseColor,
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
        // 元: cs.AddColor(STR_BASE_COLOR, {255,0,0,0}, ID_BASE_COLOR, PF_ParamFlag_NONE)
        //
        // 元の表示名は "背景色" だが、AE はパラメータ名に非 ASCII を渡すと
        // ラベルを空で描いてしまうので英語にしてある。
        params.add(
            Params::BaseColor,
            "Base Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0x00,
                    green: 0x00,
                    blue: 0x00,
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
        out_data.set_return_msg("fs-rs alphafix");
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
        let base8 = params.get(Params::BaseColor)?.as_color()?.value();
        // 元は 16/32bit では CONV8TO16 / CONV8TO32 した背景色を使う。
        let base16 = conv_8_to_16(&base8);
        let base32 = conv_8_to_32(&base8);

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
                        // 元は入力画素 (inP) を読む。
                        if pixel.alpha == 0 {
                            // アルファが 0 なら背景色をそのまま出す。背景色の
                            // アルファごと入るのも元のまま。
                            *out_pixel = base8;
                        } else {
                            let m = MAX_CHANNEL8 as i32;
                            let a = pixel.alpha as i32;
                            let inv = m - a;
                            // 元は丸めずに最後に整数除算する (切り捨て)。
                            let mix = |b: u8, c: u8| ((b as i32 * inv + c as i32 * a) / m) as u8;
                            out_pixel.alpha = MAX_CHANNEL8 as u8;
                            out_pixel.red = mix(base8.red, pixel.red);
                            out_pixel.green = mix(base8.green, pixel.green);
                            out_pixel.blue = mix(base8.blue, pixel.blue);
                        }
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        // 元は入力画素 (inP) を読む。
                        if pixel.alpha == 0 {
                            // アルファが 0 なら背景色をそのまま出す。背景色の
                            // アルファごと入るのも元のまま。
                            *out_pixel = base16;
                        } else {
                            let m = MAX_CHANNEL16 as i32;
                            let a = pixel.alpha as i32;
                            let inv = m - a;
                            // 元は丸めずに最後に整数除算する (切り捨て)。
                            let mix = |b: u16, c: u16| ((b as i32 * inv + c as i32 * a) / m) as u16;
                            out_pixel.alpha = MAX_CHANNEL16 as u16;
                            out_pixel.red = mix(base16.red, pixel.red);
                            out_pixel.green = mix(base16.green, pixel.green);
                            out_pixel.blue = mix(base16.blue, pixel.blue);
                        }
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        if pixel.alpha == 0.0 {
                            *out_pixel = base32;
                        } else {
                            let a = pixel.alpha;
                            let inv = 1.0 - a;
                            out_pixel.alpha = 1.0;
                            out_pixel.red = base32.red * inv + pixel.red * a;
                            out_pixel.green = base32.green * inv + pixel.green * a;
                            out_pixel.blue = base32.blue * inv + pixel.blue * a;
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
