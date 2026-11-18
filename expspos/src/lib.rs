//! F's ExpsPos の移植。
//!
//! 元はパラメータの実験用で、座標を10個並べているだけ。GetParams は読む
//! けれど描画には一切使わず、ON ならそのまま通し、OFF なら出力を消す。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ExpsPos

use after_effects::{self as ae};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    On,
    P0,
    P1,
    P2,
    P3,
    P4,
    P5,
    P6,
    P7,
    P8,
    P9,
}

const SLOTS: [Params; 10] = [
    Params::P0,
    Params::P1,
    Params::P2,
    Params::P3,
    Params::P4,
    Params::P5,
    Params::P6,
    Params::P7,
    Params::P8,
    Params::P9,
];

/// 元: PF_ADD_POINT("posN", N*10, N*10, 0, ..) で既定はレイヤ比の%。
const DEFAULTS: [(f32, f32); 10] = [
    (0.0, 0.0),
    (10.0, 10.0),
    (20.0, 20.0),
    (30.0, 30.0),
    (40.0, 40.0),
    (50.0, 50.0),
    (60.0, 60.0),
    (70.0, 70.0),
    (80.0, 80.0),
    (90.0, 90.0),
];

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
        // 元: PF_ADD_CHECKBOX(STR_ON, STR_ON2, TRUE, 0, ID_ON)
        params.add(
            Params::On,
            "ON",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;

        for i in 0..10 {
            let xy = DEFAULTS[i];
            params.add(
                SLOTS[i],
                &format!("pos{i}"),
                ae::PointDef::setup(|f| {
                    f.set_default(xy);
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
                out_data.set_return_msg("fs-rs expspos");
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
        let on = params.get(Params::On)?.as_checkbox()?.value();
        if on {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }
        // 元の out->clear(): 全チャンネルを 0 にする。
        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
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
        Ok(())
    }
}
