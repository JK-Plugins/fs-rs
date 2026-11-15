//! F's PixelReplace の移植。
//!
//! 24 組の「対象色 -> 置換色」を上から順に試し、対象色に (許容値の範囲で)
//! 一致した画素を置換色で塗る。置換色のアルファは replaceOpacity から作る。
//! Disp はいくつ目までの組を使うかを決める。
//!
//! 16/32bit も元と同じく画素を 8bit に落としてから比較するので、許容値は
//! どの深度でも 8bit 単位。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/PixelReplace

use after_effects::{self as ae, MAX_CHANNEL8};

use libs::utils::{
    conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32, round_byte_fp_long, round_byte_long,
};

/// 元の PR_PRM_COUNT。
const COUNT: usize = 24;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Target1Start,
    Target1Enabled,
    Target1Src,
    Target1Dst,
    Target1Opacity,
    Target1End,
    Target2Start,
    Target2Enabled,
    Target2Src,
    Target2Dst,
    Target2Opacity,
    Target2End,
    Target3Start,
    Target3Enabled,
    Target3Src,
    Target3Dst,
    Target3Opacity,
    Target3End,
    Target4Start,
    Target4Enabled,
    Target4Src,
    Target4Dst,
    Target4Opacity,
    Target4End,
    Target5Start,
    Target5Enabled,
    Target5Src,
    Target5Dst,
    Target5Opacity,
    Target5End,
    Target6Start,
    Target6Enabled,
    Target6Src,
    Target6Dst,
    Target6Opacity,
    Target6End,
    Target7Start,
    Target7Enabled,
    Target7Src,
    Target7Dst,
    Target7Opacity,
    Target7End,
    Target8Start,
    Target8Enabled,
    Target8Src,
    Target8Dst,
    Target8Opacity,
    Target8End,
    Target9Start,
    Target9Enabled,
    Target9Src,
    Target9Dst,
    Target9Opacity,
    Target9End,
    Target10Start,
    Target10Enabled,
    Target10Src,
    Target10Dst,
    Target10Opacity,
    Target10End,
    Target11Start,
    Target11Enabled,
    Target11Src,
    Target11Dst,
    Target11Opacity,
    Target11End,
    Target12Start,
    Target12Enabled,
    Target12Src,
    Target12Dst,
    Target12Opacity,
    Target12End,
    Target13Start,
    Target13Enabled,
    Target13Src,
    Target13Dst,
    Target13Opacity,
    Target13End,
    Target14Start,
    Target14Enabled,
    Target14Src,
    Target14Dst,
    Target14Opacity,
    Target14End,
    Target15Start,
    Target15Enabled,
    Target15Src,
    Target15Dst,
    Target15Opacity,
    Target15End,
    Target16Start,
    Target16Enabled,
    Target16Src,
    Target16Dst,
    Target16Opacity,
    Target16End,
    Target17Start,
    Target17Enabled,
    Target17Src,
    Target17Dst,
    Target17Opacity,
    Target17End,
    Target18Start,
    Target18Enabled,
    Target18Src,
    Target18Dst,
    Target18Opacity,
    Target18End,
    Target19Start,
    Target19Enabled,
    Target19Src,
    Target19Dst,
    Target19Opacity,
    Target19End,
    Target20Start,
    Target20Enabled,
    Target20Src,
    Target20Dst,
    Target20Opacity,
    Target20End,
    Target21Start,
    Target21Enabled,
    Target21Src,
    Target21Dst,
    Target21Opacity,
    Target21End,
    Target22Start,
    Target22Enabled,
    Target22Src,
    Target22Dst,
    Target22Opacity,
    Target22End,
    Target23Start,
    Target23Enabled,
    Target23Src,
    Target23Dst,
    Target23Opacity,
    Target23End,
    Target24Start,
    Target24Enabled,
    Target24Src,
    Target24Dst,
    Target24Opacity,
    Target24End,
    Level,
    Disp,
}

/// スロット順は元の PARAMS_IDX どおり
/// (topic, Enabled, targetColor, replaceColor, replaceOpacity, topic_end)。
const TARGETS: [[Params; 6]; COUNT] = [
    [
        Params::Target1Start,
        Params::Target1Enabled,
        Params::Target1Src,
        Params::Target1Dst,
        Params::Target1Opacity,
        Params::Target1End,
    ],
    [
        Params::Target2Start,
        Params::Target2Enabled,
        Params::Target2Src,
        Params::Target2Dst,
        Params::Target2Opacity,
        Params::Target2End,
    ],
    [
        Params::Target3Start,
        Params::Target3Enabled,
        Params::Target3Src,
        Params::Target3Dst,
        Params::Target3Opacity,
        Params::Target3End,
    ],
    [
        Params::Target4Start,
        Params::Target4Enabled,
        Params::Target4Src,
        Params::Target4Dst,
        Params::Target4Opacity,
        Params::Target4End,
    ],
    [
        Params::Target5Start,
        Params::Target5Enabled,
        Params::Target5Src,
        Params::Target5Dst,
        Params::Target5Opacity,
        Params::Target5End,
    ],
    [
        Params::Target6Start,
        Params::Target6Enabled,
        Params::Target6Src,
        Params::Target6Dst,
        Params::Target6Opacity,
        Params::Target6End,
    ],
    [
        Params::Target7Start,
        Params::Target7Enabled,
        Params::Target7Src,
        Params::Target7Dst,
        Params::Target7Opacity,
        Params::Target7End,
    ],
    [
        Params::Target8Start,
        Params::Target8Enabled,
        Params::Target8Src,
        Params::Target8Dst,
        Params::Target8Opacity,
        Params::Target8End,
    ],
    [
        Params::Target9Start,
        Params::Target9Enabled,
        Params::Target9Src,
        Params::Target9Dst,
        Params::Target9Opacity,
        Params::Target9End,
    ],
    [
        Params::Target10Start,
        Params::Target10Enabled,
        Params::Target10Src,
        Params::Target10Dst,
        Params::Target10Opacity,
        Params::Target10End,
    ],
    [
        Params::Target11Start,
        Params::Target11Enabled,
        Params::Target11Src,
        Params::Target11Dst,
        Params::Target11Opacity,
        Params::Target11End,
    ],
    [
        Params::Target12Start,
        Params::Target12Enabled,
        Params::Target12Src,
        Params::Target12Dst,
        Params::Target12Opacity,
        Params::Target12End,
    ],
    [
        Params::Target13Start,
        Params::Target13Enabled,
        Params::Target13Src,
        Params::Target13Dst,
        Params::Target13Opacity,
        Params::Target13End,
    ],
    [
        Params::Target14Start,
        Params::Target14Enabled,
        Params::Target14Src,
        Params::Target14Dst,
        Params::Target14Opacity,
        Params::Target14End,
    ],
    [
        Params::Target15Start,
        Params::Target15Enabled,
        Params::Target15Src,
        Params::Target15Dst,
        Params::Target15Opacity,
        Params::Target15End,
    ],
    [
        Params::Target16Start,
        Params::Target16Enabled,
        Params::Target16Src,
        Params::Target16Dst,
        Params::Target16Opacity,
        Params::Target16End,
    ],
    [
        Params::Target17Start,
        Params::Target17Enabled,
        Params::Target17Src,
        Params::Target17Dst,
        Params::Target17Opacity,
        Params::Target17End,
    ],
    [
        Params::Target18Start,
        Params::Target18Enabled,
        Params::Target18Src,
        Params::Target18Dst,
        Params::Target18Opacity,
        Params::Target18End,
    ],
    [
        Params::Target19Start,
        Params::Target19Enabled,
        Params::Target19Src,
        Params::Target19Dst,
        Params::Target19Opacity,
        Params::Target19End,
    ],
    [
        Params::Target20Start,
        Params::Target20Enabled,
        Params::Target20Src,
        Params::Target20Dst,
        Params::Target20Opacity,
        Params::Target20End,
    ],
    [
        Params::Target21Start,
        Params::Target21Enabled,
        Params::Target21Src,
        Params::Target21Dst,
        Params::Target21Opacity,
        Params::Target21End,
    ],
    [
        Params::Target22Start,
        Params::Target22Enabled,
        Params::Target22Src,
        Params::Target22Dst,
        Params::Target22Opacity,
        Params::Target22End,
    ],
    [
        Params::Target23Start,
        Params::Target23Enabled,
        Params::Target23Src,
        Params::Target23Dst,
        Params::Target23Opacity,
        Params::Target23End,
    ],
    [
        Params::Target24Start,
        Params::Target24Enabled,
        Params::Target24Src,
        Params::Target24Dst,
        Params::Target24Opacity,
        Params::Target24End,
    ],
];

/// 元の PixelReplace_def.h の pr_colorDef[16][6]。前3列が targetColor、
/// 後3列が replaceColor。後半8行は PF_HALF_CHAN8 (128) で、最後の行は
/// PF_HALF_CHAN8/2 (64)。
const COLOR_DEF: [[u8; 6]; 16] = [
    [255, 0, 0, 255, 0, 0],
    [0, 255, 0, 0, 255, 0],
    [0, 0, 255, 0, 0, 255],
    [255, 255, 0, 255, 255, 0],
    [255, 0, 255, 255, 0, 255],
    [0, 255, 255, 0, 255, 255],
    [255, 255, 255, 255, 255, 255],
    [0, 0, 0, 0, 0, 0],
    [128, 0, 0, 128, 0, 0],
    [0, 128, 0, 0, 128, 0],
    [0, 0, 128, 0, 0, 128],
    [128, 128, 0, 128, 128, 0],
    [128, 0, 128, 128, 0, 128],
    [0, 128, 128, 0, 128, 128],
    [128, 128, 128, 128, 128, 128],
    [64, 64, 64, 64, 64, 64],
];

const DISP_OPTIONS: [&str; COUNT] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17",
    "18", "19", "20", "21", "22", "23", "24",
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
        // target1..24。元は PF_ParamFlag_START_COLLAPSED なので折りたたんで出す。
        for (i, t) in TARGETS.iter().enumerate() {
            let c = COLOR_DEF[i % COLOR_DEF.len()];
            params.add_group(t[0], t[5], &format!("target{}", i + 1), true, |params| {
                params.add(
                    t[1],
                    "Enabled",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_label("ON");
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                params.add(
                    t[2],
                    "targetColor",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 255,
                            red: c[0],
                            green: c[1],
                            blue: c[2],
                        });
                        f.set_value(f.default());
                    }),
                )?;
                params.add(
                    t[3],
                    "replaceColor",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 255,
                            red: c[3],
                            green: c[4],
                            blue: c[5],
                        });
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_PERCENT(UI_REP_OPACITY, 100, ..)
                // PF_ADD_PERCENT は 0..100 の FIX_SLIDER。リポジトリの他と
                // 揃えて float スライダーにしている。
                params.add(
                    t[4],
                    "replaceOpacity",
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
                Ok(())
            })?;
        }

        // 元: PF_ADD_FLOAT_SLIDER(UI_LV, 0, 100, 0, 100, 0, 1, 1, 0, 0, ..)
        // 元の名前は "lebel" (level の綴り間違い)。元の .aep から値を
        // 引き継ぐのは matchName とスロット順なので、名前は直してある。
        params.add(
            Params::Level,
            "level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_POPUP(UI_POP1, 24, 24, "1|2|...|24", ..)
        // 元は SUPERVISE で target の表示数を切り替えるが、ここではスロットだけ
        // 合わせてあり、表示の出し入れは未実装 (pixelselector と同じ)。
        params.add_with_flags(
            Params::Disp,
            "Disp",
            ae::PopupDef::setup(|f| {
                f.set_options(&DISP_OPTIONS);
                f.set_default(COUNT as i32);
                f.set_value(f.default());
            }),
            ae::ParamFlag::CANNOT_TIME_VARY | ae::ParamFlag::CANNOT_INTERP,
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
        out_data.set_return_msg("fs-rs pixelreplace");
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
        // 元の GetParams: Disp までの組だけ見て、Enabled が入っているものを詰める。
        let disp = params.get(Params::Disp)?.as_popup()?.value() as usize;
        // 元: Lv = RoundByteFpLong(PF_MAX_CHAN8 * d/100)
        let lv = round_byte_fp_long(
            MAX_CHANNEL8 as f32 * params.get(Params::Level)?.as_float_slider()?.value() as f32
                / 100.0,
        ) as i32;

        let mut pairs: Vec<(ae::Pixel8, ae::Pixel8)> = Vec::with_capacity(COUNT);
        for t in TARGETS.iter().take(disp.min(COUNT)) {
            if !params.get(t[1])?.as_checkbox()?.value() {
                continue;
            }
            let src = params.get(t[2])?.as_color()?.value();
            let mut dst = params.get(t[3])?.as_color()?.value();
            // 元は PF_Fixed を 100 で割ってから 255 を掛け 1<<16 で割る。
            // 整数演算なので途中で切り捨てが入る。
            let percent = params.get(t[4])?.as_float_slider()?.value();
            let fixed = (percent * 65536.0) as i32 / 100;
            dst.alpha = round_byte_long(MAX_CHANNEL8 as i32 * fixed / (1 << 16));
            pairs.push((src, dst));
        }

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        if pairs.is_empty() {
            return Ok(());
        }

        // 元の compPix*Lv。どの深度でも 8bit に落として比べる。
        let hit = |p: &ae::Pixel8, s: &ae::Pixel8| {
            (p.blue as i32 - s.blue as i32).abs() <= lv
                && (p.green as i32 - s.green as i32).abs() <= lv
                && (p.red as i32 - s.red as i32).abs() <= lv
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
                    (ae::GenericPixel::Pixel8(_), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        // 元は一致しても break せず、最後に一致した組で上書きする。
                        for (s, d) in pairs.iter() {
                            if hit(out_pixel, s) {
                                *out_pixel = *d;
                            }
                        }
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let c = conv_16_to_8(out_pixel);
                        for (s, d) in pairs.iter() {
                            if hit(&c, s) {
                                *out_pixel = conv_8_to_16(d);
                            }
                        }
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        let c = conv_32_to_8(out_pixel);
                        for (s, d) in pairs.iter() {
                            if hit(&c, s) {
                                *out_pixel = conv_8_to_32(d);
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
