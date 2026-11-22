//! F's YuvControl の移植。
//!
//! RGB を YUV にして Y/U/V を個別に持ち上げ下げし、RGB に戻す。
//! 「UVをYと連動させる」が ON のときは U と V を -|Y| にする。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/YuvControl

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Y,
    UvAuto,
    U,
    V,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// Y / U / V の持ち上げ下げ。元の分岐をそのまま写している。
///
/// Y は 0..1 なので上限が 1、U と V は ±0.5 の範囲なので符号ごとに上限が違う。
fn adjust_y(y: f64, amount: f64) -> f64 {
    if amount > 0.0 {
        y + (1.0 - y) * amount
    } else {
        y * (1.0 + amount)
    }
}

fn adjust_uv(c: f64, amount: f64) -> f64 {
    if amount > 0.0 {
        if c >= 0.0 {
            c + (0.5 - c) * amount
        } else {
            c + (-0.5 - c) * amount
        }
    } else {
        c * (1.0 + amount)
    }
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
        // 元の並びは Y, UVをYと連動させる, U, V。
        // 元: PF_ADD_FLOAT_SLIDER(STR_Y, -100, 100, -100, 100, 1, 0, 1, 0, 0, ID_Y)
        for (p, name) in [(Params::Y, "Y"), (Params::U, "U"), (Params::V, "V")] {
            if p == Params::U {
                // 元は Y の直後にチェックボックスが来る。
                params.add(
                    Params::UvAuto,
                    "link UV to Y",
                    ae::CheckBoxDef::setup(|f| {
                        // 元のラベルは "UVをYと連動させる"。PF のパラメータ名は
                        // ASCII でないと AE 側で空文字になるので英語にしている。
                        f.set_label("on");
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
            }
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(-100.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(-100.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(0.0);
                    f.set_value(f.default());
                }),
            )?;
        }

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
        out_data.set_return_msg("fs-rs yuvcontrol");
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
        // 元の GetParams: どれも /100 される。連動が ON なら U=V=-|Y|。
        let amt_y = params.get(Params::Y)?.as_float_slider()?.value() / 100.0;
        let (amt_u, amt_v) = if params.get(Params::UvAuto)?.as_checkbox()?.value() {
            (-amt_y.abs(), -amt_y.abs())
        } else {
            (
                params.get(Params::U)?.as_float_slider()?.value() / 100.0,
                params.get(Params::V)?.as_float_slider()?.value() / 100.0,
            )
        };

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: 3つとも 0 ならコピーだけで返る。
        if amt_y == 0.0 && amt_u == 0.0 && amt_v == 0.0 {
            return Ok(());
        }

        // マット済みの 0..1 RGB を受け取り、調整後の 0..1 RGB を返す。
        let convert = move |r: f64, g: f64, b: f64| -> (f64, f64, f64) {
            let y = r * 0.299 + g * 0.587 + b * 0.114;
            let u = r * -0.169 - g * 0.331 + b * 0.500;
            // 元は - (b * -0.081) と二重否定していて、BT.601 の -0.081b にならない。
            // そのせいで Y だけ動かしてもグレーに色が付く (実機で確認)。
            // 明らかなバグなので標準の係数に直した。
            let v = r * 0.500 - g * 0.419 - b * 0.081;

            let y = adjust_y(y, amt_y);
            let u = adjust_uv(u, amt_u);
            let v = adjust_uv(v, amt_v);

            (y + 1.402 * v, y - 0.344 * u - 0.714 * v, y + 1.772 * u)
        };

        let progress_final = out_layer.height() as _;
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
                        let m = MAX_CHANNEL8 as f64;
                        if out_pixel.alpha == 0 {
                            *out_pixel = ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            };
                            return Ok(());
                        }
                        let a = out_pixel.alpha as f64 / m;
                        let (r, g, b) = convert(
                            out_pixel.red as f64 * a / m,
                            out_pixel.green as f64 * a / m,
                            out_pixel.blue as f64 * a / m,
                        );
                        // 元はアンマットしてから +0.5 で丸め、0..255 にクランプする。
                        let f = |c: f64| -> u8 {
                            let v = (m * c / a + 0.5) as i64;
                            v.clamp(0, MAX_CHANNEL8 as i64) as u8
                        };
                        out_pixel.red = f(r);
                        out_pixel.green = f(g);
                        out_pixel.blue = f(b);
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f64;
                        if out_pixel.alpha == 0 {
                            *out_pixel = ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            };
                            return Ok(());
                        }
                        let a = out_pixel.alpha as f64 / m;
                        let (r, g, b) = convert(
                            out_pixel.red as f64 * a / m,
                            out_pixel.green as f64 * a / m,
                            out_pixel.blue as f64 * a / m,
                        );
                        let f = |c: f64| -> u16 {
                            let v = (m * c / a + 0.5) as i64;
                            v.clamp(0, MAX_CHANNEL16 as i64) as u16
                        };
                        out_pixel.red = f(r);
                        out_pixel.green = f(g);
                        out_pixel.blue = f(b);
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        if out_pixel.alpha == 0.0 {
                            out_pixel.red = 0.0;
                            out_pixel.green = 0.0;
                            out_pixel.blue = 0.0;
                            return Ok(());
                        }
                        // 32bit は 0..1 のままで、マットはアルファを掛けるだけ。
                        let a = out_pixel.alpha as f64;
                        let (r, g, b) = convert(
                            out_pixel.red as f64 * a,
                            out_pixel.green as f64 * a,
                            out_pixel.blue as f64 * a,
                        );
                        out_pixel.red = (r / a) as f32;
                        out_pixel.green = (g / a) as f32;
                        out_pixel.blue = (b / a) as f32;
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
