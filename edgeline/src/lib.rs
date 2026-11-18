//! F's EdgeLine の移植。
//!
//! TargetColor に (level の許容差で) 一致した画素について、その周り
//! length px の窓を入力の中で探し、SampleColor に一致する画素が1つでも
//! あれば DrawColor で塗る。2色の境界に線を引く用途。
//!
//! 窓の外は元の getPixel8 と同じく**透明な黒 (0,0,0,0) として読む**
//! (クランプではない)。色の比較はどの深度でも 8bit に落としてから行うので、
//! level は 8bit 単位。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/EdgeLine

use after_effects::{self as ae, MAX_CHANNEL8};

use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TargetColor,
    SampleColor,
    Level,
    Length,
    DrawColor,
}

/// 元の compPix*Lv。RGB の差がすべて lv 以内かどうか (アルファは見ない)。
fn close(p: &ae::Pixel8, c: &ae::Pixel8, lv: i32) -> bool {
    (p.blue as i32 - c.blue as i32).abs() <= lv
        && (p.green as i32 - c.green as i32).abs() <= lv
        && (p.red as i32 - c.red as i32).abs() <= lv
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
        // 元: PF_ADD_COLOR(STR_TARGET, 0xFF, 0x00, 0x00, ..)
        for (p, name, rgb) in [
            (Params::TargetColor, "TargetColor", [0xFFu8, 0x00, 0x00]),
            (Params::SampleColor, "SampleColor", [0x00, 0xFF, 0x00]),
        ] {
            params.add(
                p,
                name,
                ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: rgb[0],
                        green: rgb[1],
                        blue: rgb[2],
                    });
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_FLOAT_SLIDER(STR_LEVEL, 0, 100, 0, 10, 1, 0, 1, 0, 0, ..)
        params.add(
            Params::Level,
            "level(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(10.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_SLIDER(STR_LENGTH, 0, 200, 0, 20, 10, ..)
        params.add(
            Params::Length,
            "length(px)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(200);
                f.set_slider_min(0);
                f.set_slider_max(20);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_DRAW, 0x00, 0x00, 0xFF, ..)
        params.add(
            Params::DrawColor,
            "DrawColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0x00,
                    green: 0x00,
                    blue: 0xFF,
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
                out_data.set_return_msg("fs-rs edgeline");
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
        let target = params.get(Params::TargetColor)?.as_color()?.value();
        let sample = params.get(Params::SampleColor)?.as_color()?.value();
        // 元: level は 255 倍して A_u_char にキャスト (切り捨て)。
        let level = (MAX_CHANNEL8 as f64
            * params.get(Params::Level)?.as_float_slider()?.value() as f64
            / 100.0) as i32;
        let draw8 = params.get(Params::DrawColor)?.as_color()?.value();

        // 元: length > 0 なら downScale をかける (+0.5 で丸め)。
        let mut length = params.get(Params::Length)?.as_slider()?.value();
        if length > 0 {
            let ds = in_data.downsample_x();
            length = (length as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i32;
        }

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: length が 0 以下なら素通し。
        if length <= 0 {
            return Ok(());
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width() as i32, in_layer.height() as i32);
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth {
            return Err(Error::BadCallbackParameter);
        }
        let draw16 = conv_8_to_16(&draw8);
        let draw32 = conv_8_to_32(&draw8);

        // 元の getPixel8: 範囲外は透明な黒を返す。
        let at = |x: i32, y: i32| -> ae::Pixel8 {
            if x < 0 || y < 0 || x >= in_w || y >= in_h {
                return ae::Pixel8 {
                    alpha: 0,
                    red: 0,
                    green: 0,
                    blue: 0,
                };
            }
            let (x, y) = (x as usize, y as usize);
            match depth {
                8 => *in_layer.as_pixel8(x, y),
                16 => conv_16_to_8(in_layer.as_pixel16(x, y)),
                _ => conv_32_to_8(in_layer.as_pixel32(x, y)),
            }
        };

        for yl in 0..h as i32 {
            for xl in 0..w as i32 {
                // 元は入力画素が TargetColor に近くなければ何もしない。
                if !close(&at(xl, yl), &target, level) {
                    continue;
                }
                // 元は y0 が負のとき `x0 = 0` と書いていた (y0 のつもりで x0 を
                // 書いた typo)。y0 が負のまま残るので上辺では画像の外を読み、
                // さらに窓が左端まで広がっていた。y0 を 0 に直した。
                let mut x0 = xl - length;
                if x0 < 0 {
                    x0 = 0;
                }
                let mut x1 = xl + length;
                if x1 >= in_w {
                    x1 = in_w - 1;
                }
                let mut y0 = yl - length;
                if y0 < 0 {
                    y0 = 0;
                }
                let mut y1 = yl + length;
                if y1 >= in_h {
                    y1 = in_h - 1;
                }
                let mut hit = false;
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        if close(&at(x, y), &sample, level) {
                            hit = true;
                            break;
                        }
                    }
                }
                if !hit {
                    continue;
                }
                let (x, y) = (xl as usize, yl as usize);
                match depth {
                    8 => *out_layer.as_pixel8_mut(x, y) = draw8,
                    16 => *out_layer.as_pixel16_mut(x, y) = draw16,
                    _ => *out_layer.as_pixel32_mut(x, y) = draw32,
                }
            }
        }
        Ok(())
    }
}
