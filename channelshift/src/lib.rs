//! F's ChannelShift の移植。
//!
//! A/R/G/B それぞれに角度と距離を与えて、チャンネルごとに別の場所から
//! 画素を拾う。拾う位置は固定小数なのでバイリニアで混ぜる。はみ出した
//! ところの扱いは「透明 / 端を伸ばす / 折り返す / 鏡」から選ぶ。
//!
//! 4つの距離がすべて 0 なら入力をそのまま通す。
//!
//! 元は PF_ADD_FIXED (固定小数スライダー) だが、after-effects クレートが
//! 用意していないのでリポジトリの他と揃えて float スライダーにしている。
//!
//! 元の `getPixel*_rep` / `getPixel*_mirror` は折り返しを1回しかしないので、
//! ずらし量が画像の幅や高さを超えると配列の外を読む。Rust では落ちるため
//! 最後に端へ寄せてある (範囲に収まる場合の値は元と同じ)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ChannelShift

use after_effects::{self as ae};
use libs::utils::{round_byte_fp_double, round_fp_short_double, round_short_fp_double};

/// 元の FLT2FIX。
fn flt2fix(f: f64) -> i32 {
    (f * 65536.0 + if f < 0.0 { -0.5 } else { 0.5 }) as i32
}

/// 元の CRotCalc::SetRotLength(PF_Fixed, PF_Fixed)。象限ごとに sin/cos を
/// 入れ替えて使っているので、そのまま写してある。
fn rot_length(r: i32, len: i32) -> (i32, i32) {
    if len == 0 {
        return (0, 0);
    }
    // 元の RoundRot。
    let mut rr = r % (360 << 16);
    if rr < 0 {
        rr += 360 << 16;
    }
    let r2 = rr as f64 / 65536.0;
    let v2 = len as f64 / 65536.0;
    let d = std::f64::consts::PI / 180.0;
    let (x, y) = if rr < (90 << 16) {
        ((d * r2).sin() * v2, (d * r2).cos() * v2 * -1.0)
    } else if rr < (180 << 16) {
        ((d * (r2 - 90.0)).cos() * v2, (d * (r2 - 90.0)).sin() * v2)
    } else if rr < (270 << 16) {
        (
            (d * (r2 - 180.0)).sin() * v2 * -1.0,
            (d * (r2 - 180.0)).cos() * v2,
        )
    } else {
        (
            (d * (r2 - 270.0)).cos() * v2 * -1.0,
            (d * (r2 - 270.0)).sin() * v2 * -1.0,
        )
    };
    (flt2fix(x), flt2fix(y))
}

/// 元の edge_none / edge_fill / edge_rep / edge_mirror。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    None,
    Fill,
    Rep,
    Mirror,
}

impl Edge {
    fn from_popup(v: i32) -> Self {
        match v {
            2 => Edge::Fill,
            3 => Edge::Rep,
            4 => Edge::Mirror,
            _ => Edge::None,
        }
    }

    /// 1軸分の座標の折り返し。`None` は透明 (edge_none のはみ出し)。
    fn map(self, v: i32, n: i32) -> Option<i32> {
        match self {
            Edge::None => {
                if v < 0 || v >= n {
                    None
                } else {
                    Some(v)
                }
            }
            Edge::Fill => Some(if v < 0 {
                0
            } else if v >= n {
                n - 1
            } else {
                v
            }),
            Edge::Rep => {
                let r = if v < 0 {
                    v + n
                } else if v >= n {
                    v - n
                } else {
                    v
                };
                Some(r.clamp(0, n - 1))
            }
            Edge::Mirror => {
                let r = if v < 0 {
                    -v
                } else if v >= n {
                    (n - (v - n) - 1) % n
                } else {
                    v
                };
                Some(r.clamp(0, n - 1))
            }
        }
    }
}

/// チャンネルの並び (元の PF_Pixel と同じ a,r,g,b)。
const A: usize = 0;
const R: usize = 1;
const G: usize = 2;
const B: usize = 3;

/// 元の getPixel*_CS の1チャンネル分。`get` はその深度の目盛りのまま返す。
fn sample(
    get: &dyn Fn(i32, i32) -> [f64; 4],
    edge: Edge,
    w: i32,
    h: i32,
    x: i32,
    y: i32,
    ch: usize,
) -> f64 {
    // 元: xx = x>>16 (負でも算術シフトなので床), dx1 = (x & 0xFFFF)/65536。
    let xx = x >> 16;
    let yy = y >> 16;
    let dx1 = (x & 0xFFFF) as f64 / 65536.0;
    let dy1 = (y & 0xFFFF) as f64 / 65536.0;
    let dx2 = 1.0 - dx1;
    let dy2 = 1.0 - dy1;

    let at = |px: i32, py: i32| -> f64 {
        match (edge.map(px, w), edge.map(py, h)) {
            (Some(sx), Some(sy)) => get(sx, sy)[ch],
            // edge_none は透明の黒を返す。
            _ => 0.0,
        }
    };

    if dx1 == 0.0 && dy1 == 0.0 {
        at(xx, yy)
    } else {
        at(xx, yy) * dx2 * dy2
            + at(xx + 1, yy) * dx1 * dy2
            + at(xx, yy + 1) * dx2 * dy1
            + at(xx + 1, yy + 1) * dx1 * dy1
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    RedTopic,
    RedRot,
    RedLength,
    RedTopicEnd,
    GreenTopic,
    GreenRot,
    GreenLength,
    GreenTopicEnd,
    BlueTopic,
    BlueRot,
    BlueLength,
    BlueTopicEnd,
    AlphaTopic,
    AlphaRot,
    AlphaLength,
    AlphaTopicEnd,
    Edge,
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
        for (topic, name, rot, rot_name, len, len_name, end, dflt) in [
            (
                Params::RedTopic,
                "Red",
                Params::RedRot,
                "Red_Rot",
                Params::RedLength,
                "Red_Length",
                Params::RedTopicEnd,
                0.0f32,
            ),
            (
                Params::GreenTopic,
                "Green",
                Params::GreenRot,
                "Green_Rot",
                Params::GreenLength,
                "Green_Length",
                Params::GreenTopicEnd,
                90.0,
            ),
            (
                Params::BlueTopic,
                "Blue",
                Params::BlueRot,
                "Blue_Rot",
                Params::BlueLength,
                "Blue_Length",
                Params::BlueTopicEnd,
                180.0,
            ),
            (
                Params::AlphaTopic,
                "Alpha",
                Params::AlphaRot,
                "Alpha_Rot",
                Params::AlphaLength,
                "Alpha_Length",
                Params::AlphaTopicEnd,
                270.0,
            ),
        ] {
            params.add_group(
                topic,
                end,
                name,
                false,
                |params: &mut ae::Parameters<Params>| -> Result<(), Error> {
                    // 元: PF_ADD_ANGLE(..., 0/90/180/270, ..)
                    params.add(
                        rot,
                        rot_name,
                        ae::AngleDef::setup(|f| {
                            f.set_default(dflt);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元: PF_ADD_FIXED(..., -30000, 30000, -100, 100, 0, 1, 0, 0, ..)
                    params.add(
                        len,
                        len_name,
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(-30000.0);
                            f.set_valid_max(30000.0);
                            f.set_slider_min(-100.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(0.0);
                            f.set_value(f.default());
                        }),
                    )?;
                    Ok(())
                },
            )?;
        }

        // 元: PF_ADD_POPUP(STR_EDGE_POP1, 4, 1, "none|extend|tile|mirror", ..)
        params.add(
            Params::Edge,
            "Edge",
            ae::PopupDef::setup(|f| {
                f.set_options(&["none", "extend", "tile", "mirror"]);
                f.set_default(1);
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
                out_data.set_return_msg("fs-rs channelshift");
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
        // 元の GetParams: 角度は PF_Fixed のまま、距離は固定小数にして
        // downScaleNoClip (整数の掛け算と割り算、丸めなし)。
        let num = in_data.downsample_x().num as i32;
        let den = in_data.downsample_x().den as i32;
        let get = |rot: Params, len: Params| -> Result<(i32, i32), Error> {
            let r = flt2fix(params.get(rot)?.as_angle()?.value() as f64);
            let l = flt2fix(params.get(len)?.as_float_slider()?.value());
            Ok((r, l * num / den))
        };
        let (r_angle, r_length) = get(Params::RedRot, Params::RedLength)?;
        let (g_angle, g_length) = get(Params::GreenRot, Params::GreenLength)?;
        let (b_angle, b_length) = get(Params::BlueRot, Params::BlueLength)?;
        let (a_angle, a_length) = get(Params::AlphaRot, Params::AlphaLength)?;
        let edge = Edge::from_popup(params.get(Params::Edge)?.as_popup()?.value());

        // 元の Exec: 4つとも 0 なら入力をそのまま通す。
        if r_length == 0 && g_length == 0 && b_length == 0 && a_length == 0 {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

        // 元の PramSet: 距離を XY に分ける。
        let off = [
            rot_length(a_angle, a_length),
            rot_length(r_angle, r_length),
            rot_length(g_angle, g_length),
            rot_length(b_angle, b_length),
        ];

        let (w, h) = (in_layer.width() as i32, in_layer.height() as i32);
        let (ow, oh) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        for y in 0..oh {
            for x in 0..ow {
                // 元: addChannelShiftPrm(xL<<16, yL<<16, &cs)。
                let fx = (x as i32) << 16;
                let fy = (y as i32) << 16;
                match depth {
                    8 => {
                        let get = |sx: i32, sy: i32| -> [f64; 4] {
                            let p = in_layer.as_pixel8(sx as usize, sy as usize);
                            [
                                p.alpha as f64,
                                p.red as f64,
                                p.green as f64,
                                p.blue as f64,
                            ]
                        };
                        let f = |ch: usize| -> f64 {
                            let (dx, dy) = off[ch];
                            sample(&get, edge, w, h, fx + dx, fy + dy, ch)
                        };
                        *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                            alpha: round_byte_fp_double(f(A)),
                            red: round_byte_fp_double(f(R)),
                            green: round_byte_fp_double(f(G)),
                            blue: round_byte_fp_double(f(B)),
                        };
                    }
                    16 => {
                        let get = |sx: i32, sy: i32| -> [f64; 4] {
                            let p = in_layer.as_pixel16(sx as usize, sy as usize);
                            [
                                p.alpha as f64,
                                p.red as f64,
                                p.green as f64,
                                p.blue as f64,
                            ]
                        };
                        let f = |ch: usize| -> f64 {
                            let (dx, dy) = off[ch];
                            sample(&get, edge, w, h, fx + dx, fy + dy, ch)
                        };
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha: round_short_fp_double(f(A)),
                            red: round_short_fp_double(f(R)),
                            green: round_short_fp_double(f(G)),
                            blue: round_short_fp_double(f(B)),
                        };
                    }
                    _ => {
                        let get = |sx: i32, sy: i32| -> [f64; 4] {
                            let p = in_layer.as_pixel32(sx as usize, sy as usize);
                            [
                                p.alpha as f64,
                                p.red as f64,
                                p.green as f64,
                                p.blue as f64,
                            ]
                        };
                        let f = |ch: usize| -> f64 {
                            let (dx, dy) = off[ch];
                            sample(&get, edge, w, h, fx + dx, fy + dy, ch)
                        };
                        *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                            alpha: round_fp_short_double(f(A)),
                            red: round_fp_short_double(f(R)),
                            green: round_fp_short_double(f(G)),
                            blue: round_fp_short_double(f(B)),
                        };
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

    #[test]
    fn rot_length_zero_is_origin() {
        assert_eq!(rot_length(0, 0), (0, 0));
    }

    #[test]
    fn rot_length_cardinal_directions() {
        // 0度は上 (y がマイナス)。
        assert_eq!(rot_length(0, 10 << 16), (0, -(10 << 16)));
        // 90度は右。
        assert_eq!(rot_length(90 << 16, 10 << 16), (10 << 16, 0));
        // 180度は下。
        assert_eq!(rot_length(180 << 16, 10 << 16), (0, 10 << 16));
        // 270度は左。
        assert_eq!(rot_length(270 << 16, 10 << 16), (-(10 << 16), 0));
    }

    #[test]
    fn rot_length_wraps_negative_angles() {
        assert_eq!(
            rot_length(-90 << 16, 10 << 16),
            rot_length(270 << 16, 10 << 16)
        );
    }

    #[test]
    fn edge_modes() {
        assert_eq!(Edge::None.map(-1, 10), None);
        assert_eq!(Edge::None.map(10, 10), None);
        assert_eq!(Edge::None.map(3, 10), Some(3));
        assert_eq!(Edge::Fill.map(-5, 10), Some(0));
        assert_eq!(Edge::Fill.map(12, 10), Some(9));
        assert_eq!(Edge::Rep.map(-1, 10), Some(9));
        assert_eq!(Edge::Rep.map(10, 10), Some(0));
        // 元の鏡: x<0 は符号反転、右にはみ出したら折り返す。
        assert_eq!(Edge::Mirror.map(-3, 10), Some(3));
        assert_eq!(Edge::Mirror.map(10, 10), Some(9));
        assert_eq!(Edge::Mirror.map(12, 10), Some(7));
    }

    #[test]
    fn sample_picks_single_pixel_without_fraction() {
        let get = |x: i32, y: i32| -> [f64; 4] { [0.0, (x * 10 + y) as f64, 0.0, 0.0] };
        let v = sample(&get, Edge::Fill, 4, 4, 2 << 16, 1 << 16, R);
        assert_eq!(v, 21.0);
    }

    #[test]
    fn sample_blends_halfway() {
        let get = |x: i32, _y: i32| -> [f64; 4] { [0.0, if x == 0 { 0.0 } else { 100.0 }, 0.0, 0.0] };
        let v = sample(&get, Edge::Fill, 4, 4, 1 << 15, 0, R);
        assert!((v - 50.0).abs() < 1e-9);
    }
}
