//! F's Premultiply の移植。
//!
//! マット合成とストレートを相互変換する。元実装は自前で画素を触らず、ホストの
//! PF_PremultiplyColor (FillMatte スイート) に丸ごと委譲しているだけなので、
//! ここも同じようにスイートを呼ぶ。
//!
//! 元の CFsAE::premultiply_color は mode をそのまま forward 引数に渡している。
//! mode 0 が「マットからストレート」(forward = false)、1 が「ストレートからマット」。
//! 元は深度によらず 8bit 版の premultiply_color だけを呼ぶので、ここも同じにした。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Premultiply

use after_effects::{self as ae};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mode,
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
        // 元: PF_ADD_POPUP(STR_POP1, 2, 1, "マットからストレート|ストレートからマット", ID_MODE)
        // 既定は 1 (1始まり) なので「マットからストレート」。
        params.add(
            Params::Mode,
            "mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Matte to Straight", "Straight to Matte"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_COLOR, 0x00, 0x00, 0x00, ID_COLOR)
        // 元のラベルは "マットカラー"。PF のパラメータ名は ASCII でないと AE 側で
        // 空文字になるので英語にしている (元も PiPL 経由で文字化けしている)。
        params.add(
            Params::Color,
            "Matte Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 255,
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
        out_data.set_return_msg("fs-rs premultiply");
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
        // 元の GetParams: ポップアップは 1 始まりなので 1 引いて 0..1 に丸める。
        let mode = (params.get(Params::Mode)?.as_popup()?.value() - 1).clamp(0, 1);
        let color = params.get(Params::Color)?.as_color()?.value();

        // 元は CopyInToOut せず、premultiply_color が input から output へ書く。
        ae::pf::suites::FillMatte::new()?.premultiply_color(
            in_data.effect_ref(),
            &in_layer,
            &color,
            mode != 0,
            &mut out_layer,
        )?;

        Ok(())
    }
}
