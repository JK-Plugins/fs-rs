//! F's PixelExtend の移植。
//!
//! 対象色の画素を8方向それぞれの長さだけ引き延ばす。ドット絵の縁を
//! 伸ばして隙間を埋める用途。
//!
//! 元の流れ (8/16bit):
//!   1. 対象色が1色も無ければ入力をそのまま出す
//!   2. tmp = 入力のうち対象色に (Range の許容差で) 一致した画素だけ。
//!      一致したらその「対象色そのもの」(アルファは最大) を置き、
//!      外れたら透明にする
//!   3. 伸ばす長さがどれか1つでも 0 より大きければ:
//!      - out = tmp を8方向に伸ばしたもの。各方向は len 歩まで進んで
//!        最初に見つけた不透明画素を採る。**方向の処理順が後のものが
//!        前のものを上書きする** (元は break せずに次の方向へ進む)
//!      - 1画素の隙間を下方向に埋める
//!      - 入力が「描かない色」に一致する画素を透明にする
//!      そうでなければ out = tmp
//!   4. TargetOnly が切れていれば、何も描かれていない画素に入力を戻す
//!
//! **32bit は元が入力をそのままコピーするだけ** (実装されていない)。
//! ここでも同じにしてある。
//!
//! 隙間埋めのパスは「自分の1つ下の画素」に書き込むので、画素を回る順で
//! 結果が変わる。iterate ではなく自前の行優先ループで走る。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/PixelExtend

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::conv_8_to_16;

/// 元の LEN_COUNT と並び順。名前は「伸ばす向き」で、採る画素は逆側にある。
const DIRS: [(i64, i64); 8] = [
    (0, 1),   // Top: 下から採る
    (-1, 1),  // Top_Right
    (-1, 0),  // Right
    (-1, -1), // Bottom_Right
    (0, -1),  // Bottom
    (1, -1),  // Bottom_Left
    (1, 0),   // Left
    (1, 1),   // Top_Left
];
const COLOR_COUNT: usize = 8;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TargetStart,
    Range,
    TEnabled0,
    TColor0,
    TEnabled1,
    TColor1,
    TEnabled2,
    TColor2,
    TEnabled3,
    TColor3,
    TEnabled4,
    TColor4,
    TEnabled5,
    TColor5,
    TEnabled6,
    TColor6,
    TEnabled7,
    TColor7,
    TargetEnd,
    ExtendStart,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
    ExtendEnd,
    NoDrawStart,
    NEnabled0,
    NColor0,
    NEnabled1,
    NColor1,
    NEnabled2,
    NColor2,
    NEnabled3,
    NColor3,
    NEnabled4,
    NColor4,
    NEnabled5,
    NColor5,
    NEnabled6,
    NColor6,
    NEnabled7,
    NColor7,
    NoDrawEnd,
    TargetOnly,
}

const T_ENABLED: [Params; COLOR_COUNT] = [
    Params::TEnabled0,
    Params::TEnabled1,
    Params::TEnabled2,
    Params::TEnabled3,
    Params::TEnabled4,
    Params::TEnabled5,
    Params::TEnabled6,
    Params::TEnabled7,
];
const T_COLORS: [Params; COLOR_COUNT] = [
    Params::TColor0,
    Params::TColor1,
    Params::TColor2,
    Params::TColor3,
    Params::TColor4,
    Params::TColor5,
    Params::TColor6,
    Params::TColor7,
];
const N_ENABLED: [Params; COLOR_COUNT] = [
    Params::NEnabled0,
    Params::NEnabled1,
    Params::NEnabled2,
    Params::NEnabled3,
    Params::NEnabled4,
    Params::NEnabled5,
    Params::NEnabled6,
    Params::NEnabled7,
];
const N_COLORS: [Params; COLOR_COUNT] = [
    Params::NColor0,
    Params::NColor1,
    Params::NColor2,
    Params::NColor3,
    Params::NColor4,
    Params::NColor5,
    Params::NColor6,
    Params::NColor7,
];
const LEN_PARAMS: [Params; 8] = [
    Params::Top,
    Params::TopRight,
    Params::Right,
    Params::BottomRight,
    Params::Bottom,
    Params::BottomLeft,
    Params::Left,
    Params::TopLeft,
];
/// 出荷 .aex が報告する既定色 (黒, R, G, B, Y, M, C, 白)。Target と NoDraw で同じ。
const COLOR_DEFAULTS: [[u8; 3]; COLOR_COUNT] = [
    [0, 0, 0],
    [255, 0, 0],
    [0, 255, 0],
    [0, 0, 255],
    [255, 255, 0],
    [255, 0, 255],
    [0, 255, 255],
    [255, 255, 255],
];

/// 画素を深度そのままの値で持つ ([alpha, red, green, blue])。
/// 元は 8bit と 16bit で別々の表と許容差を使う (colTable16 / range16) ので、
/// ここも深度の目盛りで比べる。8bit に落として比べると境界でずれる。
type Px = [i32; 4];

/// 元の compPix*Lv。RGB の差がすべて range 以内か (アルファは見ない)。
fn close(a: &Px, b: &Px, range: i32) -> bool {
    (a[3] - b[3]).abs() <= range && (a[2] - b[2]).abs() <= range && (a[1] - b[1]).abs() <= range
}

/// 元の InColor*。一致した色の添字、無ければ None。
fn in_color(table: &[Px], c: &Px, range: i32) -> Option<usize> {
    table.iter().position(|t| close(t, c, range))
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
        params.add_group(
            Params::TargetStart,
            Params::TargetEnd,
            "Target",
            false,
            |params| {
                // 元: PF_ADD_FLOAT_SLIDER(STR_RANGE, 0, 100, 0, 100, 1, 0, 1, 0, 0, ..)
                params.add(
                    Params::Range,
                    "Range",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(0.0);
                        f.set_value(f.default());
                    }),
                )?;
                for i in 0..COLOR_COUNT {
                    let rgb = COLOR_DEFAULTS[i];
                    params.add(
                        T_ENABLED[i],
                        &format!("TargetColorEnabled{i}"),
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    params.add(
                        T_COLORS[i],
                        &format!("TargetColor{i}"),
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

        params.add_group(
            Params::ExtendStart,
            Params::ExtendEnd,
            "Extend",
            false,
            |params| {
                for (p, name) in [
                    (Params::Top, "Top"),
                    (Params::TopRight, "Top_Right"),
                    (Params::Right, "Right"),
                    (Params::BottomRight, "Bottom_Right"),
                    (Params::Bottom, "Bottom"),
                    (Params::BottomLeft, "Bottom_Left"),
                    (Params::Left, "Left"),
                    (Params::TopLeft, "Top_Left"),
                ] {
                    params.add(
                        p,
                        name,
                        ae::SliderDef::setup(|f| {
                            f.set_valid_min(0);
                            f.set_valid_max(8);
                            f.set_slider_min(0);
                            f.set_slider_max(8);
                            f.set_default(0);
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;

        params.add_group(
            Params::NoDrawStart,
            Params::NoDrawEnd,
            "NoDraw",
            false,
            |params| {
                for i in 0..COLOR_COUNT {
                    let rgb = COLOR_DEFAULTS[i];
                    params.add(
                        N_ENABLED[i],
                        &format!("NoneColorEnabled{i}"),
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(false);
                            f.set_value(f.default());
                        }),
                    )?;
                    params.add(
                        N_COLORS[i],
                        &format!("NoneColor{i}"),
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
            Params::TargetOnly,
            "TargetOnly",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
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
                out_data.set_return_msg("fs-rs pixelextend");
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

        // 元は 32bit では入力をコピーするだけ (未実装)。
        if depth == 32 {
            return Ok(());
        }

        // 元の GetParams: Range は深度の最大値を掛けて +0.5 で丸める。
        // 色は重複を省き (許容差 0 で判定)、アルファを最大に上書きする。
        let max = match depth {
            8 => MAX_CHANNEL8 as i32,
            _ => MAX_CHANNEL16 as i32,
        };
        let range_pct = params.get(Params::Range)?.as_float_slider()?.value() as f64;
        let range = (max as f64 * range_pct / 100.0 + 0.5) as i32;

        // 色は 8bit で取って深度の目盛りに直す (元の colTable16 は CONV8TO16)。
        let to_px = |c: &ae::Pixel8| -> Px {
            match depth {
                8 => [c.alpha as i32, c.red as i32, c.green as i32, c.blue as i32],
                _ => {
                    let q = conv_8_to_16(c);
                    [q.alpha as i32, q.red as i32, q.green as i32, q.blue as i32]
                }
            }
        };
        let mut targets: Vec<Px> = Vec::with_capacity(COLOR_COUNT);
        for i in 0..COLOR_COUNT {
            if !params.get(T_ENABLED[i])?.as_checkbox()?.value() {
                continue;
            }
            let c = params.get(T_COLORS[i])?.as_color()?.value();
            let mut px = to_px(&c);
            if in_color(&targets, &px, 0).is_none() {
                px[0] = max;
                targets.push(px);
            }
        }
        // 元: 対象色が無ければ入力をそのまま出す。
        if targets.is_empty() {
            return Ok(());
        }
        let mut nones: Vec<Px> = Vec::with_capacity(COLOR_COUNT);
        for i in 0..COLOR_COUNT {
            if !params.get(N_ENABLED[i])?.as_checkbox()?.value() {
                continue;
            }
            let c = params.get(N_COLORS[i])?.as_color()?.value();
            let mut px = to_px(&c);
            if in_color(&nones, &px, 0).is_none() {
                px[0] = max;
                nones.push(px);
            }
        }
        let mut lens = [0i64; 8];
        for i in 0..8 {
            lens[i] = params.get(LEN_PARAMS[i])?.as_slider()?.value() as i64;
        }
        let go_expand = lens.iter().any(|&l| l > 0);
        let target_only = params.get(Params::TargetOnly)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        if w == 0 || h == 0 {
            return Ok(());
        }

        let input = |x: usize, y: usize| -> Px {
            if y >= in_h || x >= in_w {
                return [0, 0, 0, 0];
            }
            match depth {
                8 => {
                    let p = in_layer.as_pixel8(x, y);
                    [p.alpha as i32, p.red as i32, p.green as i32, p.blue as i32]
                }
                _ => {
                    let p = in_layer.as_pixel16(x, y);
                    [p.alpha as i32, p.red as i32, p.green as i32, p.blue as i32]
                }
            }
        };

        // 元の TargetPixelFrom: 対象色に一致したらその色、外れたら
        // 「RGB が最大値でアルファ 0」。
        let clear: Px = [0, max, max, max];
        let mut tmp = vec![clear; w * h];
        for y in 0..h {
            for x in 0..w {
                let c = input(x, y);
                tmp[y * w + x] = match in_color(&targets, &c, range) {
                    Some(i) => targets[i],
                    None => clear,
                };
            }
        }

        // 元は out をクリア (全0) してから書く。
        let mut out = vec![[0i32; 4]; w * h];

        if go_expand {
            // 元の ExpandPixel: 自分が不透明ならそのまま、そうでなければ
            // 8方向を順に試す。後の方向が前の方向を上書きする。
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if tmp[i][0] == max {
                        out[i] = tmp[i];
                        continue;
                    }
                    for (d, &(dx, dy)) in DIRS.iter().enumerate() {
                        let len = lens[d];
                        if len <= 0 {
                            continue;
                        }
                        let (mut cx, mut cy) = (x as i64, y as i64);
                        for _ in 0..len {
                            cx += dx;
                            cy += dy;
                            // 元は進む向きの軸だけ範囲を見る。
                            let inside = (dx >= 0 || cx >= 0)
                                && (dx <= 0 || cx < w as i64)
                                && (dy >= 0 || cy >= 0)
                                && (dy <= 0 || cy < h as i64);
                            if !inside {
                                break;
                            }
                            let t = tmp[cy as usize * w + cx as usize];
                            if t[0] == max {
                                out[i] = t;
                                break;
                            }
                        }
                    }
                }
            }

            // 元の PixelChk: 1画素の隙間を下に埋める。自分の1つ下に書くので
            // 行優先で順に処理する。
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if out[i][0] != max || y + 1 >= h {
                        continue;
                    }
                    let below = (y + 1) * w + x;
                    if out[below][0] == max {
                        continue;
                    }
                    if x == 0 || out[below - 1][0] != max {
                        continue;
                    }
                    if x + 1 >= w || out[below + 1][0] != max {
                        continue;
                    }
                    if y + 2 >= h {
                        continue;
                    }
                    if out[(y + 2) * w + x][0] == max {
                        out[below] = out[i];
                    }
                }
            }

            // 元の NoDrawPixel: 入力が「描かない色」に一致する画素を消す。
            if !nones.is_empty() {
                for y in 0..h {
                    for x in 0..w {
                        let c = input(x, y);
                        if in_color(&nones, &c, range).is_some() {
                            out[y * w + x] = clear;
                        }
                    }
                }
            }
        } else {
            // 元: 伸ばさないなら tmp をそのままコピーする。
            out.copy_from_slice(&tmp);
        }

        // 元の TranCopy: 描かれていない画素には入力を戻す (TargetOnly が
        // 入っているときは戻さない)。copy_hq で入力が入っているので、
        // 戻す場合は何も書かずに残せばよい。
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let drawn = out[i][0] >= max;
                if !drawn && !target_only {
                    continue;
                }
                let p = out[i];
                match depth {
                    8 => {
                        *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                            alpha: p[0] as u8,
                            red: p[1] as u8,
                            green: p[2] as u8,
                            blue: p[3] as u8,
                        }
                    }
                    _ => {
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha: p[0] as u16,
                            red: p[1] as u16,
                            green: p[2] as u16,
                            blue: p[3] as u16,
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
