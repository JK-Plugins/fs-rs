//! F's Scanline の移植。
//!
//! 走査線状に太さ height の帯を交互に並べ、奇数列と偶数列で別々の
//! 明るさと不透明度をかける。明るさが正なら白に寄せ、負なら黒に寄せる。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしている
//! ので英語名にしてある。ポップアップの項目名も同じ。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Scanline

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

/// 深度をまたいで1画素を扱う形。値は深度そのままの目盛り。
#[derive(Clone, Copy)]
struct Px {
    alpha: f64,
    red: f64,
    green: f64,
    blue: f64,
}

/// 元の PX8 / PX16 / PX32。`max` はその深度の最大値 (32bit は 1.0)。
/// `round` が true なら +0.5 して丸める (元は 8/16bit だけ丸める)。
fn shade(c: Px, lv: f64, opa: f64, max: f64, round: bool) -> Px {
    if opa <= 0.0 {
        // 元: 不透明度が 0 以下なら全チャンネル 0 を返す。
        return Px {
            alpha: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        };
    }
    let half = if round { 0.5 } else { 0.0 };
    let alpha = if opa >= 1.0 {
        c.alpha
    } else {
        (c.alpha * opa + half).clamp(0.0, max)
    };
    // 元は明るさが 0 のとき `ret = c` と画素ごと上書きしてしまい、
    // 上で求めた不透明度を捨てていた (明るさ 0 では不透明度が
    // 効かない)。RGB だけ元の値にする。
    if lv == 0.0 {
        return Px {
            alpha,
            red: c.red,
            green: c.green,
            blue: c.blue,
        };
    }
    let f = |v: f64| -> f64 {
        let n = if lv > 0.0 {
            v + (max - v) * lv
        } else {
            v + v * lv
        };
        (n + half).clamp(0.0, max)
    };
    Px {
        alpha,
        red: f(c.red),
        green: f(c.green),
        blue: f(c.blue),
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Height,
    Level0,
    Opacity0,
    Level1,
    Opacity1,
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
        // 元: PF_ADD_SLIDER(ラインの太さ, 1, 20, 2, 10, 3, ID_HEIGHT)
        params.add(
            Params::Height,
            "Thickness(pixel)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(20);
                f.set_slider_min(2);
                f.set_slider_max(10);
                f.set_default(3);
                f.set_value(f.default());
            }),
        )?;

        // 元: 奇数列 / 偶数列それぞれの明るさ (-100..100) と不透明度 (0..100)。
        for (p, name, vmin, dflt) in [
            (Params::Level0, "OddBrightness(%)", -100.0, -50.0),
            (Params::Opacity0, "OddOpacity(%)", 0.0, 50.0),
            (Params::Level1, "EvenBrightness(%)", -100.0, 0.0),
            (Params::Opacity1, "EvenOpacity(%)", 0.0, 100.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(vmin);
                    f.set_valid_max(100.0);
                    f.set_slider_min(vmin);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }

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
                out_data.set_return_msg("fs-rs scanline");
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
        let height = params.get(Params::Height)?.as_slider()?.value().max(1);
        // 元の GetParams: 明るさ・不透明度はどれも /100。
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() as f64 / 100.0)
        };
        let lv0 = pct(Params::Level0, params)?;
        let op0 = pct(Params::Opacity0, params)?;
        let lv1 = pct(Params::Level1, params)?;
        let op1 = pct(Params::Opacity1, params)?;
        // 元: dir == 1 なら行で分ける (水平の縞)。
        let horizontal = params.get(Params::Direction)?.as_popup()?.value() == 1;

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        let (max, round) = match depth {
            8 => (MAX_CHANNEL8 as f64, true),
            16 => (MAX_CHANNEL16 as f64, true),
            // 元の PX32 は +0.5 しない。
            _ => (1.0, false),
        };

        for y in 0..h {
            for x in 0..w {
                let band = if horizontal {
                    y as i32 / height
                } else {
                    x as i32 / height
                };
                let (lv, opa) = if band % 2 == 0 {
                    (lv0, op0)
                } else {
                    (lv1, op1)
                };
                // 元は「明るさが 0 でなく、または不透明度が 1 でない」ときだけ
                // 画素に手を入れる。
                if lv == 0.0 && opa == 1.0 {
                    continue;
                }
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        let c = Px {
                            alpha: p.alpha as f64,
                            red: p.red as f64,
                            green: p.green as f64,
                            blue: p.blue as f64,
                        };
                        let r = shade(c, lv, opa, max, round);
                        p.alpha = r.alpha as u8;
                        p.red = r.red as u8;
                        p.green = r.green as u8;
                        p.blue = r.blue as u8;
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        let c = Px {
                            alpha: p.alpha as f64,
                            red: p.red as f64,
                            green: p.green as f64,
                            blue: p.blue as f64,
                        };
                        let r = shade(c, lv, opa, max, round);
                        p.alpha = r.alpha as u16;
                        p.red = r.red as u16;
                        p.green = r.green as u16;
                        p.blue = r.blue as u16;
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        let c = Px {
                            alpha: p.alpha as f64,
                            red: p.red as f64,
                            green: p.green as f64,
                            blue: p.blue as f64,
                        };
                        let r = shade(c, lv, opa, max, round);
                        p.alpha = r.alpha as f32;
                        p.red = r.red as f32;
                        p.green = r.green as f32;
                        p.blue = r.blue as f32;
                    }
                }
            }
        }
        Ok(())
    }
}
