//! F's alphaThreshold の移植。
//!
//! アルファをしきい値で 0 か最大値に振り分けるだけのエフェクト。元実装
//! (Legacy/Project/AlphaThreshold/alphaThreshold.cpp) と同じく、パラメータは
//! パーセント1個で、深度ごとの最大値に換算して比較する。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/AlphaThreshold

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_fp_long, round_short_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Threshold,
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_A, 0, 100, 0, 100, 1, 50, 1, 0, 0, ID_A)
        // パラメータ名も元の "AlphaThreshold" のまま。
        params.add(
            Params::Threshold,
            "AlphaThreshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_default(50.0);
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
        out_data.set_return_msg("fs-rs alphathreshold");
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
        // 元: infoP->a は GetParams で /100 される
        let threshold = params.get(Params::Threshold)?.as_float_slider()?.value() as f32 / 100.0;

        // 元は深度ごとの最大値に掛けてから比較する
        // (8bit は RoundByteFpLong、16bit は RoundShortFpLong)。
        let v8 = round_byte_fp_long(MAX_CHANNEL8 as f32 * threshold);
        let v16 = round_short_fp_long(MAX_CHANNEL16 as f32 * threshold);

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
                        out_pixel.alpha = if out_pixel.alpha >= v8 {
                            MAX_CHANNEL8 as u8
                        } else {
                            0
                        };
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        out_pixel.alpha = if out_pixel.alpha >= v16 {
                            MAX_CHANNEL16 as u16
                        } else {
                            0
                        };
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        // 32bit だけ最初の分岐が素通しで、1.0 を超えるアルファは
                        // そのまま残る (元の FilterImage32 がそう書かれている)。
                        // 1.0 以下では 8/16bit と同じしきい値処理になる。
                        if out_pixel.alpha >= 1.0 {
                            // 元実装どおり何もしない
                        } else if out_pixel.alpha >= threshold {
                            out_pixel.alpha = 1.0;
                        } else {
                            out_pixel.alpha = 0.0;
                        }
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
