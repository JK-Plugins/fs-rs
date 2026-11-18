//! F's ColorChangeFromPoint の移植。
//!
//! 指定した座標の画素の色を拾い、その色と同じ色の画素を Color に置き換える。
//! 拾った画素のアルファが 0 だったときは「置き換え」ではなく、画面全体を
//! Color の上に合成して不透明にする。座標が画面の外なら何もしない。
//!
//! 色の比較はどの深度でも 8bit に落としてから行う (元のまま)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ColorChangeFromPoint

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{
    conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32, round_byte_fp_double, round_fp_short,
    round_short_fp_double,
};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Point,
    Color,
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
        params.add(
            Params::Point,
            "TargetColorPoint",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::Color,
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
                out_data.set_return_msg("fs-rs colorchangefrompoint");
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
        // 元の GetParams: 座標は +0.5 して整数にする。
        let (px, py) = {
            let (x, y) = params.get(Params::Point)?.as_point()?.value();
            ((x as f64 + 0.5) as i64, (y as f64 + 0.5) as i64)
        };
        let dst8 = params.get(Params::Color)?.as_color()?.value();

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth {
            return Err(Error::BadCallbackParameter);
        }

        // 元の GetTargetColor: 座標が入力の外なら MODE_NONE で何もしない。
        if px < 0 || py < 0 || px >= in_w as i64 || py >= in_h as i64 {
            return Ok(());
        }
        let (sx, sy) = (px as usize, py as usize);
        // 元の GetPixelValue8 は深度を問わず 8bit に落として返す。
        let src8 = match depth {
            8 => *in_layer.as_pixel8(sx, sy),
            16 => conv_16_to_8(in_layer.as_pixel16(sx, sy)),
            _ => conv_32_to_8(in_layer.as_pixel32(sx, sy)),
        };

        let dst16 = conv_8_to_16(&dst8);
        let dst32 = conv_8_to_32(&dst8);

        // 拾った画素が透明なら「色の置き換え」ではなく、全体を Color の上に
        // 合成して不透明にする (元の MODE_ALPHA_ZERO)。
        let alpha_zero = src8.alpha == 0;

        for y in 0..h {
            for x in 0..w {
                if y >= in_h || x >= in_w {
                    continue;
                }
                match depth {
                    8 => {
                        let i = *in_layer.as_pixel8(x, y);
                        let p = out_layer.as_pixel8_mut(x, y);
                        if alpha_zero {
                            let op = i.alpha as f64 / MAX_CHANNEL8 as f64;
                            let mix = |d: u8, s: u8| {
                                round_byte_fp_double(d as f64 * (1.0 - op) + s as f64 * op + 0.5)
                            };
                            p.alpha = MAX_CHANNEL8 as u8;
                            p.red = mix(dst8.red, i.red);
                            p.green = mix(dst8.green, i.green);
                            p.blue = mix(dst8.blue, i.blue);
                        } else if i.red == src8.red && i.green == src8.green && i.blue == src8.blue
                        {
                            *p = dst8;
                        }
                    }
                    16 => {
                        let i = *in_layer.as_pixel16(x, y);
                        let c8 = conv_16_to_8(&i);
                        let p = out_layer.as_pixel16_mut(x, y);
                        if alpha_zero {
                            let op = i.alpha as f64 / MAX_CHANNEL16 as f64;
                            let mix = |d: u16, s: u16| {
                                round_short_fp_double(d as f64 * (1.0 - op) + s as f64 * op + 0.5)
                            };
                            p.alpha = MAX_CHANNEL16 as u16;
                            p.red = mix(dst16.red, i.red);
                            p.green = mix(dst16.green, i.green);
                            p.blue = mix(dst16.blue, i.blue);
                        } else if c8.red == src8.red
                            && c8.green == src8.green
                            && c8.blue == src8.blue
                        {
                            *p = dst16;
                        }
                    }
                    _ => {
                        let i = *in_layer.as_pixel32(x, y);
                        let c8 = conv_32_to_8(&i);
                        let p = out_layer.as_pixel32_mut(x, y);
                        if alpha_zero {
                            // 元の 32bit 版は +0.5 しない。
                            let op = i.alpha as f64;
                            let mix = |d: f32, s: f32| {
                                round_fp_short((d as f64 * (1.0 - op) + s as f64 * op) as f32)
                            };
                            p.alpha = 1.0;
                            p.red = mix(dst32.red, i.red);
                            p.green = mix(dst32.green, i.green);
                            p.blue = mix(dst32.blue, i.blue);
                        } else if c8.red == src8.red
                            && c8.green == src8.green
                            && c8.blue == src8.blue
                        {
                            *p = dst32;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
