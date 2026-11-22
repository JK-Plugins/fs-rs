//! F's Extract-Hi の移植。
//!
//! 画素から選んだ成分 (輝度 / R / G / B / C / M / Y / RGB最大値 / 指定色との
//! 近さ) を 0..1 で取り出し、Border と Softness で作ったしきい値カーブに
//! 通してアルファに掛ける。RGB は触らない。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Extract-Hi

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_long, round_fp_short, round_short_fp_long};

/// 元の Target ポップアップ (1 始まり)。
const T_LUMINANCE: i32 = 1;
const T_RED: i32 = 2;
const T_GREEN: i32 = 3;
const T_BLUE: i32 = 4;
const T_CYAN: i32 = 5;
const T_MAGENTA: i32 = 6;
const T_YELLOW: i32 = 7;
const T_RGB_MAX: i32 = 8;
const T_CUSTOM: i32 = 9;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Border,
    Softness,
    Target,
    CustomColor,
    Invert,
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_BORDER, 0, 100, 0, 100, 1, 90, 1, 0, 0, ID_BORDER)
        //     PF_ADD_FLOAT_SLIDER(STR_SOFTNESS, 0, 100, 0, 100, 1, 20, 1, 0, 0, ..)
        for (p, name, dflt) in [
            (Params::Border, "Border", 90.0),
            (Params::Softness, "Softness", 20.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_POPUP(STR_TARGET, 9, 1,
        //       "Luminance|Red|Green|Blue|Cyan|Magenta|Yellow|RGB_Max|Custum", ID_TARGET)
        params.add(
            Params::Target,
            "Target",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Luminance",
                    "Red",
                    "Green",
                    "Blue",
                    "Cyan",
                    "Magenta",
                    "Yellow",
                    "RGB_Max",
                    "Custum",
                ]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_CUSTUM_COLOR, 0xFF, 0x00, 0x00, ID_CUSTUM_COLOR)
        params.add(
            Params::CustomColor,
            "CustumColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0x00,
                    blue: 0x00,
                });
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_CHECKBOX(STR_INVERT, STR_INVERT_ON, FALSE, 0, ID_INVERT)
        params.add(
            Params::Invert,
            "Invert",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
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
        out_data.set_return_msg("fs-rs extracthi");
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
        // 元の GetParams: hi = border/100、low = (border - softness)/100。
        let border = params.get(Params::Border)?.as_float_slider()?.value();
        let softness = params.get(Params::Softness)?.as_float_slider()?.value();
        let hi = border / 100.0;
        let low = (border - softness) / 100.0;
        let target = params.get(Params::Target)?.as_popup()?.value();
        let custom = params.get(Params::CustomColor)?.as_color()?.value();
        let (cr, cg, cb) = (
            custom.red as f64 / MAX_CHANNEL8 as f64,
            custom.green as f64 / MAX_CHANNEL8 as f64,
            custom.blue as f64 / MAX_CHANNEL8 as f64,
        );
        let invert = params.get(Params::Invert)?.as_checkbox()?.value();

        // 元の levelComp。
        let comp = move |mut v: f64| -> f64 {
            v = v.clamp(0.0, 1.0);
            if v < low {
                v = 0.0;
            } else if v >= hi {
                v = 1.0;
            } else {
                v = (v - low) / (hi - low);
            }
            v = v.clamp(0.0, 1.0);
            if invert {
                1.0 - v
            } else {
                v
            }
        };

        // 元の level* 群。r/g/b は 0..1 に正規化済みの値を受ける。
        // 元は深度ごとに別関数だが、どれも「正規化してから同じ式」なので
        // 1本にまとめてある (32bit だけ違う点は呼び出し側で合わせる)。
        let level = move |r: f64, g: f64, b: f64| -> f64 {
            match target {
                T_RED => r - (g + b) / 2.0,
                T_GREEN => g - (b + r) / 2.0,
                T_BLUE => b - (r + g) / 2.0,
                // C/M/Y は反転してから R/G/B の式を使う。
                T_CYAN => (1.0 - r) - ((1.0 - g) + (1.0 - b)) / 2.0,
                T_MAGENTA => (1.0 - g) - ((1.0 - b) + (1.0 - r)) / 2.0,
                T_YELLOW => (1.0 - b) - ((1.0 - r) + (1.0 - g)) / 2.0,
                T_RGB_MAX => r.max(g).max(b),
                T_CUSTOM => {
                    // 指定色との距離を sqrt(3) で割って 1 から引く。
                    let v = ((r - cr).powi(2) + (g - cg).powi(2) + (b - cb).powi(2)).sqrt()
                        / 3.0_f64.sqrt();
                    1.0 - v.min(1.0)
                }
                // T_LUMINANCE とそれ以外
                _ => 0.29891 * r + 0.58661 * g + 0.11448 * b,
            }
        };

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

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
                        let v = level(
                            out_pixel.red as f64 / m,
                            out_pixel.green as f64 / m,
                            out_pixel.blue as f64 / m,
                        );
                        let v = (out_pixel.alpha as f64 / m) * comp(v);
                        out_pixel.alpha = round_byte_long((v * m + 0.5) as i32);
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f64;
                        let v = level(
                            out_pixel.red as f64 / m,
                            out_pixel.green as f64 / m,
                            out_pixel.blue as f64 / m,
                        );
                        let v = (out_pixel.alpha as f64 / m) * comp(v);
                        out_pixel.alpha = round_short_fp_long((v * m + 0.5) as f32);
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        // 32bit は正規化済みなので割らずに渡す。
                        let (r, g, b) = (
                            out_pixel.red as f64,
                            out_pixel.green as f64,
                            out_pixel.blue as f64,
                        );
                        let v = match target {
                            // 元の 32bit 版は輝度だけ 0..1 にクランプする。
                            T_LUMINANCE => {
                                (0.29891 * r + 0.58661 * g + 0.11448 * b).clamp(0.0, 1.0)
                            }
                            // 元の 32bit 版は反転後に負だけ 0 でクランプする。
                            T_CYAN | T_MAGENTA | T_YELLOW => {
                                // 元の levelYellow32 だけ反転を
                                // `PF_MAX_CHAN16 - col->red` と書いていて、
                                // 0..1 の値から 32768 を引いていた
                                // (Cyan/Magenta は 1.0 - で正しい)。揃えた。
                                let inv = |c: f64| 1.0 - c;
                                let (ir, ig, ib) =
                                    (inv(r).max(0.0), inv(g).max(0.0), inv(b).max(0.0));
                                match target {
                                    T_CYAN => ir - (ig + ib) / 2.0,
                                    T_MAGENTA => ig - (ib + ir) / 2.0,
                                    _ => ib - (ir + ig) / 2.0,
                                }
                            }
                            // 元の 32bit 版は 1.0 を超える成分を 0 にしてから測る。
                            T_CUSTOM => {
                                let f = |c: f64| if c > 1.0 { 0.0 } else { c };
                                let v = ((f(r) - cr).powi(2)
                                    + (f(g) - cg).powi(2)
                                    + (f(b) - cb).powi(2))
                                .sqrt()
                                    / 3.0_f64.sqrt();
                                1.0 - v.min(1.0)
                            }
                            _ => level(r, g, b),
                        };
                        let v = out_pixel.alpha as f64 * comp(v);
                        out_pixel.alpha = round_fp_short(v as f32);
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
