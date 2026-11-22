//! F's grayToColorize の移植。
//!
//! 輝度を 5 段に落として、その段に対応する色で塗る (明るいほど Color1)。
//! アルファはしきい値で 0 か最大に二値化する。Mat を入れると、先に入力を
//! Mat 色の上に合成してから輝度を取る。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/grayToColorize

use after_effects::{self as ae, HALF_CHANNEL16, HALF_CHANNEL8, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{
    conv_8_to_16, conv_8_to_32, round_byte_fp_long, round_byte_long, round_fp_short, round_short,
    round_short_fp_long,
};

/// 元の SAMPLE_MAX。段数 - 1。
const SAMPLE_MAX: i32 = 4;
/// 元の TBL_COUNT。色の数。
const TBL_COUNT: usize = 5;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    AlphaThreshold,
    Color1,
    Color2,
    Color3,
    Color4,
    Color5,
    Mat,
    MatColor,
}

const COLOR_PARAMS: [Params; TBL_COUNT] = [
    Params::Color1,
    Params::Color2,
    Params::Color3,
    Params::Color4,
    Params::Color5,
];
const COLOR_NAMES: [&str; TBL_COUNT] = ["Color1", "Color2", "Color3", "Color4", "Color5"];

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
        // 元: PF_ADD_FLOAT_SLIDER(STR_ALPHA_THRESHOLD, 0, 100, 0, 100, 1, 25, 1, 0, 0, ..)
        params.add(
            Params::AlphaThreshold,
            "AlphaThreshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(25.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_COLn, ..) を5色。
        const COLS: [[u8; 3]; TBL_COUNT] = [
            [0xFF, 0xF9, 0x9C],
            [0xF5, 0xDD, 0x6D],
            [0xF3, 0xA4, 0x47],
            [0xDB, 0x90, 0x38],
            [0xF3, 0xC1, 0x86],
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

        // 元: PF_ADD_CHECKBOX(STR_MAT_CB, STR_ON, FALSE, 0, ID_MAT_CB)
        // フラグは PF_ParamFlag_CANNOT_TIME_VARY。
        params.add_with_flags(
            Params::Mat,
            "Mat",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
            ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::NONE,
        )?;

        // 元: PF_ADD_COLOR(STR_MAT_COL, 0xFF, 0xFF, 0xFF, ID_MAT_COLOR)
        params.add_with_flags(
            Params::MatColor,
            "MatColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0xFF,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
            ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::NONE,
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
        out_data.set_return_msg("fs-rs graytocolorize");
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
        // 元の GetParams: しきい値は /100 して深度ごとの最大値に掛ける。
        let level = params
            .get(Params::AlphaThreshold)?
            .as_float_slider()?
            .value() as f32
            / 100.0;
        let border8 = round_byte_fp_long(MAX_CHANNEL8 as f32 * level);
        let border16 = round_short_fp_long(MAX_CHANNEL16 as f32 * level);
        let border32 = round_fp_short(level);

        let mut col8 = [ae::Pixel8 {
            alpha: 0,
            red: 0,
            green: 0,
            blue: 0,
        }; TBL_COUNT];
        for i in 0..TBL_COUNT {
            col8[i] = params.get(COLOR_PARAMS[i])?.as_color()?.value();
        }
        let col16: [ae::Pixel16; TBL_COUNT] = std::array::from_fn(|i| conv_8_to_16(&col8[i]));
        let col32: [ae::PixelF32; TBL_COUNT] = std::array::from_fn(|i| conv_8_to_32(&col8[i]));

        let mat_on = params.get(Params::Mat)?.as_checkbox()?.value();
        let mat8 = params.get(Params::MatColor)?.as_color()?.value();
        let mat16 = conv_8_to_16(&mat8);
        let mat32 = conv_8_to_32(&mat8);

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
                        let m = MAX_CHANNEL8 as i32;
                        let (mut r, mut g, mut b) =
                            (pixel.red as i32, pixel.green as i32, pixel.blue as i32);
                        if mat_on {
                            // 元: alpha*(c - mat)/max + mat (整数演算、0方向に切り捨て)。
                            let a = pixel.alpha as i32;
                            let mix =
                                |c: i32, mc: i32| round_byte_long(a * (c - mc) / m + mc) as i32;
                            r = mix(r, mat8.red as i32);
                            g = mix(g, mat8.green as i32);
                            b = mix(b, mat8.blue as i32);
                        }
                        let a = if pixel.alpha > border8 {
                            MAX_CHANNEL8 as u8
                        } else {
                            0
                        };
                        let v = (0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64 + 0.5)
                            as i32;
                        // 元の FROM8_SAMPLE。整数演算。
                        let v = (v * SAMPLE_MAX + HALF_CHANNEL8 as i32) / m;
                        let v = SAMPLE_MAX - v.clamp(0, SAMPLE_MAX);
                        *out_pixel = col8[v as usize];
                        out_pixel.alpha = a;
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as i32;
                        let (mut r, mut g, mut b) =
                            (pixel.red as i32, pixel.green as i32, pixel.blue as i32);
                        if mat_on {
                            // 元はここで RoundByteLong を使っていて、16bit の
                            // チャンネルが 255 で止まってしまう (Mat を入れると
                            // 画がほぼ黒くなる)。深度に合う RoundShort にした。
                            let a = pixel.alpha as i32;
                            let mix = |c: i32, mc: i32| round_short(a * (c - mc) / m + mc) as i32;
                            r = mix(r, mat16.red as i32);
                            g = mix(g, mat16.green as i32);
                            b = mix(b, mat16.blue as i32);
                        }
                        let a = if pixel.alpha > border16 {
                            MAX_CHANNEL16 as u16
                        } else {
                            0
                        };
                        let v = (0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64 + 0.5)
                            as i32;
                        // 元の FROM16_SAMPLE。
                        let v = (v * SAMPLE_MAX + HALF_CHANNEL16 as i32) / m;
                        let v = SAMPLE_MAX - v.clamp(0, SAMPLE_MAX);
                        *out_pixel = col16[v as usize];
                        out_pixel.alpha = a;
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let (mut r, mut g, mut b) = (pixel.red, pixel.green, pixel.blue);
                        if mat_on {
                            let a = pixel.alpha;
                            r = round_fp_short(a * (r - mat32.red) + mat32.red);
                            g = round_fp_short(a * (g - mat32.green) + mat32.green);
                            // 元は `/ +` のタイプミスで、足し算ではなく m.blue で
                            // 割っていた (R/G と違う式になる)。足し算に直した。
                            b = round_fp_short(a * (b - mat32.blue) + mat32.blue);
                        }
                        let a = if pixel.alpha > border32 { 1.0 } else { 0.0 };
                        let v = 0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64;
                        // 元の FROM32_SAMPLE は A_u_long へのキャスト。
                        let v = (v * SAMPLE_MAX as f64 + 0.5) as i64 as i32;
                        let v = SAMPLE_MAX - v.clamp(0, SAMPLE_MAX);
                        *out_pixel = col32[v as usize];
                        out_pixel.alpha = a;
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
