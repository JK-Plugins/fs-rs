use after_effects::{
    self as ae,
    sys::{PF_FpLong, PF_PixelFloat},
};

use libs::{
    hls::HLSA,
    utils::{abs, conv_32_to_8, conv_8_to_32, get_luma, round_byte_long},
};

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
                f.set_default(2);
                f.set_value(f.default());
                f.set_options(&["Normal", "Lighten", "Darken", "Screen", "Multiply"]);
                // f.add_item("Normal");
                // f.add_item("Lighten");
                // f.add_item("Darken");
                // f.add_item("Screen");
                // f.add_item("Multiply");
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
        out_data.set_return_msg("JK Optical Diffusion");
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

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        // let level = params.get(Params::Level)?.as_float_slider()?.value();
        // let color = params.get(Params::Color)?.as_color()?.value();
        let extract_enabled = params.get(Params::ExtractEnabled)?.as_checkbox()?.value();
        let mut black_point = params.get(Params::BlackPoint)?.as_slider()?.value();
        let white_point = params.get(Params::WhitePoint)?.as_slider()?.value();
        let mut black_softness = params.get(Params::BlackSoftness)?.as_slider()?.value();
        let white_softness = params.get(Params::WhiteSoftness)?.as_slider()?.value();
        let invert = params.get(Params::Invert)?.as_checkbox()?.value();

        if black_point > white_point {
            let tmp = black_point - white_point;
            black_point -= tmp;
            if black_point < 0 {
                black_point = 0;
            }
            black_softness -= tmp;
            if black_softness < 0 {
                black_softness = 0;
            }
        }

        let range = params.get(Params::Range)?.as_float_slider()?.value() as i32;
        let use_count = params.get(Params::UseCount)?.as_slider()?.value();
        // let color1 = params.get(Params::Color1)?.as_color()?.value();
        // let color2 = params.get(Params::Color2)?.as_color()?.value();
        // let color3 = params.get(Params::Color3)?.as_color()?.value();
        // let color4 = params.get(Params::Color4)?.as_color()?.value();
        // let color5 = params.get(Params::Color5)?.as_color()?.value();
        // let color6 = params.get(Params::Color6)?.as_color()?.value();
        // let color7 = params.get(Params::Color7)?.as_color()?.value();
        // let color8 = params.get(Params::Color8)?.as_color()?.value();

        // let mut p;
        let mut p32;
        let mut extract_colors = Vec::new();

        let cnt = use_count;
        for i in 0..cnt {
            p32 = conv_8_to_32(
                &params
                    .get(
                        match Params::from_index(Params::Color1 as usize + i as usize) {
                            Some(p) => p,
                            None => continue,
                        },
                    )?
                    .as_color()?
                    .value(),
            );
            extract_colors.push(HLSA::rgb_to_hls(&p32));
        }

        let minimax_1st = params.get(Params::Minimax1st)?.as_slider()?.value();
        let minimax_2nd = params.get(Params::Minimax2nd)?.as_slider()?.value();
        let blur = params.get(Params::Blur)?.as_float_slider()?.value();

        let blend_mode = params.get(Params::BlendMode)?.as_popup()?.value();
        let mut blend_opacity = params.get(Params::BlendOpacity)?.as_float_slider()?.value();

        blend_opacity = blend_opacity / 100.0;
        blend_opacity = blend_opacity.clamp(0.0, 1.0);

        let mut noise = params.get(Params::Noise)?.as_float_slider()?.value();
        noise = noise / 100.0;
        noise = noise.clamp(0.0, 1.0);

        if blend_opacity <= 0.0 {
            noise = 0.0;
        } else if blend_opacity < 0.25 {
            noise = noise * blend_opacity / 0.25;
        }

        let progress_final = out_layer.height() as _;

        if blend_opacity <= 0.0 {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

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
                    (ae::GenericPixel::Pixel8(pixel), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        // Write Your Code Here In Pixel8

                        // 画像抽出
                        if extract_enabled {
                            // extract8
                            let alpha = out_pixel.alpha;
                            out_pixel.alpha = 0;

                            // 元がこれ
                            // A_long v = (A_long)( (( 0.29891 * (PF_FpLong)outP->red) + ( 0.58661 * (PF_FpLong)outP->green) + ( 0.11448 * (PF_FpLong)outP->blue)) * ((PF_FpLong)a / PF_MAX_CHAN8) +0.5);
                            let luma = (get_luma(pixel) * (alpha as f32 / 255.0) + 0.5) as i32;
                            let mut luma2 = luma;

                            let b0 = black_point - black_softness;
                            let b1 = black_point;
                            let w0 = white_point;
                            let w1 = white_point + white_softness;

                            if luma < b0 {
                                luma2 = 0;
                            } else if luma < b1 {
                                luma2 = 255 * (luma - b0) / (b1 - b0);
                            } else if luma <= w0 {
                                luma2 = 255;
                            } else if luma < w1 {
                                luma2 = 255 - (255 * (luma - w0) / (w1 - w0));
                            } else {
                                luma2 = 0;
                            }

                            if invert {
                                luma2 = 255 - luma2;
                            }

                            out_pixel.alpha = round_byte_long(alpha as i32 * luma2 / 255);

                            if use_count > 0 && out_pixel.alpha < 255 {
                                let p32 = conv_8_to_32(pixel);
                                let hlsa = HLSA::rgb_to_hls(&p32);

                                for i in 0..(use_count as usize) {
                                    let a_hue =
                                        (abs(hlsa.hue - extract_colors[i].hue * 65536.0)) as i32;
                                    let a_saturation =
                                        abs(hlsa.saturation
                                            - extract_colors[i].saturation * 65536.0)
                                            as i32;
                                    if a_hue <= range && a_saturation <= range {
                                        let a_lightness =
                                            (abs(hlsa.lightness
                                                - extract_colors[i].lightness * 65536.0))
                                                as i32;
                                        if a_lightness < 0x4000 {
                                            let a2 = round_byte_long(
                                                255 * (0x4000 - a_lightness) / 0x4000,
                                            );
                                            if out_pixel.alpha < a2 {
                                                out_pixel.alpha = a2;
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }

                        // Minimax iterateでやるべき？

                        if minimax_1st != 0 {}

                        if minimax_2nd != 0 {}
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        // Write Your Code Here In Pixel16
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        // Write Your Code Here In PixelF32
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
