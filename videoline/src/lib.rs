//! F's VideoLine の移植。
//!
//! 走査線状の縞を重ねて明るく (または暗く) する。Brightness が 100% より
//! 小さいと暗くする側に回る (元は 100% を基準に符号を分ける)。縞の端は
//! 固定小数の小数部でなめらかに減衰する。Offset は角度で、縞1周期ぶんを
//! 1回転として位置をずらす。
//!
//! 向きのパラメータは元が**ポップアップをチェックボックスとして読んでいる**
//! (`GetCHECKBOX` が `u.bd.value` をそのまま返すので、1 なら TRUE、2 なら 2)。
//! そのあと `== TRUE` で比べるので、実質「1項目目かどうか」の判定になる。
//! ここでも同じく「値が 1 か」で分けている。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしている
//! (--inspect-experimental で確認)。英語名にしてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/VideoLine

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_fp_long, round_fp_short, round_short_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Brightness,
    Height,
    LinePos,
    Spacing,
    Direction,
    Offset,
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
        // 元: PF_ADD_FIXED(明るさ, 0, 200, 0, 200, 75, 1, 0, 0, ID_BRIGHT)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        params.add(
            Params::Brightness,
            "Brightness(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(75.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_SLIDER(高さ, 1, 100, 1, 16, 2, ID_HEIGHT)
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

        // 元: PF_ADD_CHECKBOX(ラインの位置, 反転する, FALSE, 0, ID_REV)
        params.add(
            Params::LinePos,
            "LinePos",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("invert");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_SLIDER(間隔補正, -100, 100, 0, 10, 0, ID_INTER)
        params.add(
            Params::Spacing,
            "Spacing(dot)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-100);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_POPUP(方向, 2, 1, "水平|垂直", ID_DIR)
        params.add(
            Params::Direction,
            "Direction",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizontal", "Vertical"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_ANGLE(オフセット, 0, ID_OFFSET)
        params.add(
            Params::Offset,
            "Offset",
            ae::AngleDef::setup(|f| {
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
                out_data.set_return_msg("fs-rs videoline");
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
        // 元の GetParams: PF_Fixed のまま 100% を引いて 100 で割る (整数除算)。
        let raw = params.get(Params::Brightness)?.as_float_slider()?.value();
        let fixed = (raw * 65536.0) as i32;
        let f = (fixed - (100 << 16)) / 100;
        let minus = f < 0;
        let bright = f.abs() as f32 / 65536.0;

        let ds = in_data.downsample_x();
        // 元の downScale は +0.5 で丸める。
        let height = {
            let v = params.get(Params::Height)?.as_slider()?.value();
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i32
            }
        };
        let rev = params.get(Params::LinePos)?.as_checkbox()?.value();
        // 元の downScaleNoClip は整数演算で丸めない。そのあと height を足し、
        // 0 以下なら 1 にする。
        let inter = {
            let v = params.get(Params::Spacing)?.as_slider()?.value();
            let mut i = (v as i64 * ds.num as i64 / ds.den as i64) as i32 + height;
            if i <= 0 {
                i = 1;
            }
            i
        };
        // 元はポップアップを GetCHECKBOX で読んで == TRUE と比べる (上の説明)。
        let vertical = params.get(Params::Direction)?.as_popup()?.value() == 1;
        // 元: 角度を % 360 して 0..1 に直し、負なら 1 を足して1周期ぶんに掛ける。
        let offset = {
            let deg = params.get(Params::Offset)?.as_angle()?.value();
            let fixed = (deg * 65536.0) as i32 % (360 << 16);
            let mut o = fixed as f32 / 65536.0 / 360.0;
            if o < 0.0 {
                o += 1.0;
            }
            o * (height + inter) as f32
        };

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: 明るさが 0 なら何もしない。
        if bright == 0.0 {
            return Ok(());
        }

        let w_period = height + inter;
        if w_period <= 0 {
            return Ok(());
        }
        let off_fixed = (offset * 65536.0) as i32;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        for y in 0..h {
            for x in 0..w {
                // 元の固定小数演算。負の値の % と >> もそのまま (C と同じ挙動)。
                let base = if vertical { y as i32 } else { x as i32 };
                let fy = (base << 16) - off_fixed;
                let yv = fy % (w_period << 16);
                let ya = yv >> 16;
                let yd = (yv & 0xFFFF) as f32 / 65536.0;
                let brt = if !rev {
                    if ya == 0 {
                        bright * yd
                    } else if ya < height {
                        bright
                    } else if ya == height {
                        bright * (1.0 - yd)
                    } else {
                        0.0
                    }
                } else if ya == 0 {
                    bright * (1.0 - yd)
                } else if ya == height {
                    bright * yd
                } else if ya > height {
                    bright
                } else {
                    0.0
                };
                if brt <= 0.0 {
                    continue;
                }
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        let m = MAX_CHANNEL8 as f32;
                        let f = |c: u8| -> u8 {
                            if minus {
                                round_byte_fp_long(c as f32 - c as f32 * brt)
                            } else {
                                round_byte_fp_long(c as f32 + (m - c as f32) * brt)
                            }
                        };
                        p.red = f(p.red);
                        p.green = f(p.green);
                        p.blue = f(p.blue);
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        let m = MAX_CHANNEL16 as f32;
                        let f = |c: u16| -> u16 {
                            if minus {
                                round_short_fp_long(c as f32 - c as f32 * brt)
                            } else {
                                round_short_fp_long(c as f32 + (m - c as f32) * brt)
                            }
                        };
                        p.red = f(p.red);
                        p.green = f(p.green);
                        p.blue = f(p.blue);
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        let f = |c: f32| -> f32 {
                            if minus {
                                round_fp_short(c - c * brt)
                            } else {
                                round_fp_short(c + (1.0 - c) * brt)
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
