//! F's TargetGrad の移植。
//!
//! 絵の中から対象色 (または不透明な画素) を拾ってその外接矩形を求め、
//! 2点を結ぶ線に沿ったグラデーションをアルファに描く。RGB は gradColor。
//! autoPos を入れると外接矩形の中心を通る角度 rot の線を使い、切ると
//! start / last の2点を使う。guideDraw を入れると線と矩形を重ねて描く。
//!
//! 元の流れ:
//!   1. mode が 1 (targetColors) で有効な色が無ければ何もしない
//!   2. mode が 3 (all) 以外なら、対象色 (または alpha>0) だけ残して出力に
//!      書き、残った画素の外接矩形を求める
//!   3. autoPos なら矩形の中心を通る線、そうでなければ2点から線を決める
//!   4. 線に沿った位置 t (0..1) を hyperbolic で曲げ、1-v をアルファにする。
//!      mode が 3 以外では、2 で消えた画素 (alpha==0) は触らない
//!   5. guideDraw ならガイドを重ねる
//!
//! 既定値は元が .pref から読むので、出荷 .aex が
//! --inspect-experimental で報告する値に合わせてある。
//!
//! 未実装: colorTable のロード / セーブボタン (元はファイルダイアログを開いて
//! .pref を読み書きする)。スロットだけ合わせてあり、押しても何も起きない。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/TargetGrad

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32};

/// 元の COLOR_TABLE_COUNT。
const COLOR_COUNT: usize = 8;

/// 元の target ポップアップ (1 始まり)。
const MODE_COLORS: i32 = 1;
const MODE_ALPHA: i32 = 2;
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
    SwapPoint,
    Hyperbolic,
    TwoPointStart,
    StartPos,
    LastPos,
    TwoPointEnd,
    AutoPos,
    Rot,
    StartPercent,
    LastPercent,
    OffsetX,
    OffsetY,
    GuideDraw,
    GuideColor,
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

#[derive(Clone, Copy, PartialEq, Debug)]
struct Pt {
    x: f64,
    y: f64,
}

/// 元の RotPoint。グラデーションの線を決める。
struct RotPoint {
    start2: Pt,
    last2: Pt,
    center2: Pt,
    start2d: Pt,
    last2d: Pt,
    rot: f64,
}

/// 元の PointOnSegmentFraction。frac は float に落としてから使う。
fn on_segment(p1: Pt, p2: Pt, frac: f64) -> Pt {
    let f = frac as f32 as f64;
    Pt {
        x: p1.x + (p2.x - p1.x) * f,
        y: p1.y + (p2.y - p1.y) * f,
    }
}

impl RotPoint {
    /// 元の Set2Point。
    fn from_two_points(st: Pt, lt: Pt, st_per: f64, lt_per: f64, off_x: f64, off_y: f64) -> Self {
        let start2 = Pt {
            x: st.x + off_x,
            y: st.y + off_y,
        };
        let last2 = Pt {
            x: lt.x + off_x,
            y: lt.y + off_y,
        };
        let center2 = Pt {
            x: (start2.x + last2.x) / 2.0,
            y: (start2.y + last2.y) / 2.0,
        };
        // 元は線の向きから AE の角度を出し直す (ガイドの矢印に使う)。
        let (dx, dy) = (last2.x - start2.x, last2.y - start2.y);
        let math_deg = (-dy).atan2(dx) * 180.0 / std::f64::consts::PI;
        let mut ae_deg = (90.0 - math_deg + 180.0) % 360.0;
        if ae_deg < 0.0 {
            ae_deg += 360.0;
        }
        Self {
            start2,
            last2,
            center2,
            start2d: on_segment(start2, last2, st_per),
            last2d: on_segment(last2, start2, lt_per),
            rot: ae_deg,
        }
    }

    /// 元の SetRect。矩形の中心を通る角度 angle の線を取る。
    fn from_rect(
        rect: (f64, f64, f64, f64),
        angle: f64,
        st_per: f64,
        lt_per: f64,
        off_x: f64,
        off_y: f64,
    ) -> Self {
        let (a, b) = line_through_center(rect, angle);
        let center = Pt {
            x: (a.x + b.x) / 2.0,
            y: (a.y + b.y) / 2.0,
        };
        let start2 = Pt {
            x: a.x + off_x,
            y: a.y + off_y,
        };
        let last2 = Pt {
            x: b.x + off_x,
            y: b.y + off_y,
        };
        let center2 = Pt {
            x: center.x + off_x,
            y: center.y + off_y,
        };
        Self {
            start2,
            last2,
            center2,
            start2d: on_segment(start2, last2, st_per),
            last2d: on_segment(last2, start2, lt_per),
            rot: angle,
        }
    }

    /// 元の gPos。ガイドの矢印の先を出す。
    fn g_pos(&self, p: Pt, l: f64) -> Pt {
        let r = (self.rot + 90.0) * std::f64::consts::PI / 180.0;
        Pt {
            x: p.x + r.sin() * l,
            y: p.y - r.cos() * l,
        }
    }
}

/// 元の LineThroughCenterInRect。矩形の中心を通る角度 angle の直線が
/// 矩形の辺と交わる2点を返す。前方 (方向ベクトル側) と後方で1つずつ。
fn line_through_center(rect: (f64, f64, f64, f64), angle_deg: f64) -> (Pt, Pt) {
    const EPS: f64 = 1e-9;
    let (left, top, right, bottom) = rect;
    let cx = (left + right) * 0.5;
    let cy = (top + bottom) * 0.5;
    let rad = angle_deg * std::f64::consts::PI / 180.0;
    let dirx = rad.sin();
    // AE は Y が下向きなので符号を反転する。
    let diry = -rad.cos();

    let mut pts: Vec<Pt> = Vec::new();
    if dirx.abs() > EPS {
        for edge in [left, right] {
            let t = (edge - cx) / dirx;
            let y = cy + diry * t;
            if y >= top.min(bottom) - 1e-6 && y <= top.max(bottom) + 1e-6 {
                pts.push(Pt { x: edge, y });
            }
        }
    }
    if diry.abs() > EPS {
        for edge in [top, bottom] {
            let t = (edge - cy) / diry;
            let x = cx + dirx * t;
            if x >= left.min(right) - 1e-6 && x <= left.max(right) + 1e-6 {
                pts.push(Pt { x, y: edge });
            }
        }
    }

    // 元は 1e-3 以内を同一点として潰す。
    let mut uniq: Vec<Pt> = Vec::new();
    for p in pts {
        if !uniq
            .iter()
            .any(|q| (p.x - q.x).abs() < 1e-3 && (p.y - q.y).abs() < 1e-3)
        {
            uniq.push(p);
        }
    }
    let center = Pt { x: cx, y: cy };
    if uniq.len() < 2 {
        return (center, center);
    }

    // 方向ベクトルとの内積が正のものから最も遠い点、負のものから最も遠い点。
    let mut fwd: Option<(f64, usize)> = None;
    let mut bwd: Option<(f64, usize)> = None;
    for (i, p) in uniq.iter().enumerate() {
        let dot = (p.x - cx) * dirx + (p.y - cy) * diry;
        if dot >= 0.0 {
            if fwd.is_none() || dot > fwd.unwrap().0 {
                fwd = Some((dot, i));
            }
        } else if bwd.is_none() || dot < bwd.unwrap().0 {
            bwd = Some((dot, i));
        }
    }
    match (fwd, bwd) {
        (Some((_, f)), Some((_, b))) => (uniq[f], uniq[b]),
        _ => {
            // 片側しか無ければ中心から遠い2点を使う (元のまま)。
            uniq.sort_by(|a, b| {
                let da = (a.x - cx).powi(2) + (a.y - cy).powi(2);
                let db = (b.x - cx).powi(2) + (b.y - cy).powi(2);
                db.partial_cmp(&da).unwrap()
            });
            (uniq[0], uniq[1])
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
        // 元: PF_ADD_POPUP(STR_TARGET, 3, 既定は .pref から, "targetColors|alphaOn|all", ..)
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
            Params::SwapPoint,
            "swapPoint",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FLOAT_SLIDER(STR_HYPERBOLIC, 0, 50, 0, 20, 1, 5, 1, 0, 0, ..)
        params.add(
            Params::Hyperbolic,
            "hyperbolic",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(50.0);
                f.set_slider_min(0.0);
                f.set_slider_max(20.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(5.0);
                f.set_value(f.default());
            }),
        )?;

        params.add_group(
            Params::TwoPointStart,
            Params::TwoPointEnd,
            "2Point",
            true,
            |params| {
                // 元: PF_ADD_POINT(STR_START, 75, 50, 0, ..) 既定はレイヤ比の%。
                for (p, name, xy) in [
                    (Params::StartPos, "start", (75.0, 50.0)),
                    (Params::LastPos, "last", (25.0, 50.0)),
                ] {
                    params.add(
                        p,
                        name,
                        ae::PointDef::setup(|f| {
                            f.set_default(xy);
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;

        params.add(
            Params::AutoPos,
            "autoPos",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;

        params.add(
            Params::Rot,
            "rot",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;

        for (p, name) in [
            (Params::StartPercent, "startPercent"),
            (Params::LastPercent, "lastPercent"),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(-300.0);
                    f.set_valid_max(300.0);
                    f.set_slider_min(-50.0);
                    f.set_slider_max(50.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(0.0);
                    f.set_value(f.default());
                }),
            )?;
        }

        for (p, name) in [(Params::OffsetX, "offsetX"), (Params::OffsetY, "offsetY")] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(-2000.0);
                    f.set_valid_max(2000.0);
                    f.set_slider_min(-200.0);
                    f.set_slider_max(200.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(0.0);
                    f.set_value(f.default());
                }),
            )?;
        }

        params.add(
            Params::GuideDraw,
            "guideDraw",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::GuideColor,
            "guideColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0xFF,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_BUTTON("colorTable", ..) と PF_ADD_BUTTON("", ..)。
        // 押すとファイルダイアログを開いて .pref を読み書きするが、ここでは
        // スロットだけ合わせてあり中身は未実装。
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
                out_data.set_return_msg("fs-rs targetgrad");
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

        // 元の GetParams: 有効な色だけ集めて、同じ色は1つにまとめる。
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

        // 元の Exec: mode 1 で色が1つも無ければ何も描かない (出力も触らない)。
        if mode == MODE_COLORS && targets.is_empty() {
            return Ok(());
        }

        let grad8 = params.get(Params::GradColor)?.as_color()?.value();
        let invert = params.get(Params::SwapPoint)?.as_checkbox()?.value();
        let hyperbolic = params.get(Params::Hyperbolic)?.as_float_slider()?.value() as f64;
        let mut start = {
            let (x, y) = params.get(Params::StartPos)?.as_point()?.value();
            Pt {
                x: x as f64,
                y: y as f64,
            }
        };
        let mut last = {
            let (x, y) = params.get(Params::LastPos)?.as_point()?.value();
            Pt {
                x: x as f64,
                y: y as f64,
            }
        };
        if invert {
            std::mem::swap(&mut start, &mut last);
        }
        let auto_pos = params.get(Params::AutoPos)?.as_checkbox()?.value();
        let rot = params.get(Params::Rot)?.as_angle()?.value() as f64;
        let start_per = params.get(Params::StartPercent)?.as_float_slider()?.value() / 100.0;
        let last_per = params.get(Params::LastPercent)?.as_float_slider()?.value() / 100.0;
        let off_x = params.get(Params::OffsetX)?.as_float_slider()?.value();
        let off_y = params.get(Params::OffsetY)?.as_float_slider()?.value();
        let guide = params.get(Params::GuideDraw)?.as_checkbox()?.value();
        let guide8 = params.get(Params::GuideColor)?.as_color()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth || w == 0 || h == 0 {
            return Err(Error::BadCallbackParameter);
        }

        // 1. 抽出パス。mode 3 (all) では元は走らせない。
        //    外接矩形の初期値も元のまま (幅+8 / -8)。
        let mut area = (w as f64 + 8.0, h as f64 + 8.0, -8.0, -8.0);
        let mut keep = vec![true; w * h];
        if mode != MODE_ALL {
            let (mut l, mut t, mut r, mut b) = area;
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
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
                    keep[i] = hit;
                    // 元は残した画素をそのまま出力に書き、それ以外は 0 にする。
                    // そのあと外接矩形は「出力の alpha > 0」で取る。
                    let alpha_after = hit && alpha_positive;
                    if alpha_after {
                        let (xf, yf) = (x as f64, y as f64);
                        if xf < l {
                            l = xf;
                        }
                        if xf > r {
                            r = xf;
                        }
                        if yf < t {
                            t = yf;
                        }
                        if yf > b {
                            b = yf;
                        }
                    }
                }
            }
            area = (l, t, r, b);
        }

        // 2. 線の位置を決める。
        let rp = if auto_pos {
            RotPoint::from_rect(area, rot, start_per, last_per, off_x, off_y)
        } else {
            RotPoint::from_two_points(start, last, start_per, last_per, off_x, off_y)
        };

        // 3. グラデーション。元は iterate_origin なので座標に原点が足される。
        let origin = in_data.output_origin();
        let (ox, oy) = (origin.h as f64, origin.v as f64);
        let p1 = rp.start2d;
        let dx = (rp.last2d.x - p1.x) as f32;
        let dy = (rp.last2d.y - p1.y) as f32;
        let len_sq = dx * dx + dy * dy;
        // 元: 長さがほぼ 0 なら 1 にする。
        let inv_len_sq = if len_sq < 0.0001 {
            1.0f32
        } else {
            1.0 / len_sq
        };
        let is_all = mode == MODE_ALL;
        let max = match depth {
            8 => MAX_CHANNEL8 as f32,
            16 => MAX_CHANNEL16 as f32,
            _ => 1.0,
        };
        let grad16 = conv_8_to_16(&grad8);
        let grad32 = conv_8_to_32(&grad8);

        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                // mode 3 以外では、抽出で消えた画素は触らない (元は out->alpha==0
                // を見て早期リターンする)。
                let alive = if is_all { true } else { keep[i] };
                if !alive {
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
                // 元は f32 で計算する。
                let px = (x as f64 + ox) as f32 - p1.x as f32;
                let py = (y as f64 + oy) as f32 - p1.y as f32;
                let mut t = (px * dx + py * dy) * inv_len_sq;
                t = t.clamp(0.0, 1.0);
                let k = hyperbolic as f32;
                let v = 1.0 - ((1.0 + k) * t) / (1.0 + k * t);
                let a = v * max;
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        *p = grad8;
                        p.alpha = a as u8;
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        *p = grad16;
                        p.alpha = a as u16;
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        *p = grad32;
                        p.alpha = a;
                    }
                }
            }
        }

        // 4. ガイド。元は矩形 (autoPos のときだけ) と線を重ねて描く。
        if guide {
            let mut canvas = Canvas {
                layer: &mut out_layer,
                depth,
                w,
                h,
                col8: guide8,
                col16: conv_8_to_16(&guide8),
                col32: conv_8_to_32(&guide8),
            };
            if auto_pos {
                canvas.box_outline(area);
            }
            // 元は同じ線を (x+1) と (y+1) にもずらして3本描いて太らせる。
            let thick = |c: &mut Canvas, a: Pt, b: Pt| {
                let (ax, ay) = (a.x.round() as i64, a.y.round() as i64);
                let (bx, by) = (b.x.round() as i64, b.y.round() as i64);
                c.line(ax, ay, bx, by);
                c.line(ax + 1, ay, bx + 1, by);
                c.line(ax, ay + 1, bx, by + 1);
            };
            thick(&mut canvas, rp.start2, rp.last2);
            thick(&mut canvas, rp.center2, rp.g_pos(rp.center2, 20.0));
            thick(&mut canvas, rp.start2d, rp.g_pos(rp.start2d, 40.0));
            thick(&mut canvas, rp.last2d, rp.g_pos(rp.last2d, 40.0));
        }
        Ok(())
    }
}

/// ガイドを描くための薄いラッパ。元の CFsGraph の setPixelBlend* / Line* /
/// box8 に対応する。ガイド色のアルファは最大なので、元の PixelBlend は
/// 上書きと同じになる。
struct Canvas<'a> {
    layer: &'a mut ae::Layer,
    depth: i16,
    w: usize,
    h: usize,
    col8: ae::Pixel8,
    col16: ae::Pixel16,
    col32: ae::PixelF32,
}

impl Canvas<'_> {
    fn set(&mut self, x: i64, y: i64) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        let (x, y) = (x as usize, y as usize);
        match self.depth {
            8 => *self.layer.as_pixel8_mut(x, y) = self.col8,
            16 => *self.layer.as_pixel16_mut(x, y) = self.col16,
            _ => *self.layer.as_pixel32_mut(x, y) = self.col32,
        }
    }

    /// 元の Line8 (Bresenham)。
    fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64) {
        let sx = if x1 > x0 { 1 } else { -1 };
        let dx = (x1 - x0).abs();
        let sy = if y1 > y0 { 1 } else { -1 };
        let dy = (y1 - y0).abs();
        let (mut x, mut y) = (x0, y0);
        if dx >= dy {
            let mut e = -dx;
            for _ in 0..=dx {
                self.set(x, y);
                x += sx;
                e += 2 * dy;
                if e >= 0 {
                    y += sy;
                    e -= 2 * dx;
                }
            }
        } else {
            let mut e = -dy;
            for _ in 0..=dy {
                self.set(x, y);
                y += sy;
                e += 2 * dx;
                if e >= 0 {
                    x += sx;
                    e -= 2 * dy;
                }
            }
        }
    }

    /// 元の box8。太さ 1 の矩形の枠を描く (lineHeiht の既定が 1)。
    fn box_outline(&mut self, rect: (f64, f64, f64, f64)) {
        let (l, t, r, b) = rect;
        let (mut x0, mut y0, mut x1, mut y1) = (
            l.round() as i64,
            t.round() as i64,
            r.round() as i64,
            b.round() as i64,
        );
        if x0 > x1 {
            std::mem::swap(&mut x0, &mut x1);
        }
        if y0 > y1 {
            std::mem::swap(&mut y0, &mut y1);
        }
        // 上と下
        for x in x0..=x1 {
            self.set(x, y0);
            self.set(x, y1);
        }
        // 左と右 (角は上下で描いたぶんを除く)
        for y in (y0 + 1)..=(y1 - 1) {
            self.set(x0, y);
            self.set(x1, y);
        }
    }
}
