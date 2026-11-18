//! F's SelectColor の移植。
//!
//! 8組の「対象色」のどれかに (許容値の範囲で) 一致した画素を透明にする。
//! Rev を入れると一致しなかった画素を透明にする。
//! 透明にした画素の RGB は白で埋める。
//!
//! 16/32bit も元と同じく画素を 8bit に落としてから比較するので、
//! 許容値はどの深度でも 8bit 単位。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/SelectColor

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_16_to_8, conv_32_to_8};

/// 元の SelectColorCount。
const COUNT: usize = 8;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Target0,
    Color0,
    Target1,
    Color1,
    Target2,
    Color2,
    Target3,
    Color3,
    Target4,
    Color4,
    Target5,
    Color5,
    Target6,
    Color6,
    Target7,
    Color7,
    Rev,
    Level,
}

/// パラメータのスロット順は元の enum どおり (Target と色が交互)。
const TARGETS: [Params; COUNT] = [
    Params::Target0,
    Params::Target1,
    Params::Target2,
    Params::Target3,
    Params::Target4,
    Params::Target5,
    Params::Target6,
    Params::Target7,
];
const COLORS: [Params; COUNT] = [
    Params::Color0,
    Params::Color1,
    Params::Color2,
    Params::Color3,
    Params::Color4,
    Params::Color5,
    Params::Color6,
    Params::Color7,
];
const TARGET_NAMES: [&str; COUNT] = [
    "Target0", "Target1", "Target2", "Target3", "Target4", "Target5", "Target6", "Target7",
];
const COLOR_NAMES: [&str; COUNT] = [
    "color0", "color1", "color2", "color3", "color4", "color5", "color6", "color7",
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
        // 元: PF_ADD_CHECKBOX("TargetN","実行する",FALSE,0,..) と
        //     PF_ADD_COLOR("colorN", r,g,b, ..) が交互に8組。
        //
        // 元のラベル "実行する" / "反転する" / "許容値" と、7組目だけ名前が
        // "元の色7" になっているのは AE が非 ASCII をラベル空で描くため
        // 英語にしてある。
        const DEFAULTS: [[u8; 3]; COUNT] = [
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [0, 255, 255],
            [255, 0, 255],
            [255, 255, 0],
            [255, 255, 255],
            [0, 0, 0],
        ];
        for (i, rgb) in DEFAULTS.iter().enumerate() {
            params.add(
                TARGETS[i],
                TARGET_NAMES[i],
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("ON");
                    f.set_default(false);
                    f.set_value(f.default());
                }),
            )?;
            params.add(
                COLORS[i],
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

        // 元: PF_ADD_CHECKBOX("Rev","反転する",FALSE,0,ID_REV)
        params.add(
            Params::Rev,
            "Rev",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("invert");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_SLIDER("許容値", 0, PF_MAX_CHAN8, 0, 15, 0, ID_LEVEL)
        // 色の差が1変わるので 0..255 の整数のまま出す。
        params.add(
            Params::Level,
            "Tolerance",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(MAX_CHANNEL8 as i32);
                f.set_slider_min(0);
                f.set_slider_max(15);
                f.set_default(0);
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
        out_data.set_return_msg("fs-rs selectcolor");
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
        // 元の GetParams: チェックが入っている組の色だけを詰めて持つ。
        let mut src: Vec<ae::Pixel8> = Vec::with_capacity(COUNT);
        for i in 0..COUNT {
            if params.get(TARGETS[i])?.as_checkbox()?.value() {
                src.push(params.get(COLORS[i])?.as_color()?.value());
            }
        }
        let rev = params.get(Params::Rev)?.as_checkbox()?.value();
        let lv = params.get(Params::Level)?.as_slider()?.value();

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: 有効な色が1つも無ければ素通し。
        if src.is_empty() {
            return Ok(());
        }

        // 元は許容値 0 のときだけ等値比較の専用関数に切り替えるが、
        // ABS(diff) <= 0 は等値比較と同じなので1本にまとめてある。
        let hits = |p: &ae::Pixel8| {
            src.iter().any(|c| {
                (p.red as i32 - c.red as i32).abs() <= lv
                    && (p.green as i32 - c.green as i32).abs() <= lv
                    && (p.blue as i32 - c.blue as i32).abs() <= lv
            })
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
                        if hits(out_pixel) == rev {
                            out_pixel.alpha = 0;
                            out_pixel.red = MAX_CHANNEL8 as u8;
                            out_pixel.green = MAX_CHANNEL8 as u8;
                            out_pixel.blue = MAX_CHANNEL8 as u8;
                        }
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        // 元は CONV16TO8 で 8bit に落としてから比較する。
                        if hits(&conv_16_to_8(out_pixel)) == rev {
                            out_pixel.alpha = 0;
                            out_pixel.red = MAX_CHANNEL16 as u16;
                            out_pixel.green = MAX_CHANNEL16 as u16;
                            out_pixel.blue = MAX_CHANNEL16 as u16;
                        }
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        if hits(&conv_32_to_8(out_pixel)) == rev {
                            out_pixel.alpha = 0.0;
                            out_pixel.red = 1.0;
                            out_pixel.green = 1.0;
                            out_pixel.blue = 1.0;
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
