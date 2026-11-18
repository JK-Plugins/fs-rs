//! F's graytoneToColorize の移植。
//!
//! 画素を選んだチャンネル (輝度 / R / G / B / アルファ) のグレーにしてから、
//! その値で 9 色のグラデーションを引いて着色する。repeat でグラデーションを
//! 繰り返し、offset で位相をずらす (offset_speed はフレームごとに進む)。
//!
//! 元は「グレー化」と「着色」で iterate を2回まわすが、2周目は1周目が
//! 書いた出力しか読まないので、ここでは1回にまとめてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/graytoneToColorize

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_8_to_16, conv_8_to_32, round_byte_long, round_fp_short, round_short};

/// 元の COL_COUNT。
const COL_COUNT: usize = 9;

/// 元の target ポップアップ (1 始まり)。
const T_BRIGHTNESS: i32 = 1;
const T_RED: i32 = 2;
const T_GREEN: i32 = 3;
const T_BLUE: i32 = 4;
const T_ALPHA: i32 = 5;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Target,
    Color0,
    Color1,
    Color2,
    Color3,
    Color4,
    Color5,
    Color6,
    Color7,
    Color8,
    Repeat,
    OffsetOn,
    Offset,
    OffsetSpeed,
}

/// パラメータのスロット順は元の enum どおり。
const COLOR_PARAMS: [Params; COL_COUNT] = [
    Params::Color0,
    Params::Color1,
    Params::Color2,
    Params::Color3,
    Params::Color4,
    Params::Color5,
    Params::Color6,
    Params::Color7,
    Params::Color8,
];
const COLOR_NAMES: [&str; COL_COUNT] = [
    "color0", "color1", "color2", "color3", "color4", "color5", "color6", "color7", "color8",
];

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
        // 元: PF_ADD_POPUP(STR_TARGET, 5, 1, "brightness|red|green|blue|alpha", ID_TARGET)
        params.add(
            Params::Target,
            "target",
            ae::PopupDef::setup(|f| {
                f.set_options(&["brightness", "red", "green", "blue", "alpha"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_COLn, ..) を9色。
        const COLS: [[u8; 3]; COL_COUNT] = [
            [0x9E, 0x87, 0x2A],
            [0xF2, 0xF3, 0x9C],
            [0xA7, 0x99, 0x39],
            [0xF8, 0xF0, 0x68],
            [0xBA, 0x9B, 0x0A],
            [0xD3, 0xBE, 0x11],
            [0xF9, 0xDF, 0x4C],
            [0xFC, 0xF5, 0xCC],
            [0x9E, 0x87, 0x2A],
        ];
        for (i, rgb) in COLS.iter().enumerate() {
            params.add(
                COLOR_PARAMS[i],
                COLOR_NAMES[i],
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

        // 元: PF_ADD_FLOAT_SLIDER(STR_REP, 1, 10, 1, 5, 0, 2.5, 1, 0, 0, ID_REP)
        params.add(
            Params::Repeat,
            "repeat",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0);
                f.set_valid_max(10.0);
                f.set_slider_min(1.0);
                f.set_slider_max(5.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(2.5);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_CHECKBOX(STR_OFFSET_ON, "on", TRUE, 0, ID_OFFSET_ON)
        params.add(
            Params::OffsetOn,
            "offset_enabled",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_ANGLE(STR_OFFSET, 0, ID_OFFSET)
        params.add(
            Params::Offset,
            "offset",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FLOAT_SLIDER(STR_OFFSET_SPEED, -10, 10, -2, 2, 0, 0.2, 2, 0, 0, ..)
        params.add(
            Params::OffsetSpeed,
            "offset_speed",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-10.0);
                f.set_valid_max(10.0);
                f.set_slider_min(-2.0);
                f.set_slider_max(2.0);
                f.set_precision(2);
                f.set_curve_tolerance(0.05);
                f.set_default(0.2);
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
        out_data.set_return_msg("fs-rs graytonetocolorize");
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
        let target = params.get(Params::Target)?.as_popup()?.value();
        let mut col8 = [ae::Pixel8 {
            alpha: 0,
            red: 0,
            green: 0,
            blue: 0,
        }; COL_COUNT];
        for i in 0..COL_COUNT {
            col8[i] = params.get(COLOR_PARAMS[i])?.as_color()?.value();
            // 元の GetParams はアルファを最大に上書きする。
            col8[i].alpha = MAX_CHANNEL8 as u8;
        }
        let rep = params.get(Params::Repeat)?.as_float_slider()?.value();

        // 元の GetParams: offset が無効なら角度と速度を 0 扱いにする。
        let offset_on = params.get(Params::OffsetOn)?.as_checkbox()?.value();
        let (offset, offset_speed) = if offset_on {
            // 元: PF_Fixed の角度を % (360<<16) で 0..360 に正規化して /360。
            // crate 側は角度を度の f32 で渡すので、度のまま同じことをする。
            let mut d = params.get(Params::Offset)?.as_angle()?.value() % 360.0;
            if d < 0.0 {
                d += 360.0;
            }
            let speed = params.get(Params::OffsetSpeed)?.as_float_slider()?.value() / 24.0;
            (d as f64 / 360.0, speed)
        } else {
            (0.0, 0.0)
        };

        // 元: m_frame = current_time / time_step。
        let frame = if in_data.time_step() != 0 {
            (in_data.current_time() / in_data.time_step()) as f64
        } else {
            0.0
        };
        let phase = offset + offset_speed * frame;

        // 元の colBorder。8bit は 255 刻み、16/32bit はどちらも 32768 刻み。
        let border8: [i64; COL_COUNT] = std::array::from_fn(|i| {
            let v = i as f64 * MAX_CHANNEL8 as f64 / (COL_COUNT - 1) as f64;
            ((v + 0.5) as i64).min(MAX_CHANNEL8 as i64)
        });
        let border16: [i64; COL_COUNT] = std::array::from_fn(|i| {
            let v = i as f64 * MAX_CHANNEL16 as f64 / (COL_COUNT - 1) as f64;
            (v + 0.5) as i64
        });

        let col16: [ae::Pixel16; COL_COUNT] = std::array::from_fn(|i| conv_8_to_16(&col8[i]));
        let col32: [ae::PixelF32; COL_COUNT] = std::array::from_fn(|i| conv_8_to_32(&col8[i]));

        // 元の着色部。v (0..max) から色を引く。
        let pick = |v: i64, border: &[i64; COL_COUNT]| -> (usize, f64) {
            let mut idx = 0;
            for i in 0..COL_COUNT {
                if v <= border[i] {
                    idx = i;
                    break;
                }
            }
            if idx == 0 {
                (0, 0.0)
            } else {
                let p = (v - border[idx - 1]) as f64 / (border[idx] - border[idx - 1]) as f64;
                (idx, p)
            }
        };

        // 元: repeat は % max (8bit なら 255 で、256 ではない)、
        // offset は足したあと % (max+1)。
        let warp = |mut v: i64, max: i64| -> i64 {
            if rep > 1.0 {
                v = (v as f64 * rep) as i64 % max;
            }
            if offset_on {
                v = (v as f64 + phase * max as f64) as i64;
                v %= max + 1;
            }
            v
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
                    (ae::GenericPixel::Pixel8(pixel), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        // 1周目相当: 選んだチャンネルをアルファで減光してグレーに。
                        let m = MAX_CHANNEL8 as f64;
                        let a = pixel.alpha as f64 / m;
                        let one = |c: u8| round_byte_long((c as f64 * a + 0.5) as i32) as i64;
                        let g = match target {
                            T_RED => one(pixel.red),
                            T_GREEN => one(pixel.green),
                            T_BLUE => one(pixel.blue),
                            T_ALPHA => pixel.alpha as i64,
                            // T_BRIGHTNESS とそれ以外
                            _ => {
                                let f = |c: u8| ((c as f64 * a + 0.5) as i64).min(m as i64);
                                let v = 0.29891 * f(pixel.red) as f64
                                    + 0.58661 * f(pixel.green) as f64
                                    + 0.11448 * f(pixel.blue) as f64
                                    + 0.5;
                                round_byte_long(v as i32) as i64
                            }
                        };
                        // 2周目相当。
                        let v = warp(g, MAX_CHANNEL8 as i64);
                        let (idx, p) = pick(v, &border8);
                        let c = if idx == 0 {
                            col8[0]
                        } else {
                            // 元の colBlend8。
                            let (s, d) = (col8[idx - 1], col8[idx]);
                            if p == 0.0 {
                                s
                            } else if p >= 1.0 {
                                d
                            } else {
                                let mix = |sc: u8, dc: u8| {
                                    round_byte_long((dc as f64 * p + sc as f64 * (1.0 - p)) as i32)
                                };
                                ae::Pixel8 {
                                    alpha: d.alpha,
                                    red: mix(s.red, d.red),
                                    green: mix(s.green, d.green),
                                    blue: mix(s.blue, d.blue),
                                }
                            }
                        };
                        *out_pixel = c;
                        // 元は最後に元のアルファへ戻す。
                        out_pixel.alpha = pixel.alpha;
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f64;
                        let a = pixel.alpha as f64 / m;
                        let one = |c: u16| round_short((c as f64 * a + 0.5) as i32) as i64;
                        let g = match target {
                            T_RED => one(pixel.red),
                            T_GREEN => one(pixel.green),
                            T_BLUE => one(pixel.blue),
                            T_ALPHA => pixel.alpha as i64,
                            _ => {
                                let f = |c: u16| ((c as f64 * a + 0.5) as i64).min(m as i64);
                                let v = 0.29891 * f(pixel.red) as f64
                                    + 0.58661 * f(pixel.green) as f64
                                    + 0.11448 * f(pixel.blue) as f64
                                    + 0.5;
                                round_short(v as i32) as i64
                            }
                        };
                        let v = warp(g, MAX_CHANNEL16 as i64);
                        let (idx, p) = pick(v, &border16);
                        let c = if idx == 0 {
                            col16[0]
                        } else {
                            let (s, d) = (col16[idx - 1], col16[idx]);
                            if p == 0.0 {
                                s
                            } else if p >= 1.0 {
                                d
                            } else {
                                let mix = |sc: u16, dc: u16| {
                                    round_short((dc as f64 * p + sc as f64 * (1.0 - p)) as i32)
                                };
                                ae::Pixel16 {
                                    alpha: d.alpha,
                                    red: mix(s.red, d.red),
                                    green: mix(s.green, d.green),
                                    blue: mix(s.blue, d.blue),
                                }
                            }
                        };
                        *out_pixel = c;
                        out_pixel.alpha = pixel.alpha;
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        // 32bit も着色は 16bit の目盛りで行う (元のまま)。
                        let a = pixel.alpha as f64;
                        let g = match target {
                            T_RED => round_fp_short((pixel.red as f64 * a) as f32),
                            T_GREEN => round_fp_short((pixel.green as f64 * a) as f32),
                            T_BLUE => round_fp_short((pixel.blue as f64 * a) as f32),
                            T_ALPHA => pixel.alpha,
                            _ => {
                                let f = |c: f32| (c as f64 * a).min(1.0);
                                let v = 0.29891 * f(pixel.red)
                                    + 0.58661 * f(pixel.green)
                                    + 0.11448 * f(pixel.blue);
                                round_fp_short(v as f32)
                            }
                        };
                        let v = ((g as f64 * MAX_CHANNEL16 as f64 + 0.5) as i64)
                            .min(MAX_CHANNEL16 as i64);
                        let v = warp(v, MAX_CHANNEL16 as i64);
                        let (idx, p) = pick(v, &border16);
                        let c = if idx == 0 {
                            col32[0]
                        } else {
                            let (s, d) = (col32[idx - 1], col32[idx]);
                            if p == 0.0 {
                                s
                            } else if p >= 1.0 {
                                d
                            } else {
                                let mix = |sc: f32, dc: f32| {
                                    round_fp_short((dc as f64 * p + sc as f64 * (1.0 - p)) as f32)
                                };
                                ae::PixelF32 {
                                    alpha: d.alpha,
                                    red: mix(s.red, d.red),
                                    green: mix(s.green, d.green),
                                    blue: mix(s.blue, d.blue),
                                }
                            }
                        };
                        *out_pixel = c;
                        out_pixel.alpha = pixel.alpha;
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
