//! F's HLS_Reverse の移植。
//!
//! RGB を HLS に変換し、色相・明度・彩度を個別に 1.0 から引いて反転させ、
//! RGB に戻す。最後に元の色と指定割合でブレンドする。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/HLS_Reverse

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::hls::HLSA;
use libs::utils::{round_byte_fp_long, round_short_fp_long};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Hue,
    Lightness,
    Saturation,
    OrgBlend,
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
        // 元: PF_ADD_CHECKBOX(STR_HUE, STR_REV, FALSE, 0, ID_HUE) ほか3つ。
        // チェックボックス横の文字はどれも "Reverse"。既定は明度だけ ON。
        for (p, name, dflt) in [
            (Params::Hue, "hue", false),
            (Params::Lightness, "lightness", true),
            (Params::Saturation, "saturation", false),
        ] {
            params.add(
                p,
                name,
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("Reverse");
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_FIXED(STR_ORG_BLEND, 0, 100, 0, 100, 0, 3, 0, 0, ID_ORG_BLEND)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている
        // (AE は型が変わっても保存値を復元する。OpticalDiffusion の Range 等も同様)。
        params.add(
            Params::OrgBlend,
            "blend original(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(3);
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
        out_data.set_return_msg("fs-rs hlsreverse");
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
        let rev_h = params.get(Params::Hue)?.as_checkbox()?.value();
        let rev_l = params.get(Params::Lightness)?.as_checkbox()?.value();
        let rev_s = params.get(Params::Saturation)?.as_checkbox()?.value();
        // 元の GetParams: org_blend は取得後に /100 される。
        let blend = params.get(Params::OrgBlend)?.as_float_slider()?.value() as f32 / 100.0;

        // 0..1 の RGB を HLS 経由で反転して返す。
        //
        // 型の幅を元に合わせるのが重要。元は PF_PixelFloat (f32) を RGBtoHLS に渡し、
        // HLSA の中は PF_FpLong (f64)、HLStoRGB の戻りは再び PF_PixelFloat (f32) で
        //   r.red = (PF_FpShort)R;
        // と narrowing する。f64 のまま通すと 255 倍して切り捨てたとき 1 ずれる
        // (AEXCompat で元と比べて 11458 px / 最大1 の差として出た)。
        let flip = move |r: f32, g: f32, b: f32| -> (f32, f32, f32) {
            let mut hls = HLSA::from_rgb(r as f64, g as f64, b as f64, 1.0);
            if rev_h {
                hls.hue = 1.0 - hls.hue;
            }
            if rev_l {
                hls.lightness = 1.0 - hls.lightness;
            }
            if rev_s {
                hls.saturation = 1.0 - hls.saturation;
            }
            let (r2, g2, b2, _) = hls.to_rgb();
            (r2 as f32, g2 as f32, b2 as f32)
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
                    (ae::GenericPixel::Pixel8(p), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        let m = MAX_CHANNEL8 as f32;
                        let (r, g, b) =
                            flip(p.red as f32 / m, p.green as f32 / m, p.blue as f32 / m);
                        out_pixel.alpha = p.alpha;
                        // 元は丸めずにキャストするので切り捨て。
                        let (r8, g8, b8) = ((m * r) as u8, (m * g) as u8, (m * b) as u8);
                        if blend == 0.0 {
                            out_pixel.red = r8;
                            out_pixel.green = g8;
                            out_pixel.blue = b8;
                        } else {
                            let k = 1.0 - blend;
                            out_pixel.red =
                                round_byte_fp_long(p.red as f32 * blend + r8 as f32 * k);
                            out_pixel.green =
                                round_byte_fp_long(p.green as f32 * blend + g8 as f32 * k);
                            out_pixel.blue =
                                round_byte_fp_long(p.blue as f32 * blend + b8 as f32 * k);
                        }
                    }
                    (ae::GenericPixel::Pixel16(p), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f32;
                        let (r, g, b) =
                            flip(p.red as f32 / m, p.green as f32 / m, p.blue as f32 / m);
                        out_pixel.alpha = p.alpha;
                        let (r16, g16, b16) = ((m * r) as u16, (m * g) as u16, (m * b) as u16);
                        if blend == 0.0 {
                            out_pixel.red = r16;
                            out_pixel.green = g16;
                            out_pixel.blue = b16;
                        } else {
                            let k = 1.0 - blend;
                            out_pixel.red =
                                round_short_fp_long(p.red as f32 * blend + r16 as f32 * k);
                            out_pixel.green =
                                round_short_fp_long(p.green as f32 * blend + g16 as f32 * k);
                            out_pixel.blue =
                                round_short_fp_long(p.blue as f32 * blend + b16 as f32 * k);
                        }
                    }
                    (ae::GenericPixel::PixelF32(p), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        // 32bit は 0..1 のままなので割らずに渡す。
                        let (r, g, b) = flip(p.red, p.green, p.blue);
                        out_pixel.alpha = p.alpha;
                        if blend == 0.0 {
                            out_pixel.red = r;
                            out_pixel.green = g;
                            out_pixel.blue = b;
                        } else {
                            let k = 1.0 - blend;
                            out_pixel.red = p.red * blend + r * k;
                            out_pixel.green = p.green * blend + g * k;
                            out_pixel.blue = p.blue * blend + b * k;
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
