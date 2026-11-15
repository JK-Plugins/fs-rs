use after_effects::{self as ae};

use libs::utils::{
    comp_pix8_lv, conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32, round_byte_fp_long,
};

/// 元の F's PixelSelector のスロット構成に合わせてある。
///
/// Option グループ (反転 / 塗りつぶし / fillColor / fillColor_opacity) のあと
/// target1..24 の 24 組、最後に level と Disp。元の PixelSelector.h の
///
///     #define PS_PRM_COUNT 24
///     enum { paramsOffset_topic, paramsOffset_enabled, paramsOffset_s_color,
///            paramsOffset_topic_end, paramsOffset_num };
///
/// に対応する並びで、グループ開始/終了を含めて全104スロットになる。matchName を
/// 元と共有しているので AE は保存済みの値をスロット順で復元する。並びを変えると
/// 既存プロジェクトの値が別のパラメータに入る。
#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    OptionsStart,
    OptionReverse,
    OptionFill,
    OptionFillColor,
    OptionFillOpacity,
    OptionsEnd,
    Target1Start, Target1Enabled, Target1Color, Target1End,
    Target2Start, Target2Enabled, Target2Color, Target2End,
    Target3Start, Target3Enabled, Target3Color, Target3End,
    Target4Start, Target4Enabled, Target4Color, Target4End,
    Target5Start, Target5Enabled, Target5Color, Target5End,
    Target6Start, Target6Enabled, Target6Color, Target6End,
    Target7Start, Target7Enabled, Target7Color, Target7End,
    Target8Start, Target8Enabled, Target8Color, Target8End,
    Target9Start, Target9Enabled, Target9Color, Target9End,
    Target10Start, Target10Enabled, Target10Color, Target10End,
    Target11Start, Target11Enabled, Target11Color, Target11End,
    Target12Start, Target12Enabled, Target12Color, Target12End,
    Target13Start, Target13Enabled, Target13Color, Target13End,
    Target14Start, Target14Enabled, Target14Color, Target14End,
    Target15Start, Target15Enabled, Target15Color, Target15End,
    Target16Start, Target16Enabled, Target16Color, Target16End,
    Target17Start, Target17Enabled, Target17Color, Target17End,
    Target18Start, Target18Enabled, Target18Color, Target18End,
    Target19Start, Target19Enabled, Target19Color, Target19End,
    Target20Start, Target20Enabled, Target20Color, Target20End,
    Target21Start, Target21Enabled, Target21Color, Target21End,
    Target22Start, Target22Enabled, Target22Color, Target22End,
    Target23Start, Target23Enabled, Target23Color, Target23End,
    Target24Start, Target24Enabled, Target24Color, Target24End,
    Level,
    Disp,
}

const TARGET_COUNT: usize = 24;

/// [group_start, enabled, color, group_end] を target1..24 の順に並べたもの。
const TARGETS: [[Params; 4]; TARGET_COUNT] = [
    [Params::Target1Start, Params::Target1Enabled, Params::Target1Color, Params::Target1End],
    [Params::Target2Start, Params::Target2Enabled, Params::Target2Color, Params::Target2End],
    [Params::Target3Start, Params::Target3Enabled, Params::Target3Color, Params::Target3End],
    [Params::Target4Start, Params::Target4Enabled, Params::Target4Color, Params::Target4End],
    [Params::Target5Start, Params::Target5Enabled, Params::Target5Color, Params::Target5End],
    [Params::Target6Start, Params::Target6Enabled, Params::Target6Color, Params::Target6End],
    [Params::Target7Start, Params::Target7Enabled, Params::Target7Color, Params::Target7End],
    [Params::Target8Start, Params::Target8Enabled, Params::Target8Color, Params::Target8End],
    [Params::Target9Start, Params::Target9Enabled, Params::Target9Color, Params::Target9End],
    [Params::Target10Start, Params::Target10Enabled, Params::Target10Color, Params::Target10End],
    [Params::Target11Start, Params::Target11Enabled, Params::Target11Color, Params::Target11End],
    [Params::Target12Start, Params::Target12Enabled, Params::Target12Color, Params::Target12End],
    [Params::Target13Start, Params::Target13Enabled, Params::Target13Color, Params::Target13End],
    [Params::Target14Start, Params::Target14Enabled, Params::Target14Color, Params::Target14End],
    [Params::Target15Start, Params::Target15Enabled, Params::Target15Color, Params::Target15End],
    [Params::Target16Start, Params::Target16Enabled, Params::Target16Color, Params::Target16End],
    [Params::Target17Start, Params::Target17Enabled, Params::Target17Color, Params::Target17End],
    [Params::Target18Start, Params::Target18Enabled, Params::Target18Color, Params::Target18End],
    [Params::Target19Start, Params::Target19Enabled, Params::Target19Color, Params::Target19End],
    [Params::Target20Start, Params::Target20Enabled, Params::Target20Color, Params::Target20End],
    [Params::Target21Start, Params::Target21Enabled, Params::Target21Color, Params::Target21End],
    [Params::Target22Start, Params::Target22Enabled, Params::Target22Color, Params::Target22End],
    [Params::Target23Start, Params::Target23Enabled, Params::Target23Color, Params::Target23End],
    [Params::Target24Start, Params::Target24Enabled, Params::Target24Color, Params::Target24End],
];

/// targetColor の既定色。元の PixelSelector_def.h の pr_colorDef を i % 16 で
/// 引いているので、17個目以降は先頭から繰り返す。
const COLOR_DEF: [[u8; 3]; 16] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
    [0, 0, 0],
    [8, 0, 0],
    [0, 8, 0],
    [0, 0, 8],
    [8, 8, 0],
    [8, 0, 8],
    [0, 8, 8],
    [8, 8, 8],
    [2, 2, 2],
];

const DISP_OPTIONS: [&str; TARGET_COUNT] = [
    "1", "2", "3", "4", "5", "6", "7", "8",
    "9", "10", "11", "12", "13", "14", "15", "16",
    "17", "18", "19", "20", "21", "22", "23", "24",
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
        in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        params.add_group(
            Params::OptionsStart,
            Params::OptionsEnd,
            "Option",
            false,
            |params| {
                // 元のラベルは "反転する"。PF のパラメータ名は ASCII でないと
                // AE 側で空文字になってしまうので英語にしている (元も PiPL 経由で
                // 文字化けしている)。
                params.add(
                    Params::OptionReverse,
                    "Reverse",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                // 元のラベルは "塗りつぶす"。
                params.add(
                    Params::OptionFill,
                    "Fill",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                params.add(
                    Params::OptionFillColor,
                    "fillColor",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: 255,
                            green: 255,
                            blue: 255,
                            alpha: 255,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                params.add(
                    Params::OptionFillOpacity,
                    "fillColor_opacity",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(100.0);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            },
        )?;

        // target1..24。元は PF_ParamFlag_START_COLLAPSED なので折りたたんで出す。
        for (i, t) in TARGETS.iter().enumerate() {
            let rgb = COLOR_DEF[i % COLOR_DEF.len()];
            params.add_group(t[0], t[3], &format!("target{}", i + 1), true, |params| {
                params.add(
                    t[1],
                    "Enabled",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                params.add(
                    t[2],
                    "targetColor",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            red: rgb[0],
                            green: rgb[1],
                            blue: rgb[2],
                            alpha: 255,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            })?;
        }

        params.add(
            Params::Level,
            "level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;

        // 元は表示する target の数を切り替える supervise 付きポップアップ。
        // ここではスロットだけ合わせてあり、表示の出し入れは未実装。
        params.add(
            Params::Disp,
            "Disp",
            ae::PopupDef::setup(|f| {
                f.set_options(&DISP_OPTIONS);
                f.set_default(TARGET_COUNT as i32);
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
        out_data
            .set_return_msg("fs-rs pixelselector");
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

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let reverse = params.get(Params::OptionReverse)?.as_checkbox()?.value();
        let fill = params.get(Params::OptionFill)?.as_checkbox()?.value();
        let fill_color = params.get(Params::OptionFillColor)?.as_color()?.value();
        let fill_opacity = params.get(Params::OptionFillOpacity)?.as_float_slider()?.value();
        let level = params.get(Params::Level)?.as_float_slider()?.value();

        // 元: infoP->lv = RoundByteFpLong(PF_MAX_CHAN8 * (d/100))
        // level はパーセントで、8bit の許容差に換算して使う。
        let lv = round_byte_fp_long(MAX_CHANNEL8 as f32 * (level as f32 / 100.0));

        // 塗りつぶし色のアルファは fillColor_opacity から作る。
        //
        // 元実装は col16 / col32 を opacity を反映する前の col から作っているため、
        // fillColor_opacity が 8bit のときだけ効いて 16/32bit では無視される。
        // 深度で結果が変わる明らかな不整合なので、ここでは全深度で反映する。
        let mut fill_pixel = fill_color;
        fill_pixel.alpha = round_byte_fp_long(MAX_CHANNEL8 as f32 * (fill_opacity as f32 / 100.0));

        // 元: Enabled が TRUE の target だけを順に集める (Disp とは無関係)。
        let mut targets: Vec<ae::Pixel8> = Vec::with_capacity(TARGET_COUNT);
        for t in TARGETS.iter() {
            if params.get(t[1])?.as_checkbox()?.value() {
                targets.push(params.get(t[2])?.as_color()?.value());
            }
        }

        let progress_final = out_layer.height() as _;
        ae::pf::suites::WorldTransform::new()?.copy_hq(in_data.effect_ref(), &in_layer, &mut out_layer, None, None)?;

        // 元 Exec: count が 0 なら CopyInToOut だけで iterate しない。
        if targets.is_empty() {
            return Ok(());
        }
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
                        let b = targets.iter().any(|c| comp_pix8_lv(c, pixel, lv));
                        if b == reverse {
                            out_pixel.alpha = 0;
                            out_pixel.red = MAX_CHANNEL8 as u8;
                            out_pixel.green = MAX_CHANNEL8 as u8;
                            out_pixel.blue = MAX_CHANNEL8 as u8;
                        } else if fill {
                            out_pixel.alpha = fill_pixel.alpha;
                            out_pixel.red = fill_pixel.red;
                            out_pixel.green = fill_pixel.green;
                            out_pixel.blue = fill_pixel.blue;
                        }
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let s = conv_16_to_8(pixel);
                        let b = targets.iter().any(|c| comp_pix8_lv(c, &s, lv));
                        if b == reverse {
                            out_pixel.alpha = 0;
                            out_pixel.red = MAX_CHANNEL16 as u16;
                            out_pixel.green = MAX_CHANNEL16 as u16;
                            out_pixel.blue = MAX_CHANNEL16 as u16;
                        } else if fill {
                            let c = conv_8_to_16(&fill_pixel);
                            out_pixel.alpha = c.alpha;
                            out_pixel.red = c.red;
                            out_pixel.green = c.green;
                            out_pixel.blue = c.blue;
                        }
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let s = conv_32_to_8(pixel);
                        let b = targets.iter().any(|c| comp_pix8_lv(c, &s, lv));
                        if b == reverse {
                            out_pixel.alpha = 0.0;
                            out_pixel.red = 1.0;
                            out_pixel.green = 1.0;
                            out_pixel.blue = 1.0;
                        } else if fill {
                            let c = conv_8_to_32(&fill_pixel);
                            out_pixel.alpha = c.alpha;
                            out_pixel.red = c.red;
                            out_pixel.green = c.green;
                            out_pixel.blue = c.blue;
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
