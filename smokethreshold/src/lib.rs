//! F's smokeThreshold の移植。
//!
//! アルファをしきい値で二値化し、残った画素を輝度で hilight / normal /
//! shadow1 / shadow2 の4帯に振り分けてそれぞれの色で塗る。さらに
//! アルファを LineWeight ぶん収縮させ、削れた縁を線の色で塗る。
//!
//! 元の処理順:
//!   1. 画素ごとに二値化してマスクと帯のタグを作る
//!   2. 線がどれか1つでも有効なら MinimaxA でアルファだけを収縮する
//!   3. 画素ごとに「元のマスク > 収縮後のアルファ」なら縁と見なして線の色で塗る
//!
//! 元は 1 の結果を red (マスク) と green (タグ 2/4/6/8) に押し込んで 2 の
//! minimax を通すが、minimax はアルファしか読み書きしないので、ここでは
//! マスクとタグを別の配列で持っている (等価)。
//!
//! 8bit は元と同じ整数演算。16/32bit はこのリポジトリの他と同じく f32 版を
//! 使う ([`libs::fs_graphics`])。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/smokeThreshold

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::fs_graphics::{Image as ImageF, PixelF, ScanLine};
use libs::fs_graphics8::{Image8, Px8};
use libs::utils::{conv_8_to_32, round_byte_fp_long, round_fp_short};

/// 帯のタグ。元は green チャンネルに 2/4/6/8 を入れている。
const TAG_NONE: u8 = 0;
const TAG_HI: u8 = 2;
const TAG_NORMAL: u8 = 4;
const TAG_SDW1: u8 = 6;
const TAG_SDW2: u8 = 8;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    AlphaThreshold,
    HilightEnabled,
    Shadow1Enabled,
    Shadow2Enabled,
    Hilight,
    Shadow1,
    Shadow2,
    HilightColor,
    NormalColor,
    Shadow1Color,
    Shadow2Color,
    LineStart,
    LineWeight,
    NormalLineEnabled,
    Shadow1LineEnabled,
    Shadow2LineEnabled,
    NormalLine,
    Shadow1Line,
    Shadow2Line,
    LineEnd,
}

/// GetParams 相当。しきい値は 0..1 に正規化済み。
struct Settings {
    alpha: f32,
    hi: f32,
    sdw1: f32,
    sdw2: f32,
    hi_on: bool,
    s1_on: bool,
    s2_on: bool,
    nl_on: bool,
    s1l_on: bool,
    s2l_on: bool,
    line_weight: i32,
    hi_col: ae::Pixel8,
    nor_col: ae::Pixel8,
    sdw1_col: ae::Pixel8,
    sdw2_col: ae::Pixel8,
    nor_line: ae::Pixel8,
    sdw1_line: ae::Pixel8,
    sdw2_line: ae::Pixel8,
}

impl Settings {
    fn any_line(&self) -> bool {
        self.nl_on || self.s1l_on || self.s2l_on
    }

    /// 帯のタグを選ぶ。元の alpha* の if 連鎖と同じ順序。
    /// v と各しきい値は同じ目盛りで渡す。
    fn tag<T: PartialOrd>(&self, v: T, hi: T, sdw1: T, sdw2: T) -> u8 {
        if self.hi_on && v >= hi {
            TAG_HI
        } else if self.s2_on && v <= sdw2 {
            TAG_SDW2
        } else if self.s1_on && v <= sdw1 {
            TAG_SDW1
        } else {
            TAG_NORMAL
        }
    }

    /// タグと「縁かどうか」から塗る色を選ぶ。
    /// 元の outline* と同じで、hilight には線の指定がない。
    fn color(&self, tag: u8, outline: bool) -> ae::Pixel8 {
        match tag {
            TAG_HI => self.hi_col,
            TAG_SDW1 => {
                if outline && self.s1l_on {
                    self.sdw1_line
                } else {
                    self.sdw1_col
                }
            }
            TAG_SDW2 => {
                if outline && self.s2l_on {
                    self.sdw2_line
                } else {
                    self.sdw2_col
                }
            }
            _ => {
                if outline && self.nl_on {
                    self.nor_line
                } else {
                    self.nor_col
                }
            }
        }
    }
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_ALPHA, 0, 100, 25, 75, 1, 50, 1, 0, 0, ID_ALPHA)
        params.add(
            Params::AlphaThreshold,
            "alphaThreshold",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(25.0);
                f.set_slider_max(75.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;

        for (p, name) in [
            (Params::HilightEnabled, "hilightEnabled"),
            (Params::Shadow1Enabled, "shadow1Enabled"),
            (Params::Shadow2Enabled, "shadow2Enabled"),
        ] {
            params.add(
                p,
                name,
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(true);
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: hilight 80 (60..100) / shadow1 40 (20..60) / shadow2 20 (0..40)
        for (p, name, smin, smax, dflt) in [
            (Params::Hilight, "hilight", 60.0, 100.0, 80.0),
            (Params::Shadow1, "shadow1", 20.0, 60.0, 40.0),
            (Params::Shadow2, "shadow2", 0.0, 40.0, 20.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(smin);
                    f.set_slider_max(smax);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }

        for (p, name, g) in [
            (Params::HilightColor, "hilightColor", 0xF0u8),
            (Params::NormalColor, "normalColor", 0x90),
            (Params::Shadow1Color, "shadow1Color", 0x50),
            (Params::Shadow2Color, "shadow2Color", 0x30),
        ] {
            params.add(
                p,
                name,
                ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: g,
                        green: g,
                        blue: g,
                    });
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_TOPIC(STR_OUTLINE, ..) ... PF_END_TOPIC
        params.add_group(
            Params::LineStart,
            Params::LineEnd,
            "OutLine",
            false,
            |params| {
                // 元: PF_ADD_SLIDER(STR_LINE_WEIGHT, 1, 5, 1, 5, 2, ..)
                params.add(
                    Params::LineWeight,
                    "LineWeight",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(5);
                        f.set_slider_min(1);
                        f.set_slider_max(5);
                        f.set_default(2);
                        f.set_value(f.default());
                    }),
                )?;

                for (p, name) in [
                    (Params::NormalLineEnabled, "normalLine_Enabled"),
                    (Params::Shadow1LineEnabled, "shadow1Line_Enabled"),
                    (Params::Shadow2LineEnabled, "shadow2Line_Enabled"),
                ] {
                    params.add(
                        p,
                        name,
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(true);
                            f.set_value(f.default());
                        }),
                    )?;
                }

                // 元は shadow1Line / shadow2Line の色の名前に
                // STR_SDW1_LINE_E / STR_SDW2_LINE_E (チェックボックス用の
                // 文字列) を渡していて、UI に "shadow1Line_Enabled" と
                // 出てしまう。正しい名前にしてある。
                for (p, name, g) in [
                    (Params::NormalLine, "normalLine", 0x50u8),
                    (Params::Shadow1Line, "shadow1Line", 0x30),
                    (Params::Shadow2Line, "shadow2Line", 0x20),
                ] {
                    params.add(
                        p,
                        name,
                        ae::ColorDef::setup(|f| {
                            f.set_default(ae::Pixel8 {
                                alpha: 0xFF,
                                red: g,
                                green: g,
                                blue: g,
                            });
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
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
                out_data.set_return_msg("fs-rs smokethreshold");
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

    fn settings(&self, params: &mut ae::Parameters<Params>) -> Result<Settings, Error> {
        // 元の GetParams はしきい値を /100 する。
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f32, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() as f32 / 100.0)
        };
        Ok(Settings {
            alpha: pct(Params::AlphaThreshold, params)?,
            hi: pct(Params::Hilight, params)?,
            sdw1: pct(Params::Shadow1, params)?,
            sdw2: pct(Params::Shadow2, params)?,
            hi_on: params.get(Params::HilightEnabled)?.as_checkbox()?.value(),
            s1_on: params.get(Params::Shadow1Enabled)?.as_checkbox()?.value(),
            s2_on: params.get(Params::Shadow2Enabled)?.as_checkbox()?.value(),
            nl_on: params
                .get(Params::NormalLineEnabled)?
                .as_checkbox()?
                .value(),
            s1l_on: params
                .get(Params::Shadow1LineEnabled)?
                .as_checkbox()?
                .value(),
            s2l_on: params
                .get(Params::Shadow2LineEnabled)?
                .as_checkbox()?
                .value(),
            line_weight: params.get(Params::LineWeight)?.as_slider()?.value(),
            hi_col: params.get(Params::HilightColor)?.as_color()?.value(),
            nor_col: params.get(Params::NormalColor)?.as_color()?.value(),
            sdw1_col: params.get(Params::Shadow1Color)?.as_color()?.value(),
            sdw2_col: params.get(Params::Shadow2Color)?.as_color()?.value(),
            nor_line: params.get(Params::NormalLine)?.as_color()?.value(),
            sdw1_line: params.get(Params::Shadow1Line)?.as_color()?.value(),
            sdw2_line: params.get(Params::Shadow2Line)?.as_color()?.value(),
        })
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let s = self.settings(params)?;
        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth {
            return Err(Error::BadCallbackParameter);
        }

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        if depth == 8 {
            // 元の toInfo8: しきい値は 255 倍して +0.5 で丸める。
            let m = MAX_CHANNEL8 as f32;
            let q = |v: f32| round_byte_fp_long(m * v + 0.5) as i32;
            let (ta, thi, ts1, ts2) = (q(s.alpha), q(s.hi), q(s.sdw1), q(s.sdw2));

            let mut mask = vec![0u8; w * h];
            let mut tag = vec![TAG_NONE; w * h];
            // minimax はアルファしか見ないので、マスクをアルファに入れた
            // 画像を作って通す。
            let mut img = Image8::new(w, h);
            for y in 0..h.min(in_h) {
                for x in 0..w.min(in_w) {
                    let p = in_layer.as_pixel8(x, y);
                    // 元: 輝度は +0.5 して A_long にキャスト。
                    let v = (0.29891 * p.red as f64
                        + 0.58661 * p.green as f64
                        + 0.11448 * p.blue as f64
                        + 0.5) as i32;
                    let on = p.alpha as i32 >= ta;
                    let i = y * w + x;
                    mask[i] = if on { MAX_CHANNEL8 as u8 } else { 0 };
                    tag[i] = if on {
                        s.tag(v, thi, ts1, ts2)
                    } else {
                        TAG_NONE
                    };
                    *img.get_mut(x, y) = Px8::new(mask[i], 0, 0, 0);
                }
            }

            if s.any_line() {
                // 元: MinimaxA8(lineWeight * -1) で収縮する。
                img.minimax_alpha8(-s.line_weight);
            }

            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    let a = mask[i];
                    let mut p = ae::Pixel8 {
                        alpha: 0,
                        red: 0,
                        green: 0,
                        blue: 0,
                    };
                    if a > 0 {
                        // 元: ol = (red - alpha) > 0、red は収縮前のマスク。
                        let outline = a > img.get(x, y).alpha;
                        p = s.color(tag[i], outline);
                    }
                    p.alpha = a;
                    *out_layer.as_pixel8_mut(x, y) = p;
                }
            }
            return Ok(());
        }

        // 16/32bit: 0..1 の f32 で処理する。
        let (ta, thi, ts1, ts2) = (
            round_fp_short(s.alpha),
            round_fp_short(s.hi),
            round_fp_short(s.sdw1),
            round_fp_short(s.sdw2),
        );
        let to32 = |c: &ae::Pixel8| {
            let p = conv_8_to_32(c);
            PixelF::new(p.alpha, p.red, p.green, p.blue)
        };

        let mut mask = vec![0.0f32; w * h];
        let mut tag = vec![TAG_NONE; w * h];
        let mut img = ImageF::new(w, h);
        for y in 0..h.min(in_h) {
            for x in 0..w.min(in_w) {
                let p = match depth {
                    16 => {
                        let p = in_layer.as_pixel16(x, y);
                        let k = 1.0 / MAX_CHANNEL16 as f32;
                        PixelF::new(
                            p.alpha as f32 * k,
                            p.red as f32 * k,
                            p.green as f32 * k,
                            p.blue as f32 * k,
                        )
                    }
                    32 => {
                        let p = in_layer.as_pixel32(x, y);
                        PixelF::new(p.alpha, p.red, p.green, p.blue)
                    }
                    _ => return Err(Error::BadCallbackParameter),
                };
                let v = 0.29891 * p.red + 0.58661 * p.green + 0.11448 * p.blue;
                let on = p.alpha >= ta;
                let i = y * w + x;
                mask[i] = if on { 1.0 } else { 0.0 };
                tag[i] = if on {
                    s.tag(v, thi, ts1, ts2)
                } else {
                    TAG_NONE
                };
                *img.get_mut(x, y) = PixelF::new(mask[i], 0.0, 0.0, 0.0);
            }
        }

        if s.any_line() {
            img.minimax_alpha(-s.line_weight, ScanLine::HorAndVer);
        }

        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let a = mask[i];
                let mut p = PixelF::new(0.0, 0.0, 0.0, 0.0);
                if a > 0.0 {
                    let outline = a > img.get(x, y).alpha;
                    p = to32(&s.color(tag[i], outline));
                }
                p.alpha = a;
                match depth {
                    16 => {
                        let m = MAX_CHANNEL16 as f32;
                        let k = |c: f32| (c.clamp(0.0, 1.0) * m).round() as u16;
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha: k(p.alpha),
                            red: k(p.red),
                            green: k(p.green),
                            blue: k(p.blue),
                        };
                    }
                    _ => {
                        *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                            alpha: p.alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        };
                    }
                }
            }
        }
        Ok(())
    }
}
