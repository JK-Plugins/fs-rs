//! F's CCplus の移植。
//!
//! 入力の輝度 (アルファで減光済み) を 0..255 の位置に見立てて、
//! start_color -> center_color -> end_color の2段グラデーションから色を引き、
//! アルファも start_alpha -> end_alpha で補間する。出力は入力を一切残さず
//! この色で塗り替える (元は先に出力をクリアする)。最後に noise_value の
//! 範囲でランダムな明暗を RGB に足す。
//!
//! ノイズは元と同じ xorShift で、フレーム番号で1回種をまき、**アルファが
//! 0 より大きい画素でだけ1つ引く**。引く順で結果が変わるので、iterate
//! ではなく自前の行優先ループで走る。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/CCplus

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::fs_graphics::XorShift;
use libs::utils::{
    conv_8_to_16, conv_8_to_32, round_byte_fp_long, round_byte_long, round_fp_short, round_short,
    round_short_fp_long,
};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    OrgRev,
    ColorStart,
    ColorCenter,
    ColorEnd,
    ColorCenterPos,
    AlphaStart,
    AlphaEnd,
    NoiseValue,
}

/// 元の colorValue8。value は 0..255。
fn color_value8(
    value: i32,
    border: i32,
    start: ae::Pixel8,
    center: ae::Pixel8,
    end: ae::Pixel8,
    alpha_start: i32,
    alpha_end: i32,
) -> ae::Pixel8 {
    let m = MAX_CHANNEL8 as i32;
    let mut r = if value == border {
        center
    } else if value == 0 {
        start
    } else if value >= m {
        end
    } else if value <= border {
        // 元は成分ごとに別々に割って足す (整数演算なので切り捨てが2回)。
        let max = border;
        let (v1, v2) = (max - value, value);
        let mix = |s: u8, c: u8| round_byte_long(s as i32 * v1 / max + c as i32 * v2 / max);
        ae::Pixel8 {
            alpha: 0,
            red: mix(start.red, center.red),
            green: mix(start.green, center.green),
            blue: mix(start.blue, center.blue),
        }
    } else {
        let max = m - border;
        let (v1, v2) = (max - (value - border), value - border);
        let mix = |c: u8, e: u8| round_byte_long(c as i32 * v1 / max + e as i32 * v2 / max);
        ae::Pixel8 {
            alpha: 0,
            red: mix(center.red, end.red),
            green: mix(center.green, end.green),
            blue: mix(center.blue, end.blue),
        }
    };
    r.alpha = round_byte_long(alpha_start + (alpha_end - alpha_start) * value / m);
    r
}

/// 元の colorValue16。16bit 版も整数演算。
fn color_value16(
    value: i32,
    border: i32,
    start: ae::Pixel16,
    center: ae::Pixel16,
    end: ae::Pixel16,
    alpha_start: i32,
    alpha_end: i32,
) -> ae::Pixel16 {
    let m = MAX_CHANNEL16 as i32;
    let mut r = if value == border {
        center
    } else if value == 0 {
        start
    } else if value >= m {
        end
    } else if value <= border {
        let max = border;
        let (v1, v2) = (max - value, value);
        let mix = |s: u16, c: u16| round_short(s as i32 * v1 / max + c as i32 * v2 / max);
        ae::Pixel16 {
            alpha: 0,
            red: mix(start.red, center.red),
            green: mix(start.green, center.green),
            blue: mix(start.blue, center.blue),
        }
    } else {
        let max = m - border;
        let (v1, v2) = (max - (value - border), value - border);
        let mix = |c: u16, e: u16| round_short(c as i32 * v1 / max + e as i32 * v2 / max);
        ae::Pixel16 {
            alpha: 0,
            red: mix(center.red, end.red),
            green: mix(center.green, end.green),
            blue: mix(center.blue, end.blue),
        }
    };
    r.alpha = round_short_fp_long((alpha_start + (alpha_end - alpha_start) * value / m) as f32);
    r
}

/// 元の colorValue32。32bit は 0..1 の実数で、割り算を最後にまとめる。
fn color_value32(
    value: f32,
    border: f32,
    start: ae::PixelF32,
    center: ae::PixelF32,
    end: ae::PixelF32,
    alpha_start: f32,
    alpha_end: f32,
) -> ae::PixelF32 {
    let mut r = if value == border {
        center
    } else if value == 0.0 {
        start
    } else if value >= 1.0 {
        end
    } else if value <= border {
        let max = border;
        let (v1, v2) = (max - value, value);
        let mix = |s: f32, c: f32| round_fp_short((s * v1 + c * v2) / max);
        ae::PixelF32 {
            alpha: 0.0,
            red: mix(start.red, center.red),
            green: mix(start.green, center.green),
            blue: mix(start.blue, center.blue),
        }
    } else {
        let max = 1.0 - border;
        let (v1, v2) = (max - (value - border), value - border);
        let mix = |c: f32, e: f32| round_fp_short((c * v1 + e * v2) / max);
        ae::PixelF32 {
            alpha: 0.0,
            red: mix(center.red, end.red),
            green: mix(center.green, end.green),
            blue: mix(center.blue, end.blue),
        }
    };
    r.alpha = round_fp_short(alpha_start + (alpha_end - alpha_start) * value);
    r
}

/// 元の F_RAND2(-n, n)。整数版の xorShift を使う。
fn rand_range(rng: &mut XorShift, n: i32) -> i32 {
    let span = (n - (-n)) as f64 + 1.0;
    -n + (rng.next() as f64 * span / (1.0 + XorShift::RAND_MAX as f64)) as i32
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
        // 元: PF_ADD_CHECKBOX(StrORG_REV1, StrORG_REV2, TRUE, 0, ID_ORG_REV)
        params.add(
            Params::OrgRev,
            "original_reverce",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(start/center/end, ..)
        for (p, name, rgb) in [
            (Params::ColorStart, "start_color", [0xFFu8, 0xFF, 0xFF]),
            (Params::ColorCenter, "center_color", [0x80, 0x80, 0xFF]),
            (Params::ColorEnd, "end_color", [0x00, 0x00, 0xFF]),
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

        // 元: PF_ADD_FIXED(StrCOLOR_CENTER_POS, 0, 100, 0, 100, 50, 1, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        params.add(
            Params::ColorCenterPos,
            "center_color_pos(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FIXED(StrALPHA_START/END, -200, 200, 0, 100, 100/0, 1, 0, 0, ..)
        for (p, name, dflt) in [
            (Params::AlphaStart, "start_alpha(%)", 100.0),
            (Params::AlphaEnd, "end_alpha(%)", 0.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(-200.0);
                    f.set_valid_max(200.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_SLIDER(StrNOISE_VALUE, 0, 64, 0, 8, 2, ..)
        params.add(
            Params::NoiseValue,
            "noise_value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(64);
                f.set_slider_min(0);
                f.set_slider_max(8);
                f.set_default(2);
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
                out_data.set_return_msg("fs-rs ccplus");
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
        // 元の GetParams: チェックの値を反転して持つ。
        let org_rev = !params.get(Params::OrgRev)?.as_checkbox()?.value();
        let start8 = params.get(Params::ColorStart)?.as_color()?.value();
        let center8 = params.get(Params::ColorCenter)?.as_color()?.value();
        let end8 = params.get(Params::ColorEnd)?.as_color()?.value();

        let m8 = MAX_CHANNEL8 as f32;
        let border8 = round_byte_fp_long(
            m8 * params
                .get(Params::ColorCenterPos)?
                .as_float_slider()?
                .value() as f32
                / 100.0,
        ) as i32;

        // 元: 255 倍して -510..510 にクランプしてから整数にする。
        let alpha_of = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i32, Error> {
            let f = m8 * params.get(p)?.as_float_slider()?.value() as f32 / 100.0;
            Ok(f.clamp(-510.0, 510.0) as i32)
        };
        let alpha_start8 = alpha_of(Params::AlphaStart, params)?;
        let alpha_end8 = alpha_of(Params::AlphaEnd, params)?;
        let noise8 = params.get(Params::NoiseValue)?.as_slider()?.value();

        // 元の to16 / to32。
        let conv8to16 = |v: i32| -> i32 {
            // 元の FS_CONVERT8TO16。整数演算。
            (v * MAX_CHANNEL16 as i32 + 128) / MAX_CHANNEL8 as i32
        };
        let (start16, center16, end16) = (
            conv_8_to_16(&start8),
            conv_8_to_16(&center8),
            conv_8_to_16(&end8),
        );
        let (start32, center32, end32) = (
            conv_8_to_32(&start8),
            conv_8_to_32(&center8),
            conv_8_to_32(&end8),
        );

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth {
            return Err(Error::BadCallbackParameter);
        }

        // 元の Exec は出力をクリアしてから入力を読む (コピーしない)。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = XorShift::new(frame);

        // 1周目: 色を引いて塗る。入力の外側はクリアのまま。
        for y in 0..h {
            for x in 0..w {
                let inside = y < in_h && x < in_w;
                match depth {
                    8 => {
                        let v = if !inside {
                            0
                        } else {
                            let p = in_layer.as_pixel8(x, y);
                            if p.alpha == 0 {
                                0
                            } else {
                                // 元の Pixel8Level: 輝度を切り捨てて 0..255。
                                let l = (0.29891 * p.red as f64
                                    + 0.58661 * p.green as f64
                                    + 0.11448 * p.blue as f64)
                                    .min(MAX_CHANNEL8 as f64)
                                    as i32;
                                (l * p.alpha as i32 / MAX_CHANNEL8 as i32)
                                    .clamp(0, MAX_CHANNEL8 as i32)
                            }
                        };
                        let v = if org_rev { MAX_CHANNEL8 as i32 - v } else { v };
                        *out_layer.as_pixel8_mut(x, y) = color_value8(
                            v,
                            border8,
                            start8,
                            center8,
                            end8,
                            alpha_start8,
                            alpha_end8,
                        );
                    }
                    16 => {
                        let m = MAX_CHANNEL16 as i32;
                        let v = if !inside {
                            0
                        } else {
                            let p = in_layer.as_pixel16(x, y);
                            if p.alpha == 0 {
                                0
                            } else {
                                let l = (0.29891 * p.red as f64
                                    + 0.58661 * p.green as f64
                                    + 0.11448 * p.blue as f64)
                                    .min(m as f64) as i32;
                                (l * p.alpha as i32 / m).clamp(0, m)
                            }
                        };
                        let v = if org_rev { m - v } else { v };
                        *out_layer.as_pixel16_mut(x, y) = color_value16(
                            v,
                            conv8to16(border8),
                            start16,
                            center16,
                            end16,
                            conv8to16(alpha_start8),
                            conv8to16(alpha_end8),
                        );
                    }
                    _ => {
                        let v = if !inside {
                            0.0
                        } else {
                            let p = in_layer.as_pixel32(x, y);
                            if p.alpha == 0.0 {
                                0.0
                            } else {
                                let l = 0.29891 * p.red + 0.58661 * p.green + 0.11448 * p.blue;
                                (l * p.alpha).clamp(0.0, 1.0)
                            }
                        };
                        let v = if org_rev { 1.0 - v } else { v };
                        *out_layer.as_pixel32_mut(x, y) = color_value32(
                            v,
                            border8 as f32 / m8,
                            start32,
                            center32,
                            end32,
                            alpha_start8 as f32 / m8,
                            alpha_end8 as f32 / m8,
                        );
                    }
                }
            }
        }

        // 2周目: ノイズ。元は noise_value > 0 のときだけ走り、アルファが
        // 0 より大きい画素でだけ乱数を1つ引く。
        if noise8 <= 0 {
            return Ok(());
        }
        // 元: 16bit と 32bit は noise_value を FS_CONVERT8TO16 した値を使う。
        let noise16 = conv8to16(noise8);
        for y in 0..h {
            for x in 0..w {
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        if p.alpha > 0 {
                            let v = rand_range(&mut rng, noise8);
                            p.red = round_byte_long(p.red as i32 + v);
                            p.green = round_byte_long(p.green as i32 + v);
                            p.blue = round_byte_long(p.blue as i32 + v);
                        }
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        if p.alpha > 0 {
                            let v = rand_range(&mut rng, noise16);
                            p.red = round_short(p.red as i32 + v);
                            p.green = round_short(p.green as i32 + v);
                            p.blue = round_short(p.blue as i32 + v);
                        }
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        if p.alpha > 0.0 {
                            // 元は 16bit スケールで引いてから /PF_MAX_CHAN16 する。
                            let v = rand_range(&mut rng, noise16) as f32 / MAX_CHANNEL16 as f32;
                            p.red = round_fp_short(p.red + v);
                            p.green = round_fp_short(p.green + v);
                            p.blue = round_fp_short(p.blue + v);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
