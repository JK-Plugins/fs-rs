//! F's PaperPlaneGetWeight の移植。
//!
//! 紙飛行機の「重さ」を RGB に書き込むためのもので、絵を作るのではなく
//! データを埋めるエフェクト。アルファのある画素を不透明にして、
//! 選んだ重さ (180/225/310/360 を 5 で割った値) と枚数から計算した値を
//! R と G に半分ずつ、枚数を B に入れる。
//!
//! 枚数が 0 以下なら出力を消す。
//!
//! **8bit だけが実装されている**。元は 16/32bit の呼び出しがコメントアウト
//! されていて入力をコピーするだけなので、ここでも同じにしてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/PaperPlaneGetWeight

use after_effects::{self as ae, MAX_CHANNEL8};

/// 元の WEIGHT_SCALE と pw*。
const WEIGHT_SCALE: i32 = 5;
const WEIGHTS: [i32; 4] = [
    180 / WEIGHT_SCALE,
    225 / WEIGHT_SCALE,
    310 / WEIGHT_SCALE,
    360 / WEIGHT_SCALE,
];

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    HalfSize,
    Count,
    Weight,
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
            Params::HalfSize,
            "HalfSize",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 出荷 .aex の範囲に合わせてある (0..5、既定 1)。
        params.add(
            Params::Count,
            "Count",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(5);
                f.set_slider_min(0);
                f.set_slider_max(5);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::Weight,
            "Weight",
            ae::PopupDef::setup(|f| {
                f.set_options(&["180", "225", "310", "360"]);
                f.set_default(2);
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
                out_data.set_return_msg("fs-rs paperplanegetweight");
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
        let is_half = params.get(Params::HalfSize)?.as_checkbox()?.value();
        let count = params.get(Params::Count)?.as_slider()?.value();
        let popup = params.get(Params::Weight)?.as_popup()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: 枚数が 0 以下なら出力を消して終わり。
        if count <= 0 {
            for y in 0..h {
                for x in 0..w {
                    match depth {
                        8 => {
                            *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        }
                        16 => {
                            *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        }
                        _ => {
                            *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                                alpha: 0.0,
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                            }
                        }
                    }
                }
            }
            return Ok(());
        }

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元は 16/32bit の呼び出しがコメントアウトされていてコピーだけ。
        if depth != 8 {
            return Ok(());
        }

        // 元の GetParams。
        let mut weight = WEIGHTS
            .get((popup - 1).max(0) as usize)
            .copied()
            .unwrap_or(0);
        let mut c = count;
        if is_half {
            weight *= c * 2;
            // 元: 下位4bitに枚数、上位に枚数*2 を詰める。
            c |= (c * 2) << 4;
        } else {
            weight *= c;
            c |= c << 4;
        }
        let weight_b = c as u8;
        if weight > 255 + 255 {
            weight = 255 + 255;
        }
        let weight_g = (weight / 2) as u8;
        // 元: R は残り (奇数なら R の方が 1 大きい)。
        let weight_r = (weight - weight / 2) as u8;

        for y in 0..h {
            for x in 0..w {
                let p = out_layer.as_pixel8_mut(x, y);
                let a = p.alpha;
                p.alpha = MAX_CHANNEL8 as u8;
                p.red = 0;
                p.green = 0;
                p.blue = 0;
                if a == 0 || weight <= 0 {
                    continue;
                }
                // 元: +0.5 して丸め、RoundByteLong でクランプ。
                let f = |v: u8| -> u8 {
                    ((v as f64 * a as f64 / MAX_CHANNEL8 as f64 + 0.5) as i64)
                        .clamp(0, MAX_CHANNEL8 as i64) as u8
                };
                p.red = f(weight_r);
                p.green = f(weight_g);
                p.blue = weight_b;
            }
        }
        Ok(())
    }
}
