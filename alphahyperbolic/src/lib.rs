//! F's AlphaHyperbolic の移植。
//!
//! アルファに双曲線カーブ v = ((1+k)t) / (1+kt) をかける。パラメータが負のときは
//! 符号を反転して invert を立てる、という元実装の作りもそのまま。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/AlphaHyperbolic

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Hyperbolic,
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_HYPERBOLIC, -10, 50, -2, 10, 1, 5, 1, 0, 0, ID_HYPERBOLIC)
        // 入力は -10..50 でスライダーは -2..10。負の値は GetParams で符号を反転して
        // invert を立てる扱いになる。
        params.add(
            Params::Hyperbolic,
            "hyperbolic",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-10.0);
                f.set_valid_max(50.0);
                f.set_slider_min(-2.0);
                f.set_slider_max(10.0);
                f.set_precision(1);
                f.set_default(5.0);
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
        out_data.set_return_msg("fs-rs alphahyperbolic");
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
        // 元の GetParams: 負なら符号を反転して invert を立てる。
        let raw = params.get(Params::Hyperbolic)?.as_float_slider()?.value() as f32;
        let invert = raw < 0.0;
        let k = if invert { -raw } else { raw };

        // 元の Exec: hyperbolic が 0 なら何もしない。
        if k == 0.0 {
            return ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            );
        }

        // アルファ 0..1 に双曲線カーブをかける。invert が立っていないときは
        // 前後で 1-t を取るので、カーブの向きが反転する。
        let curve = move |t: f32| -> f32 {
            let mut t = t.clamp(0.0, 1.0);
            if !invert {
                t = 1.0 - t;
            }
            let mut v = ((1.0 + k) * t) / (1.0 + k * t);
            v = v.clamp(0.0, 1.0);
            if !invert {
                v = 1.0 - v;
            }
            v
        };

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
                        // 完全透明と完全不透明はカーブをかけずに素通し (元と同じ)。
                        if out_pixel.alpha == 0 || out_pixel.alpha as u32 >= MAX_CHANNEL8 {
                            return Ok(());
                        }
                        let v = curve(out_pixel.alpha as f32 / MAX_CHANNEL8 as f32);
                        // 元は丸めずにキャストするので切り捨て。
                        out_pixel.alpha = (v * MAX_CHANNEL8 as f32) as u8;
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        if out_pixel.alpha == 0 || out_pixel.alpha as u32 >= MAX_CHANNEL16 {
                            return Ok(());
                        }
                        let v = curve(out_pixel.alpha as f32 / MAX_CHANNEL16 as f32);
                        out_pixel.alpha = (v * MAX_CHANNEL16 as f32) as u16;
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        if out_pixel.alpha <= 0.0 || out_pixel.alpha >= 1.0 {
                            return Ok(());
                        }
                        out_pixel.alpha = curve(out_pixel.alpha);
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
