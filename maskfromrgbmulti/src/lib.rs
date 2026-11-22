//! F's MaskFromRGB_Multi の移植。
//!
//! R/G/B/Y/M/C を個別に選び、それぞれの「その色らしさ」(その成分 - 他2成分の
//! 最大値) を level 倍してアルファに足し込む。RGB は白で塗りつぶす。
//! Y/M/C は RGB を反転したもの (rev) に対して B/G/R の式を使う。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MaskFromRGB_Multi

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_fp_long, round_fp_short, round_short_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Level,
    Red,
    Green,
    Blue,
    Yellow,
    Magenta,
    Cyan,
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_LEVEL, 0, 5000, 50, 300, 1, 100, 1, 0, 0, ID_LEVEL)
        params.add(
            Params::Level,
            "Level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(5000.0);
                f.set_slider_min(50.0);
                f.set_slider_max(300.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_CHECKBOX(STR_x, STR_ON, 既定値, 0, ID_x) を6色分。
        // Red だけ既定が TRUE。
        //
        // 元のフラグは SUPERVISE | CANNOT_TIME_VARY | CANNOT_INTERP だが、
        // 元の USER_CHANGED_PARAM ハンドラは中身が空で何もしないので
        // SUPERVISE は落としてある。
        for (p, name, dflt) in [
            (Params::Red, "Red", true),
            (Params::Green, "Green", false),
            (Params::Blue, "Blue", false),
            (Params::Yellow, "Yellow", false),
            (Params::Magenta, "Magenta", false),
            (Params::Cyan, "Cyan", false),
        ] {
            params.add_with_flags(
                p,
                name,
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
                ae::ParamFlag::CANNOT_TIME_VARY | ae::ParamFlag::CANNOT_INTERP,
                ae::ParamUIFlags::NONE,
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
        out_data.set_return_msg("fs-rs maskfromrgbmulti");
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
        // 元の GetParams: Level は /100 される。
        let level = params.get(Params::Level)?.as_float_slider()?.value() as f32 / 100.0;
        let on = [
            params.get(Params::Red)?.as_checkbox()?.value(),
            params.get(Params::Green)?.as_checkbox()?.value(),
            params.get(Params::Blue)?.as_checkbox()?.value(),
            params.get(Params::Yellow)?.as_checkbox()?.value(),
            params.get(Params::Magenta)?.as_checkbox()?.value(),
            params.get(Params::Cyan)?.as_checkbox()?.value(),
        ];
        // 元: colFlag[Y]||colFlag[M]||colFlag[C]
        let any_ymc = on[3] || on[4] || on[5];

        // 各色が参照する (取り出す成分, 比較する2成分) の組。添字は 0=R,1=G,2=B。
        // R/G/B は col を、Y/M/C は反転した rev を見る。
        const PICK: [(usize, usize, usize); 6] = [
            (0, 2, 1), // R: red   - max(blue, green)
            (1, 2, 0), // G: green - max(blue, red)
            (2, 1, 0), // B: blue  - max(green, red)
            (2, 1, 0), // Y: rev.blue  - max(rev.green, rev.red)
            (1, 2, 0), // M: rev.green - max(rev.blue, rev.red)
            (0, 2, 1), // C: rev.red   - max(rev.blue, rev.green)
        ];

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
                        let m = MAX_CHANNEL8 as f32;
                        let a = out_pixel.alpha as f32;
                        let mut col = [
                            out_pixel.red as f32,
                            out_pixel.green as f32,
                            out_pixel.blue as f32,
                        ];
                        let mut rev = col;
                        let mut v: f32 = 0.0;
                        if a != 0.0 {
                            // 元は Y/M/C が1つでも選ばれていると col 側のマット処理を
                            // 丸ごとスキップする。R/G/B の項は col を見るので、
                            // 半透明部分の結果が Y/M/C の選択有無で変わってしまう。
                            // else 側では col をマット処理しているので意図は
                            // 「使う色をマット処理する」であり、両方に適用する。
                            if any_ymc {
                                for i in 0..3 {
                                    rev[i] = round_byte_fp_long(m - col[i]) as f32;
                                }
                                if a < m {
                                    let af = a / m;
                                    for c in rev.iter_mut() {
                                        *c = round_byte_fp_long(*c * af) as f32;
                                    }
                                    for c in col.iter_mut() {
                                        *c = round_byte_fp_long(*c * af) as f32;
                                    }
                                }
                            } else if a < m {
                                let af = a / m;
                                for c in col.iter_mut() {
                                    *c = round_byte_fp_long(*c * af) as f32;
                                }
                            }
                            // 各項は round_byte_fp_long で 0..最大値 に丸めてから足す。負の差は 0 になる。
                            // 2つ目以降は元と同じく v < 最大値 のときだけ足す。
                            for (i, &(b, x, y)) in PICK.iter().enumerate() {
                                if !on[i] || (i > 0 && v >= m) {
                                    continue;
                                }
                                let s = if i < 3 { &col } else { &rev };
                                let ax = s[x].max(s[y]);
                                v += round_byte_fp_long((s[b] - ax) * level) as f32;
                            }
                            if v > m {
                                v = m;
                            }
                        }
                        out_pixel.alpha = v as u8;
                        out_pixel.red = MAX_CHANNEL8 as u8;
                        out_pixel.green = MAX_CHANNEL8 as u8;
                        out_pixel.blue = MAX_CHANNEL8 as u8;
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f32;
                        let a = out_pixel.alpha as f32;
                        let mut col = [
                            out_pixel.red as f32,
                            out_pixel.green as f32,
                            out_pixel.blue as f32,
                        ];
                        let mut rev = col;
                        let mut v: f32 = 0.0;
                        if a != 0.0 {
                            // 元は Y/M/C が1つでも選ばれていると col 側のマット処理を
                            // 丸ごとスキップする。R/G/B の項は col を見るので、
                            // 半透明部分の結果が Y/M/C の選択有無で変わってしまう。
                            // else 側では col をマット処理しているので意図は
                            // 「使う色をマット処理する」であり、両方に適用する。
                            if any_ymc {
                                for i in 0..3 {
                                    rev[i] = round_short_fp_long(m - col[i]) as f32;
                                }
                                if a < m {
                                    let af = a / m;
                                    for c in rev.iter_mut() {
                                        *c = round_short_fp_long(*c * af) as f32;
                                    }
                                    for c in col.iter_mut() {
                                        *c = round_short_fp_long(*c * af) as f32;
                                    }
                                }
                            } else if a < m {
                                let af = a / m;
                                for c in col.iter_mut() {
                                    *c = round_short_fp_long(*c * af) as f32;
                                }
                            }
                            // 各項は round_short_fp_long で 0..最大値 に丸めてから足す。負の差は 0 になる。
                            // 2つ目以降は元と同じく v < 最大値 のときだけ足す。
                            for (i, &(b, x, y)) in PICK.iter().enumerate() {
                                if !on[i] || (i > 0 && v >= m) {
                                    continue;
                                }
                                let s = if i < 3 { &col } else { &rev };
                                let ax = s[x].max(s[y]);
                                v += round_short_fp_long((s[b] - ax) * level) as f32;
                            }
                            if v > m {
                                v = m;
                            }
                        }
                        out_pixel.alpha = v as u16;
                        out_pixel.red = MAX_CHANNEL16 as u16;
                        out_pixel.green = MAX_CHANNEL16 as u16;
                        out_pixel.blue = MAX_CHANNEL16 as u16;
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        let a = out_pixel.alpha;
                        let mut col = [out_pixel.red, out_pixel.green, out_pixel.blue];
                        let mut v: f32 = 0.0;
                        if a != 0.0 {
                            // 元は 32bit だけ先に 1.0 で上限クランプする。
                            for c in col.iter_mut() {
                                if *c > 1.0 {
                                    *c = 1.0;
                                }
                            }
                            let mut rev = col;
                            // 元は Y/M/C が1つでも選ばれていると col 側のマット処理を
                            // 丸ごとスキップする。R/G/B の項は col を見るので、
                            // 半透明部分の結果が Y/M/C の選択有無で変わってしまう。
                            // else 側では col をマット処理しているので意図は
                            // 「使う色をマット処理する」であり、両方に適用する。
                            if any_ymc {
                                for i in 0..3 {
                                    rev[i] = round_fp_short(1.0 - col[i]);
                                }
                                if a < 1.0 {
                                    for c in rev.iter_mut() {
                                        *c = round_fp_short(*c * a);
                                    }
                                    for c in col.iter_mut() {
                                        *c = round_fp_short(*c * a);
                                    }
                                }
                            } else if a < 1.0 {
                                for c in col.iter_mut() {
                                    *c = round_fp_short(*c * a);
                                }
                            }
                            // 32bit は元が各項を丸めないので、負の差もそのまま
                            // 引かれる (8/16bit とは挙動が違う)。最後に上限だけ
                            // クランプする。
                            for (i, &(b, x, y)) in PICK.iter().enumerate() {
                                if !on[i] || (i > 0 && v >= 1.0) {
                                    continue;
                                }
                                let s = if i < 3 { &col } else { &rev };
                                let ax = s[x].max(s[y]);
                                v += (s[b] - ax) * level;
                            }
                            if v > 1.0 {
                                v = 1.0;
                            }
                        }
                        out_pixel.alpha = v;
                        out_pixel.red = 1.0;
                        out_pixel.green = 1.0;
                        out_pixel.blue = 1.0;
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
