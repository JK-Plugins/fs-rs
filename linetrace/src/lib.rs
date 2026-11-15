//! F's LineTrace の移植。
//!
//! 画素を白地に合成してから AE の RGBtoHLS で色相・明度・彩度を取り、
//! 黒 / 赤 / 黄 / 緑 / 青 / 紫 のどれに当たるかを色相で振り分けて、
//! 明度がその色の Border より暗ければ指定色で、そうでなければ白で塗る。
//! 線画をトレースして色ごとに塗り分ける用途。
//!
//! 色相の判定は AE のカラースイート (RGBtoHLS) に任せているので、元と
//! 同じホストの上では同じ値が返る。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/LineTrace

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{
    conv_8_to_16, conv_8_to_32, round_byte_fp_long, round_fp_short, round_short_fp_long,
};

/// 元の LT_*。パラメータは黒・赤・緑・青・黄・紫の順に並ぶ。
const LT_BLACK: usize = 0;
const LT_RED: usize = 1;
const LT_GREEN: usize = 2;
const LT_BLUE: usize = 3;
const LT_YELLOW: usize = 4;
const LT_VIOLET: usize = 5;
const LT_COUNT: usize = 6;

/// 元の TARGET_*。PF_Fixed (16.16) の 0.3 / 0.9 / 0.5。
const TARGET_BLACK: i32 = (1 << 16) * 3 / 10;
const TARGET_WHITE: i32 = (1 << 16) * 9 / 10;
const TARGET_GRAY: i32 = (1 << 16) * 5 / 10;

/// 元の RANGE_* / TARGET_*。色相の境界 (度)。
const RANGE_RED0: i32 = 0;
const RANGE_RED1: i32 = 60;
const RANGE_RED2: i32 = 330;
const TARGET_YELLOW1: i32 = 70;
const TARGET_GREEN1: i32 = 175;
const TARGET_BLUE1: i32 = 280;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    BlackEnabled,
    BlackBorder,
    BlackColor,
    RedEnabled,
    RedBorder,
    RedColor,
    GreenEnabled,
    GreenBorder,
    GreenColor,
    BlueEnabled,
    BlueBorder,
    BlueColor,
    YellowEnabled,
    YellowBorder,
    YellowColor,
    VioletEnabled,
    VioletBorder,
    VioletColor,
}

/// スロット順は元の enum どおり (黒・赤・緑・青・黄・紫で、各3つ)。
/// 添字は LT_* に合わせてある。
const SLOTS: [[Params; 3]; LT_COUNT] = [
    [
        Params::BlackEnabled,
        Params::BlackBorder,
        Params::BlackColor,
    ],
    [Params::RedEnabled, Params::RedBorder, Params::RedColor],
    [
        Params::GreenEnabled,
        Params::GreenBorder,
        Params::GreenColor,
    ],
    [Params::BlueEnabled, Params::BlueBorder, Params::BlueColor],
    [
        Params::YellowEnabled,
        Params::YellowBorder,
        Params::YellowColor,
    ],
    [
        Params::VioletEnabled,
        Params::VioletBorder,
        Params::VioletColor,
    ],
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
        // 元: 黒・赤・緑・青・黄・紫の順に
        //   PF_ADD_CHECKBOX(色名, "ON", TRUE, ..)
        //   PF_ADD_FIXED(色名+"Border", 0, 100, 0, 100, 100, 1, 0, 0, ..)
        //   PF_ADD_COLOR("Draw"+色名, r, g, b, ..)
        // 元の Border は FIX_SLIDER だがリポジトリの他と揃えて float
        // スライダーにしている (AE は型が変わっても保存値を復元する)。
        const DEFS: [(&str, [u8; 3]); LT_COUNT] = [
            ("Black", [0x00, 0x00, 0x00]),
            ("Red", [0xFF, 0x00, 0x00]),
            ("Green", [0x00, 0xFF, 0x00]),
            ("Blue", [0x00, 0x00, 0xFF]),
            ("Yellow", [0xFF, 0xFF, 0x00]),
            ("Violet", [0xFF, 0x00, 0xFF]),
        ];
        for (i, (name, rgb)) in DEFS.iter().enumerate() {
            params.add(
                SLOTS[i][0],
                name,
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("ON");
                    f.set_default(true);
                    f.set_value(f.default());
                }),
            )?;
            params.add(
                SLOTS[i][1],
                &format!("{name}Border"),
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(100.0);
                    f.set_value(f.default());
                }),
            )?;
            params.add(
                SLOTS[i][2],
                &format!("Draw{name}"),
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
        out_data.set_return_msg("fs-rs linetrace");
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
        let mut enabled = [false; LT_COUNT];
        let mut border = [0i32; LT_COUNT];
        let mut col8 = [ae::Pixel8 {
            alpha: 0,
            red: 0,
            green: 0,
            blue: 0,
        }; LT_COUNT];
        for i in 0..LT_COUNT {
            enabled[i] = params.get(SLOTS[i][0])?.as_checkbox()?.value();
            // 元の GetFIXED_PER2ONE: PF_Fixed のパーセントを 100 で割って
            // 0..1 の 16.16 固定小数にする。HLS の明度と同じ目盛り。
            let pct = params.get(SLOTS[i][1])?.as_float_slider()?.value();
            border[i] = (pct * 65536.0) as i32 / 100;
            col8[i] = params.get(SLOTS[i][2])?.as_color()?.value();
        }
        let col16: [ae::Pixel16; LT_COUNT] = std::array::from_fn(|i| conv_8_to_16(&col8[i]));
        let col32: [ae::PixelF32; LT_COUNT] = std::array::from_fn(|i| conv_8_to_32(&col8[i]));

        // 元の色相の振り分け。戻り値は LT_* か None (= 白)。
        let pick = move |hls: ae::HLSPixel| -> Option<usize> {
            // HLSPixel の H/L/S は PF_Fixed (16.16)。元は生の PF_Fixed の
            // まま比較するので、ここでも as_fixed() で生値にして比べる。
            let l = hls.l.as_fixed();
            if l >= TARGET_WHITE {
                return None;
            }
            if l < TARGET_BLACK {
                // 元はここだけ Border を見ずに黒を塗る。
                return if enabled[LT_BLACK] {
                    Some(LT_BLACK)
                } else {
                    None
                };
            }
            if hls.s.as_fixed() < TARGET_GRAY {
                return if border[LT_BLACK] > l && enabled[LT_BLACK] {
                    Some(LT_BLACK)
                } else {
                    None
                };
            }
            let h = hls.h.as_fixed() >> 16;
            let i = if h >= RANGE_RED2 || (h >= RANGE_RED0 && h < RANGE_RED1) {
                LT_RED
            } else if h >= RANGE_RED1 && h < TARGET_YELLOW1 {
                LT_YELLOW
            } else if h >= TARGET_YELLOW1 && h < TARGET_GREEN1 {
                LT_GREEN
            } else if h >= TARGET_GREEN1 && h < TARGET_BLUE1 {
                LT_BLUE
            } else if h >= TARGET_BLUE1 && h < RANGE_RED2 {
                LT_VIOLET
            } else {
                return None;
            };
            if border[i] > l && enabled[i] {
                Some(i)
            } else {
                None
            }
        };

        let cs8 = ae::pf::suites::ColorCallbacks::new()?;
        let cs16 = ae::pf::suites::ColorCallbacks16::new()?;
        let cs32 = ae::pf::suites::ColorCallbacksFloat::new()?;
        let effect_ref = in_data.effect_ref();

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
                        let m = MAX_CHANNEL8 as f32;
                        let white = ae::Pixel8 {
                            alpha: MAX_CHANNEL8 as u8,
                            red: MAX_CHANNEL8 as u8,
                            green: MAX_CHANNEL8 as u8,
                            blue: MAX_CHANNEL8 as u8,
                        };
                        let black = ae::Pixel8 {
                            alpha: MAX_CHANNEL8 as u8,
                            red: 0,
                            green: 0,
                            blue: 0,
                        };
                        let mut p = *out_pixel;
                        let op = (p.alpha as f32 / m).min(1.0);
                        if op <= 0.0 {
                            *out_pixel = white;
                        } else {
                            if op < 1.0 {
                                // 白地に合成する。
                                let op1 = 1.0 - op;
                                let f = |c: u8| round_byte_fp_long(c as f32 * op + m * op1);
                                // 元は blue を2回書いて red を書き忘れていた
                                // (blue にだけ合成が二重にかかる)。
                                p.red = f(p.red);
                                p.green = f(p.green);
                                p.blue = f(p.blue);
                            }
                            let hls = cs8.rgb_to_hls(effect_ref, &p)?;
                            *out_pixel = match pick(hls) {
                                None => white,
                                Some(LT_BLACK) => black,
                                Some(i) => col8[i],
                            };
                        }
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f32;
                        let white = ae::Pixel16 {
                            alpha: MAX_CHANNEL16 as u16,
                            red: MAX_CHANNEL16 as u16,
                            green: MAX_CHANNEL16 as u16,
                            blue: MAX_CHANNEL16 as u16,
                        };
                        let black = ae::Pixel16 {
                            alpha: MAX_CHANNEL16 as u16,
                            red: 0,
                            green: 0,
                            blue: 0,
                        };
                        let mut p = *out_pixel;
                        let op = (p.alpha as f32 / m).min(1.0);
                        if op <= 0.0 {
                            *out_pixel = white;
                        } else {
                            if op < 1.0 {
                                let op1 = 1.0 - op;
                                let f = |c: u16| round_short_fp_long(c as f32 * op + m * op1);
                                // 元は blue を2回書いて red を書き忘れていた
                                // (blue にだけ合成が二重にかかる)。
                                p.red = f(p.red);
                                p.green = f(p.green);
                                p.blue = f(p.blue);
                            }
                            let hls = cs16.rgb_to_hls(effect_ref, &p)?;
                            *out_pixel = match pick(hls) {
                                None => white,
                                Some(LT_BLACK) => black,
                                Some(i) => col16[i],
                            };
                        }
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        let white = ae::PixelF32 {
                            alpha: 1.0,
                            red: 1.0,
                            green: 1.0,
                            blue: 1.0,
                        };
                        let black = ae::PixelF32 {
                            alpha: 1.0,
                            red: 0.0,
                            green: 0.0,
                            blue: 0.0,
                        };
                        let mut p = *out_pixel;
                        let op = p.alpha.min(1.0);
                        if op <= 0.0 {
                            *out_pixel = white;
                        } else {
                            if op < 1.0 {
                                let op1 = 1.0 - op;
                                let f = |c: f32| round_fp_short(c * op + op1);
                                // 元は blue を2回書いて red を書き忘れていた
                                // (blue にだけ合成が二重にかかる)。
                                p.red = f(p.red);
                                p.green = f(p.green);
                                p.blue = f(p.blue);
                            }
                            let hls = cs32.rgb_to_hls(effect_ref, &p)?;
                            *out_pixel = match pick(hls) {
                                None => white,
                                Some(LT_BLACK) => black,
                                Some(i) => col32[i],
                            };
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
