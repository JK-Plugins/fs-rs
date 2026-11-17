//! F's ChannelBlur の移植。
//!
//! **元が未完成**で、パラメータは全部そろっているのに `Exec` の中身が
//! 空っぽ (8/16/32bit のどの枝も `break;` だけ) という骨組みのまま
//! 止まっている。やることは入力をそのままコピーするだけ。出荷された
//! .aex が無いのもそのせいだと思われる。
//!
//! ```c
//! ERR(ae->CopyInToOut());
//! switch(ae->pixelFormat())
//! {
//! case PF_PixelFormat_ARGB128:
//!     break;
//! case PF_PixelFormat_ARGB64:
//!     break;
//! case PF_PixelFormat_ARGB32:
//!     break;
//! }
//! ```
//!
//! 名前とパラメータ (`Mode` が RGB / HLS / Yuv、チャンネルごとのぼかし量が
//! 4つ) から、**チャンネルごとに別の量でぼかす**つもりだったのは読める。
//! ただし元に一行も書かれていないので、ぼかしの種類 (箱かガウスか)、
//! HLS / Yuv への行き帰りの作法、アルファの扱いを決める手がかりが無い。
//! 突き合わせる出荷バイナリも無いため、ここでは**元のとおり素通し**に
//! してある。中身を入れるかどうかは移植ではなく新しく作る話なので、
//! 必要なら別に決める。
//!
//! パラメータの構成だけは元のまま並べてあるので、既存の .aep に
//! 置いてあっても値は読み込まれる (もっとも元も何もしない)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ChannelBlur

use after_effects::{self as ae};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
    Ch0,
    Ch1,
    Ch2,
    Ch3,
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
        // 元: PF_ADD_POPUP("Mode", 3, 1, "RGB|HLS|Yuv")
        params.add(
            Params::Mode,
            "Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["RGB", "HLS", "Yuv"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(各チャンネル, 0, 500, 0, 100, 0)
        for (p, name) in [
            (Params::Ch0, "R_H_Y_blur"),
            (Params::Ch1, "G_L_u_blur"),
            (Params::Ch2, "B_S_v_blur"),
            (Params::Ch3, "Alpha_blur"),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(0);
                    f.set_valid_max(500);
                    f.set_slider_min(0);
                    f.set_slider_max(100);
                    f.set_default(0);
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
                out_data.set_return_msg("fs-rs channelblur");
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
        _params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        // 元の Exec は入力をコピーするだけ (中身が書かれていない)。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        Ok(())
    }
}
