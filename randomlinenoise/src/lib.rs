//! F's RandomLineNoise の移植。
//!
//! 短い横棒 (縦にもできる) を乱数で画面中にばらまいて、その下の画素の RGB に
//! 正負のノイズを足す。テレビの砂嵐というより、横に走る線状のノイズ。
//!
//! 1本ぶんの手順 (元の Exec + GetLineDrawParamsFromParamInfo):
//!
//! 1. 始点を `F_RAND2(ノイズの長さの最大/-2, 画面の幅)` と
//!    `F_RAND3(画面の高さ)` で決める。**左と下には画面の外にも出る**
//! 2. 長さを `F_RAND2(長さの最小, 長さの最大)` で決める。0 以下なら描かない
//! 3. 縦のチェックが入っていれば縦長、入っていなければ横長の長方形にする
//!    (太さはノイズの幅)
//! 4. 画面から完全にはみ出していれば描かない。かかっていれば縁で切る
//! 5. 色のチェックが入っていれば RGB 別々に `F_RAND2(-強さ, 強さ)`、
//!    入っていなければ1回引いて3つに使い、**残り2回は捨てる**
//! 6. 長方形の中の、アルファが 0 でない画素の RGB に足す
//!
//! 元から写した細かいところ:
//!
//! - 種はフレーム番号。「ノイズ量 > 0 かつ 強さ > 0」でなければ何もしない
//! - **ノイズ量はスライダーの値の 20 倍**
//! - 強さは `PF_Fixed / 100` の整数割り算 (切り捨て)。目盛りは 16bit の
//!   チャンネル値なので、50% で PF_MAX_CHAN16 ぶん
//! - 長さと幅は downsample を掛ける (`downScale`)
//! - 長さの最小のほうが大きければ入れ替える
//! - **描かない回も乱数を3回捨てる**。ここを飛ばすと以降の並びが全部ずれる
//! - 深度ごとの足し方は元のまま: 8bit は `FS_CONVERT16TO8` で 8bit に
//!   落としてから足す (負の値は 0 方向へ切り捨て)、16bit はそのまま足す、
//!   32bit は f32 で 32768 で割ってから足す。clamp の上限も元のとおり
//!   8bit は 255、16bit は 32768、32bit は 32.0
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/RandomLineNoise

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    value: i64,
    /// 元の strong (16bit のチャンネル値の目盛り)
    strong: i64,
    length_min: i64,
    length_max: i64,
    color: bool,
    weight: i64,
    vertual: bool,
}

/// 描く1本。元の LineDrawParam のうち位置と色だけ。画面の中に切ってある。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    x0: i64,
    x1: i64,
    y0: i64,
    y1: i64,
    r: i64,
    g: i64,
    b: i64,
}

/// 元の Exec の乱数のところ。深度に依らないのでまとめてある。
fn plan(info: &Info, w: i64, h: i64, frame: u32) -> Vec<Line> {
    let mut out = Vec::new();
    if info.value <= 0 || info.strong <= 0 {
        return out;
    }
    let mut rng = XorShift::new(frame);
    // 元: lengthMax/-2。C の整数割り算なので 0 方向へ切り捨て。
    let left = info.length_max / -2;
    for _ in 0..info.value {
        let x = rng.rand2(left, w);
        // 元の F_RAND3(height) は 0..=height。height そのものも出るが、
        // そのときは下の判定で落ちる。
        let y = rng.fm_rand(h);

        let length = rng.rand2(info.length_min, info.length_max);
        let mut ok = length > 0;
        let (mut x0, mut x1, mut y0, mut y1) = (0i64, 0i64, 0i64, 0i64);
        if ok {
            if info.vertual {
                (x0, x1) = (x, x + info.weight - 1);
                (y0, y1) = (y, y + length - 1);
            } else {
                (x0, x1) = (x, x + length - 1);
                (y0, y1) = (y, y + info.weight - 1);
            }
            if x1 < 0 || x0 >= w || y1 < 0 || y0 >= h {
                ok = false;
            } else {
                x0 = x0.max(0);
                x1 = x1.min(w - 1);
                y0 = y0.max(0);
                y1 = y1.min(h - 1);
            }
        }
        if ok {
            let (r, g, b) = if info.color {
                (
                    rng.rand2(-info.strong, info.strong),
                    rng.rand2(-info.strong, info.strong),
                    rng.rand2(-info.strong, info.strong),
                )
            } else {
                let v = rng.rand2(-info.strong, info.strong);
                // 元: 使わないぶんも2回引いて進める。
                rng.next();
                rng.next();
                (v, v, v)
            };
            out.push(Line {
                x0,
                x1,
                y0,
                y1,
                r,
                g,
                b,
            });
        } else {
            // 元: 描かない回も3回引いて進める。
            rng.next();
            rng.next();
            rng.next();
        }
    }
    out
}

/// 元の FS_CONVERT16TO8。負の値でも 0 方向へ切り捨てる (C の整数割り算)。
fn conv_16_to_8(v: i64) -> i64 {
    (v * 255 + 16384) / 32768
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    Strong,
    LengthMin,
    LengthMax,
    Color,
    Weight,
    Vertual,
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
        // 元: PF_ADD_SLIDER("ノイズ量", 0, 10000, 0, 1000, 100)。
        // 名前は元が非 ASCII なので英語。
        params.add(
            Params::Value,
            "Amount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(10000);
                f.set_slider_min(0);
                f.set_slider_max(1000);
                f.set_default(100);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("ノイズの強さ(%)", 0, 100, 0, 50, 10, 1, 0, 0)。
        params.add(
            Params::Strong,
            "Strength(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(10.0);
                f.set_value(f.default());
            }),
        )?;
        for (p, name, dflt) in [
            (Params::LengthMin, "Length Min", 10),
            (Params::LengthMax, "Length Max", 30),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(0);
                    f.set_valid_max(1024);
                    f.set_slider_min(0);
                    f.set_slider_max(100);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_CHECKBOX("ノイズの色", "カラー", FALSE)。
        params.add(
            Params::Color,
            "Noise_Color",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("Color");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("ノイズの幅", 1, 16, 1, 4, 2)。
        params.add(
            Params::Weight,
            "Thickness",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(16);
                f.set_slider_min(1);
                f.set_slider_max(4);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("縦方向に", "縦", FALSE)。
        params.add(
            Params::Vertual,
            "Vertical",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("Vert");
                f.set_default(false);
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
                out_data.set_return_msg("fs-rs randomlinenoise");
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
        let ds = in_data.downsample_x();
        let down = |v: i64| -> i64 {
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        // 元の GetParams の順。
        // ノイズ量はスライダーの 20 倍。
        let value = params.get(Params::Value)?.as_slider()?.value() as i64 * 20;
        // 強さは PF_Fixed を 100 で整数割り算 (切り捨て)。
        let strong =
            libs::utils::flt2fix(params.get(Params::Strong)?.as_float_slider()?.value()) as i64
                / 100;
        let mut length_min = down(params.get(Params::LengthMin)?.as_slider()?.value() as i64);
        let mut length_max = down(params.get(Params::LengthMax)?.as_slider()?.value() as i64);
        if length_min > length_max {
            std::mem::swap(&mut length_min, &mut length_max);
        }
        let color = params.get(Params::Color)?.as_checkbox()?.value();
        let weight = down(params.get(Params::Weight)?.as_slider()?.value() as i64);
        let vertual = params.get(Params::Vertual)?.as_checkbox()?.value();
        let info = Info {
            value,
            strong,
            length_min,
            length_max,
            color,
            weight,
            vertual,
        };

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Render/Exec: まず入力をそのままコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if w == 0 || h == 0 {
            return Ok(());
        }

        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let lines = plan(&info, w as i64, h as i64, frame);
        if lines.is_empty() {
            return Ok(());
        }

        for l in &lines {
            for y in l.y0..=l.y1 {
                for x in l.x0..=l.x1 {
                    let (x, y) = (x as usize, y as usize);
                    match depth {
                        8 => {
                            let p = out_layer.as_pixel8_mut(x, y);
                            if p.alpha == 0 {
                                continue;
                            }
                            // 元: 16bit の目盛りを 8bit に落としてから足す。
                            let f =
                                |v: u8, n: i64| (v as i64 + conv_16_to_8(n)).clamp(0, 255) as u8;
                            p.red = f(p.red, l.r);
                            p.green = f(p.green, l.g);
                            p.blue = f(p.blue, l.b);
                        }
                        16 => {
                            let p = out_layer.as_pixel16_mut(x, y);
                            if p.alpha == 0 {
                                continue;
                            }
                            let f = |v: u16, n: i64| (v as i64 + n).clamp(0, 32768) as u16;
                            p.red = f(p.red, l.r);
                            p.green = f(p.green, l.g);
                            p.blue = f(p.blue, l.b);
                        }
                        _ => {
                            let p = out_layer.as_pixel32_mut(x, y);
                            if p.alpha == 0.0 {
                                continue;
                            }
                            // 元: f32 で 32768 で割って足し、0..32 で切る。
                            let f = |v: f32, n: i64| {
                                (v + n as f32 / 32768.0f32).clamp(0.0, 32.0) as f32
                            };
                            p.red = f(p.red, l.r);
                            p.green = f(p.green, l.g);
                            p.blue = f(p.blue, l.b);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info {
            value: 1,
            strong: 6553,
            length_min: 10,
            length_max: 10,
            color: false,
            weight: 2,
            vertual: false,
        }
    }

    /// 横向きは長さぶんの横棒、太さはノイズの幅。
    #[test]
    fn the_line_lies_flat_by_default() {
        let l = plan(&info(), 400, 400, 0);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].x1 - l[0].x0, 9);
        assert_eq!(l[0].y1 - l[0].y0, 1);
    }

    /// 縦のチェックで縦横が入れ替わる。
    #[test]
    fn the_line_stands_up_when_asked() {
        let mut i = info();
        i.vertual = true;
        let l = plan(&i, 400, 400, 0);
        assert_eq!(l[0].x1 - l[0].x0, 1);
        assert_eq!(l[0].y1 - l[0].y0, 9);
    }

    /// 色のチェックが無ければ RGB は同じ値。
    #[test]
    fn the_grey_noise_shares_one_value() {
        let mut i = info();
        i.value = 50;
        for l in plan(&i, 400, 400, 2) {
            assert_eq!((l.r, l.g), (l.g, l.b));
        }
    }

    /// 色のチェックを入れると RGB が別々になる。
    #[test]
    fn the_colour_noise_draws_three_values() {
        let mut i = info();
        i.value = 50;
        i.color = true;
        let l = plan(&i, 400, 400, 2);
        assert!(l.iter().any(|l| l.r != l.g || l.g != l.b));
    }

    /// 色を使うかどうかで乱数の並びが崩れない (使わない回も3回ぶん引く)。
    #[test]
    fn the_sequence_does_not_drift_between_the_colour_modes() {
        let mut a = info();
        a.value = 60;
        let mut b = a;
        b.color = true;
        let (ra, rb) = (plan(&a, 200, 200, 5), plan(&b, 200, 200, 5));
        assert_eq!(ra.len(), rb.len());
        for (x, y) in ra.iter().zip(rb.iter()) {
            assert_eq!((x.x0, x.x1, x.y0, x.y1), (y.x0, y.x1, y.y0, y.y1));
        }
    }

    /// 長さが 0 なら1本も描かない (乱数だけ進む)。
    #[test]
    fn a_zero_length_draws_nothing() {
        let mut i = info();
        i.value = 100;
        i.length_min = 0;
        i.length_max = 0;
        assert!(plan(&i, 200, 200, 1).is_empty());
    }

    /// 強さが 0、または量が 0 なら何もしない。
    #[test]
    fn nothing_happens_without_amount_or_strength() {
        let mut i = info();
        i.strong = 0;
        assert!(plan(&i, 200, 200, 0).is_empty());
        let mut i = info();
        i.value = 0;
        assert!(plan(&i, 200, 200, 0).is_empty());
    }

    /// 始点は左に食い込むので、画面の縁で切られた棒も出る。
    #[test]
    fn the_line_is_clipped_at_the_edges() {
        let mut i = info();
        i.value = 500;
        i.length_max = 40;
        i.length_min = 40;
        let l = plan(&i, 64, 64, 3);
        assert!(l.iter().any(|l| l.x0 == 0 && l.x1 - l.x0 < 39), "左で切れる");
        assert!(l.iter().any(|l| l.x1 == 63), "右で切れる");
        for l in &l {
            assert!(l.x0 >= 0 && l.x1 < 64 && l.y0 >= 0 && l.y1 < 64);
        }
    }

    /// 8bit に落とすところ。負の値は 0 方向へ切り捨てる。
    #[test]
    fn the_eight_bit_noise_truncates_towards_zero() {
        assert_eq!(conv_16_to_8(32768), 255);
        assert_eq!(conv_16_to_8(0), 0);
        assert_eq!(conv_16_to_8(6553), 51);
        assert_eq!(conv_16_to_8(-6553), -50);
        assert_eq!(conv_16_to_8(-32768), -254);
    }
}
