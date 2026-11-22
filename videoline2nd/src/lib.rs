//! F's VideoLine2nd の移植。
//!
//! 走査線状の縞を重ねて明るく (または暗く) する。VideoLine と違い縞の端は
//! なめらかにならず、Brightness が負なら暗くする。周期は height*2 + spacing
//! で、縞は前半 height 行ぶん。
//!
//! **32bit は元の FilterImage32 が空で何もしない**。ここでも同じにしてある。
//!
//! 向きのパラメータは元が**ポップアップをチェックボックスとして読んでいる**
//! (`GetCHECKBOX` が `u.bd.value` をそのまま返すので、1 なら TRUE、2 なら 2)。
//! そのあと `== TRUE` で比べるので、実質「1項目目かどうか」の判定になる。
//! ここでも同じく「値が 1 か」で分けている。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしている
//! (--inspect-experimental で確認)。英語名にしてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/VideoLine2nd

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_fp_long, round_fp_short, round_short_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Brightness,
    Height,
    Spacing,
    Direction,
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
        // 元: PF_ADD_FLOAT_SLIDER(明るさ, -100, 100, -100, 100, 1, -50, 1, 0, 0, ..)
        params.add(
            Params::Brightness,
            "Brightness(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-100.0);
                f.set_valid_max(100.0);
                f.set_slider_min(-100.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(-50.0);
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::Height,
            "Height(dot)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(100);
                f.set_slider_min(1);
                f.set_slider_max(16);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::Spacing,
            "Spacing(dot)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::Direction,
            "Direction",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizontal", "Vertical"]);
                f.set_default(1);
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
                out_data.set_return_msg("fs-rs videoline2nd");
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
        // 元の GetParams: /100 するだけ (符号はそのまま)。
        let bright = params.get(Params::Brightness)?.as_float_slider()?.value() as f64 / 100.0;
        let ds = in_data.downsample_x();
        let height = {
            let v = params.get(Params::Height)?.as_slider()?.value();
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i32
            }
        };
        let inter = {
            let v = params.get(Params::Spacing)?.as_slider()?.value();
            (v as i64 * ds.num as i64 / ds.den as i64) as i32
        };
        let vertical = params.get(Params::Direction)?.as_popup()?.value() == 1;

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の画素関数は明るさ 0 で即返る。
        if bright == 0.0 {
            return Ok(());
        }
        let depth = out_layer.bit_depth();
        // 元の FilterImage32 は空で何もしない。
        if depth == 32 {
            return Ok(());
        }
        let period = height * 2 + inter;
        if period <= 0 {
            return Ok(());
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        for y in 0..h {
            for x in 0..w {
                let base = if vertical { y as i32 } else { x as i32 };
                // 元は % のみ (負にはならない座標なので素直)。
                if base % period >= height {
                    continue;
                }
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        let m = MAX_CHANNEL8 as f64;
                        let f = |c: u8| -> u8 {
                            let c = c as f64;
                            if bright < 0.0 {
                                round_byte_fp_long((c + c * bright) as f32)
                            } else {
                                round_byte_fp_long((c + (m - c) * bright) as f32)
                            }
                        };
                        p.red = f(p.red);
                        p.green = f(p.green);
                        p.blue = f(p.blue);
                    }
                    _ => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        let m = MAX_CHANNEL16 as f64;
                        let f = |c: u16| -> u16 {
                            let c = c as f64;
                            if bright < 0.0 {
                                round_short_fp_long((c + c * bright) as f32)
                            } else {
                                round_short_fp_long((c + (m - c) * bright) as f32)
                            }
                        };
                        p.red = f(p.red);
                        p.green = f(p.green);
                        p.blue = f(p.blue);
                    }
                }
            }
        }
        Ok(())
    }
}
