use after_effects::{self as ae};

use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32, round_byte_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Level,
    Mode1,
    Target0,
    SrcColor0,
    DstColor0,
    Target1,
    SrcColor1,
    DstColor1,
    Target2,
    SrcColor2,
    DstColor2,
    Target3,
    SrcColor3,
    DstColor3,
    Target4,
    SrcColor4,
    DstColor4,
    Target5,
    SrcColor5,
    DstColor5,
    Target6,
    SrcColor6,
    DstColor6,
    Target7,
    SrcColor7,
    DstColor7,
}

impl Params {
    pub fn from_index(index: usize) -> Option<Self> {
        use Params::*;
        match index {
            0 => Some(Level),
            1 => Some(Mode1),
            2 => Some(Target0),
            3 => Some(SrcColor0),
            4 => Some(DstColor0),
            5 => Some(Target1),
            6 => Some(SrcColor1),
            7 => Some(DstColor1),
            8 => Some(Target2),
            9 => Some(SrcColor2),
            10 => Some(DstColor2),
            11 => Some(Target3),
            12 => Some(SrcColor3),
            13 => Some(DstColor3),
            14 => Some(Target4),
            15 => Some(SrcColor4),
            16 => Some(DstColor4),
            17 => Some(Target5),
            18 => Some(SrcColor5),
            19 => Some(DstColor5),
            20 => Some(Target6),
            21 => Some(SrcColor6),
            22 => Some(DstColor6),
            23 => Some(Target7),
            24 => Some(SrcColor7),
            25 => Some(DstColor7),
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
            Params::Level,
            "level",
            ae::FloatSliderDef::setup(|f| {
                f.set_default(0.0);
                f.set_precision(1); // 小数点以下の桁数
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_value(f.default());
            }),
        )?;

        // 全体のオンオフ
        params.add(
            Params::Mode1,
            "Enabled",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;

        // 1つ目
        // 個別のオンオフ
        params.add(
            Params::Target0,
            "Target0",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 色の選択
        params.add(
            Params::SrcColor0,
            "Source Color0",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 255,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 変更後の色
        params.add(
            Params::DstColor0,
            "Destination Color0",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 128,
                    green: 128,
                    blue: 128,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 2つ目
        // 個別のオンオフ
        params.add(
            Params::Target1,
            "Target1",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 色の選択
        params.add(
            Params::SrcColor1,
            "Source Color1",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 255,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 変更後の色
        params.add(
            Params::DstColor1,
            "Destination Color1",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 128,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 3つ目
        // 個別のオンオフ
        params.add(
            Params::Target2,
            "Target2",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 色の選択
        params.add(
            Params::SrcColor2,
            "Source Color2",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 変更後の色
        params.add(
            Params::DstColor2,
            "Destination Color2",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 4つ目
        // 個別のオンオフ
        params.add(
            Params::Target3,
            "Target3",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 色の選択
        params.add(
            Params::SrcColor3,
            "Source Color3",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;
        // 変更後の色
        params.add(
            Params::DstColor3,
            "Destination Color3",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 0,
                    blue: 128,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 5つ目（黄色）
        params.add(
            Params::Target4,
            "Target4",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::SrcColor4,
            "Source Color4",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 255,
                    green: 255,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::DstColor4,
            "Destination Color4",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 128,
                    green: 128,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 6つ目（水色）
        params.add(
            Params::Target5,
            "Target5",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::SrcColor5,
            "Source Color5",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 255,
                    blue: 255,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::DstColor5,
            "Destination Color5",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 128,
                    blue: 128,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 7つ目（マゼンタ）
        params.add(
            Params::Target6,
            "Target6",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::SrcColor6,
            "Source Color6",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 255,
                    green: 0,
                    blue: 255,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::DstColor6,
            "Destination Color6",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 128,
                    green: 0,
                    blue: 128,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;

        // 8つ目（黒→白）
        params.add(
            Params::Target7,
            "Target7",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::SrcColor7,
            "Source Color7",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 0,
                    green: 0,
                    blue: 0,
                    alpha: 255,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::DstColor7,
            "Destination Color7",
            ae::ColorDef::setup(|f| {
                f.set_default(Pixel8 {
                    red: 255,
                    green: 255,
                    blue: 255,
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
        out_data.set_return_msg("fs-rs colorchange");
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

    fn collect_enabled_color_pairs(
        params: &ae::Parameters<Params>,
    ) -> Result<Vec<(Pixel8, Pixel8)>, Error> {
        let mut pairs = Vec::new();

        for i in 0..8 {
            let target = match Params::from_index(Params::Target0 as usize + i * 3) {
                Some(p) => p,
                None => continue,
            };
            let src = match Params::from_index(Params::SrcColor0 as usize + i * 3 + 1) {
                Some(p) => p,
                None => continue,
            };
            let dst = match Params::from_index(Params::DstColor0 as usize + i * 3 + 2) {
                Some(p) => p,
                None => continue,
            };

            if params.get(target)?.as_checkbox()?.value() {
                let src_color = params.get(src)?.as_color()?.value();
                let dst_color = params.get(dst)?.as_color()?.value();
                pairs.push((src_color, dst_color));
            }
        }

        Ok(pairs)
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let level = params.get(Params::Level)?.as_float_slider()?.value();
        let level = (MAX_CHANNEL8 as f64 * level / 100.0) as u8; // Convert to 0-255 range
                                                                 // 0.0 - 100.0 => 0 - 255
                                                                 // let level = (MAX_CHANNEL8 as f64 * level / 100.0) as u8; // Convert to 0-255 range
                                                                 // いどう
                                                                 // let src_color = params.get(Params::SrcColor0)?.as_color()?.value();
                                                                 // let dst_color = params.get(Params::DstColor0)?.as_color()?.value();

        let color_pairs = Plugin::collect_enabled_color_pairs(params)?;

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
                    (ae::GenericPixel::Pixel8(pixel), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        for (src_color, dst_color) in &color_pairs {
                            let matched = if level == 0 {
                                pixel.red == src_color.red
                                    && pixel.green == src_color.green
                                    && pixel.blue == src_color.blue
                            } else {
                                pixel.red.abs_diff(src_color.red) <= level
                                    && pixel.green.abs_diff(src_color.green) <= level
                                    && pixel.blue.abs_diff(src_color.blue) <= level
                            };

                            if matched {
                                out_pixel.red = dst_color.red;
                                out_pixel.green = dst_color.green;
                                out_pixel.blue = dst_color.blue;
                                break; // 最初にマッチした色だけ置き換えたい場合
                            }
                        }
                    }

                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let p = &conv_16_to_8(pixel);
                        for (src_color, dst_color) in &color_pairs {
                            let d = conv_8_to_16(&dst_color);

                            let matched = if level == 0 {
                                p.red == src_color.red
                                    && p.green == src_color.green
                                    && p.blue == src_color.blue
                            } else {
                                p.red.abs_diff(src_color.red) <= level
                                    && p.green.abs_diff(src_color.green) <= level
                                    && p.blue.abs_diff(src_color.blue) <= level
                            };
                            if matched {
                                out_pixel.red = d.red;
                                out_pixel.green = d.green;
                                out_pixel.blue = d.blue;
                                break;
                            }
                        }
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let p = &conv_32_to_8(pixel);
                        for (src_color, dst_color) in &color_pairs {
                            let d = conv_8_to_32(&dst_color);

                            let matched = if level == 0 {
                                p.red == src_color.red
                                    && p.green == src_color.green
                                    && p.blue == src_color.blue
                            } else {
                                p.red.abs_diff(src_color.red) <= level
                                    && p.green.abs_diff(src_color.green) <= level
                                    && p.blue.abs_diff(src_color.blue) <= level
                            };
                            if matched {
                                out_pixel.red = d.red;
                                out_pixel.green = d.green;
                                out_pixel.blue = d.blue;
                                break;
                            }
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
