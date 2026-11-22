//! F's Tone の移植。
//!
//! 画素の輝度から Shadows -> Midtones -> Highlights を線形補間した色を引き、
//! 元の色と混ぜる。MidtonesOffset は Midtones を置く輝度位置を動かす。
//!
//! 元の Tone.cpp と Toner.cpp は #include 行以外まったく同じで、画素処理も
//! 同一。名前が違うだけの2本なので、どちらも同じ式を移植してある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Tone

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use after_effects::sys::{PF_Pixel, PF_Pixel16, PF_PixelFloat};

use libs::utils::{
    conv_32_to_16, conv_32_to_8, conv_8_to_32, round_byte_fp_long, round_fp_short,
    round_short_fp_long,
};

/// 元の colcColor。level (0..1) に対応する色を返す。
/// b は Midtones を置く位置 (MidtonesBorder)。
fn calc_color(
    level: f32,
    border: f32,
    shadows: PF_PixelFloat,
    midtones: PF_PixelFloat,
    highlights: PF_PixelFloat,
) -> PF_PixelFloat {
    let lerp = |a: PF_PixelFloat, b: PF_PixelFloat, v: f32| PF_PixelFloat {
        alpha: 0.0,
        red: a.red + (b.red - a.red) * v,
        green: a.green + (b.green - a.green) * v,
        blue: a.blue + (b.blue - a.blue) * v,
    };
    if level <= 0.0 {
        // 元: border が 0 のときだけ Midtones になる。
        if border == 0.0 {
            midtones
        } else {
            shadows
        }
    } else if level >= 1.0 {
        if border == 1.0 {
            midtones
        } else {
            highlights
        }
    } else if level == border {
        midtones
    } else if level < border {
        lerp(shadows, midtones, level / border)
    } else {
        lerp(midtones, highlights, (level - border) / (1.0 - border))
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Highlights,
    Midtones,
    Shadows,
    MidtonesOffset,
    Blend,
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
        // 元: PF_ADD_COLOR(STR_*_COLOR, ...) を3つ。
        for (p, name, rgb) in [
            (Params::Highlights, "Highlights", [0xFFu8, 0xFF, 0xFF]),
            (Params::Midtones, "Midtones", [0x7F, 0x64, 0x46]),
            (Params::Shadows, "Shadows", [0x00, 0x00, 0x00]),
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

        // 元: PF_ADD_FIXED(STR_Midtones_OFFSET, -100, 100, -50, 50, 0, 1, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている
        // (AE は型が変わっても保存値を復元する)。
        params.add(
            Params::MidtonesOffset,
            "MidtonesOffset",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-100.0);
                f.set_valid_max(100.0);
                f.set_slider_min(-50.0);
                f.set_slider_max(50.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FIXED(STR_Blend_w_Original_FIXED, 0, 100, 0, 100, 0, 1, 0, 0, ..)
        params.add(
            Params::Blend,
            "Blend w. Original",
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
        out_data.set_return_msg("fs-rs tone");
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
        // 元の GetParams: 3色は CONV8TO32 して 32bit で持つ。
        let highlights = conv_8_to_32(&params.get(Params::Highlights)?.as_color()?.value());
        let midtones = conv_8_to_32(&params.get(Params::Midtones)?.as_color()?.value());
        let shadows = conv_8_to_32(&params.get(Params::Shadows)?.as_color()?.value());

        // 元: s = offset/100 を -1..1 にクランプし、border = 0.5*(s+1)。
        let s = (params
            .get(Params::MidtonesOffset)?
            .as_float_slider()?
            .value() as f32
            / 100.0)
            .clamp(-1.0, 1.0);
        let border = 0.5 * (s + 1.0);
        let blend = params.get(Params::Blend)?.as_float_slider()?.value() as f32 / 100.0;

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: Blend が 1.0 以上なら画素を走らない (= 素通し)。
        if blend >= 1.0 {
            return Ok(());
        }

        let color_at = |level: f32| calc_color(level, border, shadows, midtones, highlights);

        // 元は 8/16bit では輝度ごとの色をテーブルに作ってから引く。
        // 16bit のテーブルは CONV32TO16 を通すが、これは中で 8bit を
        // 経由するので実際には 256 段しかない (元のまま)。
        let tbl8: Vec<PF_Pixel> = (0..=MAX_CHANNEL8)
            .map(|i| conv_32_to_8(&color_at(i as f32 / MAX_CHANNEL8 as f32)))
            .collect();
        let tbl16: Vec<PF_Pixel16> = (0..=MAX_CHANNEL16)
            .map(|i| conv_32_to_16(&color_at(i as f32 / MAX_CHANNEL16 as f32)))
            .collect();

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
                        // 元: ((r*77)+(g*151)+(b*28))>>8 の整数演算。
                        let v = ((out_pixel.red as i32 * 77)
                            + (out_pixel.green as i32 * 151)
                            + (out_pixel.blue as i32 * 28))
                            >> 8;
                        let col = tbl8[v.clamp(0, MAX_CHANNEL8 as i32) as usize];
                        if blend == 0.0 {
                            out_pixel.red = col.red;
                            out_pixel.green = col.green;
                            out_pixel.blue = col.blue;
                        } else {
                            // 元は PF_FpShort (f32) で掛けて足す。
                            let s = 1.0 - blend;
                            let mix =
                                |o: u8, c: u8| round_byte_fp_long(o as f32 * blend + c as f32 * s);
                            out_pixel.red = mix(out_pixel.red, col.red);
                            out_pixel.green = mix(out_pixel.green, col.green);
                            out_pixel.blue = mix(out_pixel.blue, col.blue);
                        }
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        // 元: 16bit は float で /256 する (8bit の >>8 と違う)。
                        let v = (out_pixel.red as f32 * 77.0 / 256.0)
                            + (out_pixel.green as f32 * 151.0 / 256.0)
                            + (out_pixel.blue as f32 * 28.0 / 256.0);
                        let v = v.clamp(0.0, MAX_CHANNEL16 as f32);
                        // 元は (A_long)v で切り捨ててから引く。
                        let col = tbl16[v as usize];
                        if blend == 0.0 {
                            out_pixel.red = col.red;
                            out_pixel.green = col.green;
                            out_pixel.blue = col.blue;
                        } else {
                            let s = 1.0 - blend;
                            let mix = |o: u16, c: u16| {
                                round_short_fp_long(o as f32 * blend + c as f32 * s)
                            };
                            out_pixel.red = mix(out_pixel.red, col.red);
                            out_pixel.green = mix(out_pixel.green, col.green);
                            out_pixel.blue = mix(out_pixel.blue, col.blue);
                        }
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        // 32bit はテーブルを使わず画素ごとに色を計算する。
                        let g = (out_pixel.red * 77.0 / 256.0)
                            + (out_pixel.green * 151.0 / 256.0)
                            + (out_pixel.blue * 28.0 / 256.0);
                        let col = color_at(g.clamp(0.0, 1.0));
                        if blend == 0.0 {
                            out_pixel.red = col.red;
                            out_pixel.green = col.green;
                            out_pixel.blue = col.blue;
                        } else {
                            let s = 1.0 - blend;
                            let mix = |o: f32, c: f32| round_fp_short(o * blend + c * s);
                            out_pixel.red = mix(out_pixel.red, col.red);
                            out_pixel.green = mix(out_pixel.green, col.green);
                            out_pixel.blue = mix(out_pixel.blue, col.blue);
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
