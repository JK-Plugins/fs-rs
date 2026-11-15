mod od_core;

use after_effects as ae;

use crate::od_core::{process, process8, BlendMode, OdParams};
use libs::fs_graphics::{Image, PixelF};
use libs::fs_graphics8::{Image8, Px8};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    ExtractEnabled,
    ExtractLightnessGroupStart,
    BlackPoint,
    WhitePoint,
    BlackSoftness,
    WhiteSoftness,
    Invert,
    ExtractLightnessGroupEnd,
    ExtractColorGroupStart,
    UseCount,
    Range,
    Color1,
    Color2,
    Color3,
    Color4,
    Color5,
    Color6,
    Color7,
    Color8,
    ExtractColorGroupEnd,
    Minimax1st,
    Minimax2nd,
    Blur,
    BlendMode,
    BlendOpacity,
    Noise,
}

impl Params {
    fn from_index(index: usize) -> Option<Self> {
        use Params::*;
        match index {
            0 => Some(ExtractEnabled),
            1 => Some(ExtractLightnessGroupStart),
            2 => Some(BlackPoint),
            3 => Some(WhitePoint),
            4 => Some(BlackSoftness),
            5 => Some(WhiteSoftness),
            6 => Some(Invert),
            7 => Some(ExtractLightnessGroupEnd),
            8 => Some(ExtractColorGroupStart),
            9 => Some(UseCount),
            10 => Some(Range),
            11 => Some(Color1),
            12 => Some(Color2),
            13 => Some(Color3),
            14 => Some(Color4),
            15 => Some(Color5),
            16 => Some(Color6),
            17 => Some(Color7),
            18 => Some(Color8),
            19 => Some(ExtractColorGroupEnd),
            20 => Some(Minimax1st),
            21 => Some(Minimax2nd),
            22 => Some(Blur),
            23 => Some(BlendMode),
            24 => Some(BlendOpacity),
            25 => Some(Noise),
            _ => None,
        }
    }
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
        in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add(
            Params::ExtractEnabled,
            "Extract Enabled",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;

        params.add_group(
            Params::ExtractLightnessGroupStart,
            Params::ExtractLightnessGroupEnd,
            "Extract lightness",
            true,
            |params| {
                // black point
                params.add(
                    Params::BlackPoint,
                    "Black Point",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(128);
                        f.set_slider_min(0);
                        f.set_slider_max(128);
                        f.set_default(64);
                        f.set_value(f.default());
                    }),
                )?;

                // white point
                params.add(
                    Params::WhitePoint,
                    "White Point",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(100);
                        f.set_valid_max(255);
                        f.set_slider_min(128);
                        f.set_slider_max(255);
                        f.set_default(128);
                        f.set_value(f.default());
                    }),
                )?;

                // black softness
                params.add(
                    Params::BlackSoftness,
                    "Black Softness",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(255);
                        f.set_slider_min(0);
                        f.set_slider_max(128);
                        f.set_default(128);
                        f.set_value(f.default());
                    }),
                )?;

                // white softness
                params.add(
                    Params::WhiteSoftness,
                    "White Softness",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(255);
                        f.set_slider_min(0);
                        f.set_slider_max(64);
                        f.set_default(64);
                        f.set_value(f.default());
                    }),
                )?;

                // Invert
                params.add(
                    Params::Invert,
                    "Invert",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_value(f.default());
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_group(
            Params::ExtractColorGroupStart,
            Params::ExtractColorGroupEnd,
            "Extract TargetColor",
            true,
            |params| {
                // Use Count
                params.add(
                    Params::UseCount,
                    "Use Count",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(8);
                        f.set_slider_min(0);
                        f.set_slider_max(8);
                        f.set_default(0);
                        f.set_value(f.default());
                    }),
                )?;

                // Range
                params.add(
                    Params::Range,
                    "Range",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(10.0);
                        f.set_default(0.5);
                        f.set_precision(1);
                        f.set_value(f.default());
                    }),
                )?;

                // Color1
                params.add_with_flags(
                    Params::Color1,
                    "Color 1",
                    ae::ColorDef::setup(|f| {
                        f.set_default(Pixel8 {
                            red: 0xFF,
                            green: 0x00,
                            blue: 0x00,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color2
                params.add_with_flags(
                    Params::Color2,
                    "Color 2",
                    ae::ColorDef::setup(|f| {
                        f.set_default(Pixel8 {
                            red: 0x00,
                            green: 0xFF,
                            blue: 0x00,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color3
                params.add_with_flags(
                    Params::Color3,
                    "Color 3",
                    ae::ColorDef::setup(|f| {
                        f.set_default(Pixel8 {
                            red: 0x00,
                            green: 0x00,
                            blue: 0xFF,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color4
                params.add_with_flags(
                    Params::Color4,
                    "Color 4",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 0xFF,
                            green: 0xFF,
                            blue: 0x00,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color5
                params.add_with_flags(
                    Params::Color5,
                    "Color 5",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 0xFF,
                            green: 0x00,
                            blue: 0xFF,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color6
                params.add_with_flags(
                    Params::Color6,
                    "Color 6",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 0x00,
                            green: 0xFF,
                            blue: 0xFF,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color7
                params.add_with_flags(
                    Params::Color7,
                    "Color 7",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 0xFF,
                            green: 0x80,
                            blue: 0x00,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                // Color8
                params.add_with_flags(
                    Params::Color8,
                    "Color 8",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 0x00,
                            green: 0x80,
                            blue: 0xFF,
                            alpha: 0xFF,
                        });
                        f.set_value(f.default());
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::NONE,
                )?;

                Ok(())
            },
        )?;

        // Minimax 1st
        params.add(
            Params::Minimax1st,
            "Minimax 1st",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-100);
                f.set_valid_max(100);
                f.set_slider_min(-25);
                f.set_slider_max(25);
                f.set_default(-3);
                f.set_value(f.default());
            }),
        )?;

        // Minimax 2nd
        params.add(
            Params::Minimax2nd,
            "Minimax 2nd",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-100);
                f.set_valid_max(100);
                f.set_slider_min(-25);
                f.set_slider_max(25);
                f.set_default(3);
                f.set_value(f.default());
            }),
        )?;

        // Blur
        params.add(
            Params::Blur,
            "Blur",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(255.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_curve_tolerance(30.0);
                f.set_default(1.0);
                f.set_precision(1);
                f.set_display_flags(ae::ValueDisplayFlag::NONE);
                f.set_phase(0.0);
                f.set_value(f.default());
            }),
        )?;

        // Blend Mode
        params.add(
            Params::BlendMode,
            "Blend Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&BlendMode::NAMES);
                f.set_default(BlendMode::DEFAULT);
                f.set_value(f.default());
            }),
        )?;

        // Blend Opacity
        params.add(
            Params::BlendOpacity,
            "Blend Opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(70.0);
                f.set_precision(1);
                f.set_value(f.default());
            }),
        )?;

        // Noise
        params.add(
            Params::Noise,
            "Noise",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(10.0);
                f.set_default(1.0);
                f.set_precision(1);
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
        out_data.set_return_msg("JK Optical Diffusion F's");
    }

    fn global_setup(&mut self, in_data: &InData) -> Result<(), ae::Error> {
        win_dbg_logger::DEBUGGER_LOGGER.set_force_log_without_debugger(true);
        log::info!("GlobalSetup");
        // For Premiere - declare supported pixel formats
        if in_data.is_premiere() {
            let suite = ae::pf::suites::PixelFormat::new()?;

            // Add the pixel formats we support in order of preference.
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
        params: &mut ae::Parameters<Params>,
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

    fn read_params(params: &mut ae::Parameters<Params>) -> Result<OdParams, Error> {
        let use_count = params
            .get(Params::UseCount)?
            .as_slider()?
            .value()
            .clamp(0, 8) as usize;
        let mut extract_colors = Vec::with_capacity(use_count);
        for i in 0..use_count {
            let Some(id) = Params::from_index(Params::Color1 as usize + i) else {
                continue;
            };
            let c = params.get(id)?.as_color()?.value();
            extract_colors.push([
                c.red as f64 / 255.0,
                c.green as f64 / 255.0,
                c.blue as f64 / 255.0,
            ]);
        }

        Ok(OdParams {
            extract_enabled: params.get(Params::ExtractEnabled)?.as_checkbox()?.value(),
            black_point: params.get(Params::BlackPoint)?.as_slider()?.value(),
            white_point: params.get(Params::WhitePoint)?.as_slider()?.value(),
            black_softness: params.get(Params::BlackSoftness)?.as_slider()?.value(),
            white_softness: params.get(Params::WhiteSoftness)?.as_slider()?.value(),
            invert: params.get(Params::Invert)?.as_checkbox()?.value(),
            color_range: params.get(Params::Range)?.as_float_slider()?.value(),
            extract_colors,
            minimax_1st: params.get(Params::Minimax1st)?.as_slider()?.value(),
            minimax_2nd: params.get(Params::Minimax2nd)?.as_slider()?.value(),
            blur: params.get(Params::Blur)?.as_float_slider()?.value(),
            blend_mode: BlendMode::from_popup(params.get(Params::BlendMode)?.as_popup()?.value()),
            blend_opacity: params.get(Params::BlendOpacity)?.as_float_slider()?.value(),
            noise: params.get(Params::Noise)?.as_float_slider()?.value(),
        })
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let od = Self::read_params(params)?;

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth {
            return Err(Error::BadCallbackParameter);
        }

        // ノイズのシードは元と同じくフレーム番号 (current_time / time_step)
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if time >= 0 && step > 0 {
            (time / step) as u32
        } else {
            0
        };

        // 8bit は元の .aex と一致させるため整数演算の実装を使う
        if depth == 8 {
            let mut src = Image8::new(w, h);
            for y in 0..h.min(in_h) {
                for x in 0..w.min(in_w) {
                    let p = in_layer.as_pixel8(x, y);
                    *src.get_mut(x, y) = Px8::new(p.alpha, p.red, p.green, p.blue);
                }
            }
            let dst = process8(&src, &od, frame);
            for y in 0..h {
                for x in 0..w {
                    let p = dst.get(x, y);
                    *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                        alpha: p.alpha,
                        red: p.red,
                        green: p.green,
                        blue: p.blue,
                    };
                }
            }
            return Ok(());
        }

        // 16/32bit: 0.0..=1.0 のストレートアルファの f32 で処理する
        let mut src = Image::new(w, h);
        for y in 0..h.min(in_h) {
            for x in 0..w.min(in_w) {
                *src.get_mut(x, y) = match depth {
                    16 => {
                        let p = in_layer.as_pixel16(x, y);
                        let k = 1.0 / ae::MAX_CHANNEL16 as f32;
                        PixelF::new(
                            p.alpha as f32 * k,
                            p.red as f32 * k,
                            p.green as f32 * k,
                            p.blue as f32 * k,
                        )
                    }
                    32 => {
                        let p = in_layer.as_pixel32(x, y);
                        PixelF::new(p.alpha.clamp(0.0, 1.0), p.red, p.green, p.blue)
                    }
                    _ => return Err(Error::BadCallbackParameter),
                };
            }
        }

        let dst = process(&src, &od, frame);

        // 出力へ書き戻す
        for y in 0..h {
            for x in 0..w {
                let p = dst.get(x, y);
                match depth {
                    16 => {
                        let m = ae::MAX_CHANNEL16 as f32;
                        let q = |c: f32| (c.clamp(0.0, 1.0) * m).round() as u16;
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha: q(p.alpha),
                            red: q(p.red),
                            green: q(p.green),
                            blue: q(p.blue),
                        };
                    }
                    32 => {
                        *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                            alpha: p.alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        };
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }
            }
        }
        Ok(())
    }
}
