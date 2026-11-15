//! F's whiteInOut の移植。
//!
//! 白に向かって飛ばす / 白から戻すエフェクト。パラメータ 0 で素通し、
//! 100 で純白になる。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/whiteInOut

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

/// 元の OFFSET_VALUE。
const OFFSET_VALUE: f64 = 0.2;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_Y, 0, 100, 0, 100, 1, 0, 1, 0, 0, ID_Y)
        // 元のパラメータ名は "value"。
        params.add(
            Params::Value,
            "value",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
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
        out_data.set_return_msg("fs-rs whiteinout");
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
        // 元の GetParams: /100 してクランプしたあと 1.0 から引く。
        // つまりパラメータ 0 -> value 1.0 (素通し)、100 -> value 0.0 (純白)。
        let raw = params.get(Params::Value)?.as_float_slider()?.value() / 100.0;
        let value = 1.0 - raw.clamp(0.0, 1.0);

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の画素関数の先頭: value が 1.0 以上なら何もしない。
        if value >= 1.0 {
            return Ok(());
        }

        // 0..1 の1チャンネル分。value が 0 のときは分岐に入らず最大値のままに
        // なるので、呼び出し側で白を入れる。
        let push = move |c: f64| -> f64 {
            let a = (1.0 - c) * OFFSET_VALUE * (1.0 - value);
            (c + a) / value
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
                        let m = MAX_CHANNEL8 as f64;
                        // value <= 0 のときは元のコードが初期値のまま返すので純白。
                        let (r, g, b) = if value > 0.0 {
                            let f = |c: u8| -> u8 {
                                // 元は +0.5 で丸めて上限だけクランプする
                                // (下限は c >= 0 なので起きない)。
                                let v = (push(c as f64 / m) * m + 0.5) as i64;
                                v.min(MAX_CHANNEL8 as i64) as u8
                            };
                            (f(out_pixel.red), f(out_pixel.green), f(out_pixel.blue))
                        } else {
                            (MAX_CHANNEL8 as u8, MAX_CHANNEL8 as u8, MAX_CHANNEL8 as u8)
                        };
                        out_pixel.red = r;
                        out_pixel.green = g;
                        out_pixel.blue = b;
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f64;
                        let (r, g, b) = if value > 0.0 {
                            let f = |c: u16| -> u16 {
                                let v = (push(c as f64 / m) * m + 0.5) as i64;
                                v.min(MAX_CHANNEL16 as i64) as u16
                            };
                            (f(out_pixel.red), f(out_pixel.green), f(out_pixel.blue))
                        } else {
                            (MAX_CHANNEL16 as u16, MAX_CHANNEL16 as u16, MAX_CHANNEL16 as u16)
                        };
                        out_pixel.red = r;
                        out_pixel.green = g;
                        out_pixel.blue = b;
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        // 32bit は 0..1 のままなので割らずに渡す。
                        let (r, g, b) = if value > 0.0 {
                            let f = |c: f32| -> f32 { push(c as f64).min(1.0) as f32 };
                            (f(out_pixel.red), f(out_pixel.green), f(out_pixel.blue))
                        } else {
                            (1.0, 1.0, 1.0)
                        };
                        out_pixel.red = r;
                        out_pixel.green = g;
                        out_pixel.blue = b;
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
