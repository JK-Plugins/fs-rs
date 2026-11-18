//! F's Posterization8bit の移植。
//!
//! 各チャンネルを level 段階に量子化する。元実装
//! (Legacy/Project/Posterization8bit/Posterization8bit.cpp) と同じく、いったん
//! アルファを乗じた値 (マット済み) に対して量子化し、量子化後のアルファで割って
//! 戻す。grayOnly のときは輝度1本だけを量子化して RGB に入れる。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Posterization8bit

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_long, round_short};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Level,
    GrayOnly,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// level 段階に量子化して 0..1 に戻す。
///
/// 元の 2 行に対応する:
///
/// ```c
/// A_long v = (A_long)((double)level * x + 0.5);
/// v = (A_long)((double)v * MAX / (double)level + 0.5);
/// ```
fn quantize(x: f64, level: f64, max: f64) -> f64 {
    let v = (level * x + 0.5) as i64 as f64;
    (v * max / level + 0.5) as i64 as f64
}

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
        // 元: PF_ADD_SLIDER(STR_LEVEL, 2, 64, 2, 12, 12, ID_LEVEL)
        // 入力は 2..64 だがスライダーは 2..12 まで。
        params.add(
            Params::Level,
            "level",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(64);
                f.set_slider_min(2);
                f.set_slider_max(12);
                f.set_default(12);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_CHECKBOX(STR_G, "on", TRUE, 0, ID_GRAY) — 既定はオン。
        params.add(
            Params::GrayOnly,
            "grayOnly",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
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
        out_data.set_return_msg("fs-rs posterization8bit");
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
        let level = params.get(Params::Level)?.as_slider()?.value().max(2) as f64;
        let gray_only = params.get(Params::GrayOnly)?.as_checkbox()?.value();

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
                        let max = MAX_CHANNEL8 as f64;
                        if out_pixel.alpha == 0 {
                            *out_pixel = ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            };
                            return Ok(());
                        }
                        let a = out_pixel.alpha as f64 / max;
                        let r = out_pixel.red as f64 * a / max;
                        let g = out_pixel.green as f64 * a / max;
                        let b = out_pixel.blue as f64 * a / max;

                        let va = quantize(a, level, max);
                        // 元は va を除数に使うので、量子化でアルファが 0 に落ちると
                        // 0 除算になる (alpha=1, level=12 で起きる)。そのときは
                        // アルファ 0 と同じ扱いにして透明にする。
                        if va <= 0.0 {
                            *out_pixel = ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            };
                            return Ok(());
                        }
                        out_pixel.alpha = round_byte_long(va as i32);

                        let unmat = |c: f64| {
                            round_byte_long((quantize(c, level, max) * max / va + 0.5) as i32)
                        };
                        if gray_only {
                            let y = 0.299 * r + 0.587 * g + 0.114 * b;
                            let v = unmat(y);
                            out_pixel.red = v;
                            out_pixel.green = v;
                            out_pixel.blue = v;
                        } else {
                            out_pixel.red = unmat(r);
                            out_pixel.green = unmat(g);
                            out_pixel.blue = unmat(b);
                        }
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let max = MAX_CHANNEL16 as f64;
                        if out_pixel.alpha == 0 {
                            *out_pixel = ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            };
                            return Ok(());
                        }
                        let a = out_pixel.alpha as f64 / max;
                        let r = out_pixel.red as f64 * a / max;
                        let g = out_pixel.green as f64 * a / max;
                        let b = out_pixel.blue as f64 * a / max;

                        let va = quantize(a, level, max);
                        if va <= 0.0 {
                            *out_pixel = ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            };
                            return Ok(());
                        }
                        out_pixel.alpha = round_short(va as i32);

                        let unmat =
                            |c: f64| round_short((quantize(c, level, max) * max / va + 0.5) as i32);
                        if gray_only {
                            let y = 0.299 * r + 0.587 * g + 0.114 * b;
                            let v = unmat(y);
                            out_pixel.red = v;
                            out_pixel.green = v;
                            out_pixel.blue = v;
                        } else {
                            out_pixel.red = unmat(r);
                            out_pixel.green = unmat(g);
                            out_pixel.blue = unmat(b);
                        }
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        if out_pixel.alpha == 0.0 {
                            out_pixel.red = 0.0;
                            out_pixel.green = 0.0;
                            out_pixel.blue = 0.0;
                            return Ok(());
                        }
                        // 32bit は 0..1 のままなので max を掛けずに量子化する。
                        let a0 = out_pixel.alpha as f64;
                        let r = out_pixel.red as f64 * a0;
                        let g = out_pixel.green as f64 * a0;
                        let b = out_pixel.blue as f64 * a0;

                        let va = (level * a0 + 0.5) as i64 as f64;
                        if va <= 0.0 {
                            out_pixel.alpha = 0.0;
                            out_pixel.red = 0.0;
                            out_pixel.green = 0.0;
                            out_pixel.blue = 0.0;
                            return Ok(());
                        }
                        let a = va / level;
                        out_pixel.alpha = a as f32;

                        let unmat = |c: f64| (((level * c + 0.5) as i64 as f64) / level / a) as f32;
                        if gray_only {
                            let y = 0.299 * r + 0.587 * g + 0.114 * b;
                            let v = unmat(y);
                            out_pixel.red = v;
                            out_pixel.green = v;
                            out_pixel.blue = v;
                        } else {
                            out_pixel.red = unmat(r);
                            out_pixel.green = unmat(g);
                            out_pixel.blue = unmat(b);
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
