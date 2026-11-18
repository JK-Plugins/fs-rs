//! F's TargetGradRadical の移植。
//!
//! 絵の中から対象色 (または不透明な画素) を拾い、center を中心にした
//! 楕円状のグラデーションをアルファに描く。RGB は gradColor。
//! radius が半径、aspect が縦横比、angle が傾き、feather がぼけ幅、
//! hyperbolic が曲がり具合。
//!
//! TargetGrad との違い:
//! - グラデーションが直線ではなく中心からの距離で決まる
//! - 外接矩形を使わない (元も SearchExtractArea を呼ばない)
//! - ガイド描画がない
//!
//! 元の mask_val は `if (invert) 1-mask; if (!invert) 1-mask;` と書かれていて、
//! どちらの枝でも1回反転する。つまり invert に関係なく必ず反転するので、
//! ここでは1回の反転にまとめてある (挙動は同じ)。invert は距離 t の側に
//! かかる。
//!
//! 既定値は元が .pref から読むので、出荷 .aex が --inspect-experimental で
//! 報告する値に合わせてある。
//!
//! 未実装: colorTable のロード / セーブボタン (TargetGrad と同じ)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/TargetGradRadical

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32};

/// 元の COLOR_TABLE_COUNT。
const COLOR_COUNT: usize = 8;

const MODE_COLORS: i32 = 1;
const MODE_ALL: i32 = 3;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Target,
    ColorsStart,
    Enabled1,
    Color1,
    Enabled2,
    Color2,
    Enabled3,
    Color3,
    Enabled4,
    Color4,
    Enabled5,
    Color5,
    Enabled6,
    Color6,
    Enabled7,
    Color7,
    Enabled8,
    Color8,
    ColorsEnd,
    GradColor,
    Invert,
    Center,
    Radius,
    Feather,
    Hyperbolic,
    Aspect,
    Angle,
    LoadButton,
    SaveButton,
}

const ENABLED: [Params; COLOR_COUNT] = [
    Params::Enabled1,
    Params::Enabled2,
    Params::Enabled3,
    Params::Enabled4,
    Params::Enabled5,
    Params::Enabled6,
    Params::Enabled7,
    Params::Enabled8,
];
const COLORS: [Params; COLOR_COUNT] = [
    Params::Color1,
    Params::Color2,
    Params::Color3,
    Params::Color4,
    Params::Color5,
    Params::Color6,
    Params::Color7,
    Params::Color8,
];
/// 出荷 .aex が報告する target1..8 の既定色 (R,G,B,Y,M,C,白,黒)。
const TARGET_DEFAULTS: [[u8; 3]; COLOR_COUNT] = [
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
    [0, 0, 0],
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
        params.add(
            Params::Target,
            "target",
            ae::PopupDef::setup(|f| {
                f.set_options(&["targetColors", "alphaOn", "all"]);
                f.set_default(MODE_ALL);
                f.set_value(f.default());
            }),
        )?;

        params.add_group(
            Params::ColorsStart,
            Params::ColorsEnd,
            "targetColors",
            true,
            |params| {
                for i in 0..COLOR_COUNT {
                    let rgb = TARGET_DEFAULTS[i];
                    params.add(
                        ENABLED[i],
                        &format!("targetEnabled{}", i + 1),
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    params.add(
                        COLORS[i],
                        &format!("target{}", i + 1),
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
            },
        )?;

        params.add(
            Params::GradColor,
            "gradColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0x00,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::Invert,
            "invert",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::Center,
            "center",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;

        // 出荷 .aex の範囲と既定値に合わせてある。
        for (p, name, vmin, vmax, smin, smax, dflt, prec) in [
            (Params::Radius, "radius", 0.0, 4000.0, 0.0, 1000.0, 500.0, 1),
            (
                Params::Feather,
                "feather",
                0.02,
                100.0,
                0.02,
                100.0,
                100.0,
                1,
            ),
            (
                Params::Hyperbolic,
                "hyperbolic",
                0.0,
                50.0,
                0.0,
                20.0,
                5.0,
                1,
            ),
            (Params::Aspect, "aspect", 0.0, 2000.0, 0.0, 200.0, 100.0, 1),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(vmin);
                    f.set_valid_max(vmax);
                    f.set_slider_min(smin);
                    f.set_slider_max(smax);
                    f.set_precision(prec);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }

        params.add(
            Params::Angle,
            "angle",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;

        for (p, name, label) in [
            (Params::LoadButton, "colorTable", "load"),
            (Params::SaveButton, "", "save"),
        ] {
            params.add(
                p,
                name,
                ae::ButtonDef::setup(|f| {
                    f.set_label(label);
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
                out_data.set_return_msg("fs-rs targetgradradical");
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
        let mode = params.get(Params::Target)?.as_popup()?.value();

        let mut targets: Vec<ae::Pixel8> = Vec::with_capacity(COLOR_COUNT);
        for i in 0..COLOR_COUNT {
            if !params.get(ENABLED[i])?.as_checkbox()?.value() {
                continue;
            }
            let c = params.get(COLORS[i])?.as_color()?.value();
            if !targets
                .iter()
                .any(|t| t.red == c.red && t.green == c.green && t.blue == c.blue)
            {
                targets.push(c);
            }
        }
        if mode == MODE_COLORS && targets.is_empty() {
            return Ok(());
        }

        let grad8 = params.get(Params::GradColor)?.as_color()?.value();
        let invert = params.get(Params::Invert)?.as_checkbox()?.value();
        let center = {
            let (x, y) = params.get(Params::Center)?.as_point()?.value();
            (x as f64, y as f64)
        };
        let blur = params.get(Params::Radius)?.as_float_slider()?.value();
        // 元の GetParams: /100 して 0.02 を下限にする。
        let feather = {
            let v = params.get(Params::Feather)?.as_float_slider()?.value() / 100.0;
            if v < 0.02 {
                0.02
            } else {
                v
            }
        };
        let hyperbolic = params.get(Params::Hyperbolic)?.as_float_slider()?.value();
        // 元の GetParams: /100 して 0 以下なら 0.01。
        let aspect = {
            let v = params.get(Params::Aspect)?.as_float_slider()?.value() / 100.0;
            if v <= 0.0 {
                0.01
            } else {
                v
            }
        };
        let angle = params.get(Params::Angle)?.as_angle()?.value() as f64;

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth || w == 0 || h == 0 {
            return Err(Error::BadCallbackParameter);
        }

        // 1. 抽出パス (mode 3 では走らせない)。元は外接矩形を取らない。
        let mut keep = vec![true; w * h];
        if mode != MODE_ALL {
            for y in 0..h {
                for x in 0..w {
                    let inside = y < in_h && x < in_w;
                    let (c8, alpha_positive) = if !inside {
                        (
                            ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            },
                            false,
                        )
                    } else {
                        match depth {
                            8 => {
                                let p = in_layer.as_pixel8(x, y);
                                (*p, p.alpha > 0)
                            }
                            16 => {
                                let p = in_layer.as_pixel16(x, y);
                                (conv_16_to_8(p), p.alpha > 0)
                            }
                            _ => {
                                let p = in_layer.as_pixel32(x, y);
                                (conv_32_to_8(p), p.alpha > 0.0)
                            }
                        }
                    };
                    let hit = if mode == MODE_COLORS {
                        targets
                            .iter()
                            .any(|c| c.red == c8.red && c.green == c8.green && c.blue == c8.blue)
                    } else {
                        alpha_positive
                    };
                    // 残った画素でもアルファが 0 なら、グラデーションの
                    // 早期リターン条件 (out->alpha == 0) に当たる。
                    keep[y * w + x] = hit && alpha_positive;
                }
            }
        }

        // 2. 半径などの前処理。元の RenderTargetGradRadialImpl。
        let ds = in_data.downsample_x();
        let ds_x = ds.num as f32 / ds.den as f32;
        let radius = (blur as f32 * ds_x).max(1.0);
        let inv_aspect = 1.0 / (aspect as f32).max(0.01);
        let feather = (feather as f32).clamp(0.0, 1.0);
        // 元は -angle で回す。
        let rad = angle as f32 * 0.01745329;
        let (cos_q, sin_q) = ((-rad).cos(), (-rad).sin());
        let k = hyperbolic as f32;
        let is_all = mode == MODE_ALL;
        let (cx, cy) = (center.0 as f32, center.1 as f32);

        let origin = in_data.output_origin();
        let (ox, oy) = (origin.h as f32, origin.v as f32);
        let grad16 = conv_8_to_16(&grad8);
        let grad32 = conv_8_to_32(&grad8);

        for y in 0..h {
            for x in 0..w {
                if !is_all && !keep[y * w + x] {
                    // 抽出パスが 0 を書いた画素。
                    match depth {
                        8 => {
                            *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        }
                        16 => {
                            *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        }
                        _ => {
                            *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                                alpha: 0.0,
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                            }
                        }
                    }
                    continue;
                }
                let dx = (x as f32 + ox) - cx;
                let dy = (y as f32 + oy) - cy;
                // 回してから横だけ縮める。
                let rx = (dx * cos_q - dy * sin_q) * inv_aspect;
                let ry = dx * sin_q + dy * cos_q;
                let dist = (rx * rx + ry * ry).sqrt();
                let mut t = (dist / radius).clamp(0.0, 1.0);
                if invert {
                    t = 1.0 - t;
                }
                let t_hyper = ((1.0 + k) * t) / (1.0 + k * t);
                let feather_start = (1.0f32 - feather).max(0.0);
                let mut mask = if t_hyper <= feather_start {
                    0.0
                } else if t_hyper >= 1.0 {
                    1.0
                } else {
                    // smoothstep
                    let v = (t_hyper - feather_start) / (1.0 - feather_start);
                    v * v * (3.0 - 2.0 * v)
                };
                // 元は invert の有無にかかわらず1回反転する (枝が2つに
                // 分かれているだけ)。
                mask = 1.0 - mask;
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        *p = grad8;
                        // 元は整数深度では clamp したあと +0.5 して丸める。
                        p.alpha = ((mask * MAX_CHANNEL8 as f32).clamp(0.0, MAX_CHANNEL8 as f32)
                            + 0.5) as u8;
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        *p = grad16;
                        p.alpha = ((mask * MAX_CHANNEL16 as f32).clamp(0.0, MAX_CHANNEL16 as f32)
                            + 0.5) as u16;
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        *p = grad32;
                        p.alpha = mask.clamp(0.0, 1.0);
                    }
                }
            }
        }
        Ok(())
    }
}
