//! F's RimFill の移植。
//!
//! 不透明な絵のまわりの透明な画素を width px ぶん塗って縁取りを足す。
//! 塗る色は CustomColor か、境界の内側にあった画素の色 (AdjacentColor)。
//! "Treat White as Transparent" を入れると、真っ白な画素も透明扱いにする。
//!
//! 元の流れ: 横方向に1行ずつ、続いて縦方向に1列ずつ。
//!   1. 行 (列) を読み出す。白を透明扱いにする指定なら、その場で透明にする
//!   2. 「自分が不透明ではなく、隣が完全不透明」な位置を境界として印をつける
//!      (左向き = 1、右向き = 2。2本目の走査が 1 を 2 で上書きすることがある)
//!   3. 境界から外側へ、**元から透明だった画素の上だけ** width px まで塗る
//!
//! 横の走査は出力に書き込みながら元の行を読む (読みは走査開始時のコピー)。
//! 縦の走査は横の結果を読む。どちらも元のまま。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/RimFill

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_8_to_16, conv_8_to_32};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Width,
    Mode,
    CustomColor,
    White,
}

/// 深度をまたいで1画素を扱うための形。値は 0..1 に正規化した実数ではなく、
/// その深度の生の値 (f64) で持つ。
#[derive(Clone, Copy, PartialEq)]
struct Px {
    alpha: f64,
    red: f64,
    green: f64,
    blue: f64,
}

impl Px {
    const TRANSPARENT: Px = Px {
        alpha: 0.0,
        red: 0.0,
        green: 0.0,
        blue: 0.0,
    };

    /// 元の IsWhitePixel。RGB がすべて最大値以上かどうか (アルファは見ない)。
    fn is_white(&self, max: f64) -> bool {
        self.red >= max && self.green >= max && self.blue >= max
    }
}

/// 元の RimFillH / RimFillV をまとめたもの。`line` は1行 (1列) ぶん。
/// 戻り値は書き換えた結果。
fn fill_line(line: &[Px], width: i64, custom: Option<Px>, treat_white: bool, max: f64) -> Vec<Px> {
    let n = line.len();
    let mut out = line.to_vec();
    if n == 0 {
        return out;
    }

    // 1. 走査用のコピーを作る。白を透明扱いにする指定ならここで透明にする。
    //    (元はコピー側だけ書き換えて、出力はそのまま残す)
    let mut buf = line.to_vec();
    let mut was_transparent = vec![false; n];
    for i in 0..n {
        if treat_white && buf[i].is_white(max) {
            buf[i] = Px::TRANSPARENT;
        }
        was_transparent[i] = buf[i].alpha == 0.0;
    }

    // 2. 境界を探す。右隣が完全不透明なら 1、左隣が完全不透明なら 2。
    //    2本目の走査は 1 を 2 で上書きすることがある (元のまま)。
    let mut dir = vec![0u8; n];
    for i in 0..n.saturating_sub(1) {
        if buf[i].alpha != max && buf[i + 1].alpha == max {
            dir[i] = 1;
        }
    }
    for i in (1..n).rev() {
        if buf[i].alpha != max && buf[i - 1].alpha == max {
            dir[i] = 2;
        }
    }

    // 3. 境界から外側へ塗る。元から透明だった画素に当たっている間だけ進む。
    for i in 0..n {
        if dir[i] == 0 {
            continue;
        }
        let mut col = match custom {
            Some(c) => c,
            // AdjacentColor は走査用コピーから隣の色を取る。
            None => {
                if dir[i] == 1 {
                    buf[i + 1]
                } else {
                    buf[i - 1]
                }
            }
        };
        col.alpha = max;
        if dir[i] == 1 {
            let to = (i as i64 - width).clamp(0, n as i64 - 1) as usize;
            for j in (to..=i).rev() {
                if !was_transparent[j] {
                    break;
                }
                out[j] = col;
            }
        } else {
            let to = (i as i64 + width).clamp(0, n as i64 - 1) as usize;
            for j in i..=to {
                if !was_transparent[j] {
                    break;
                }
                out[j] = col;
            }
        }
    }
    out
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
        // 元: PF_ADD_SLIDER(STR_WIDTH, 0, 200, 0, 10, 0, ID_WIDTH)
        params.add(
            Params::Width,
            "width",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(200);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_POPUP(STR_MODE, 2, 1, "CustomColor|AdjacentColor", ID_MODE)
        params.add(
            Params::Mode,
            "FillMethod",
            ae::PopupDef::setup(|f| {
                f.set_options(&["CustomColor", "AdjacentColor"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_CUSTOMCOLOR, 0, 0, 0, ID_CUSTOMCOLOR)
        params.add(
            Params::CustomColor,
            "CustomColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_CHECKBOX(STR_WHITE, "on", TRUE, 0, ID_WHITE)
        params.add(
            Params::White,
            "Treat White as Transparent",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
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
                out_data.set_return_msg("fs-rs rimfill");
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
        let width = params.get(Params::Width)?.as_slider()?.value() as i64;
        // 元: ポップアップが 1 のときだけ CustomColor。
        let is_custom = params.get(Params::Mode)?.as_popup()?.value() == 1;
        let col8 = params.get(Params::CustomColor)?.as_color()?.value();
        let treat_white = params.get(Params::White)?.as_checkbox()?.value();

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: 幅が 0 以下なら素通し。
        if width <= 0 {
            return Ok(());
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        let max = match depth {
            8 => MAX_CHANNEL8 as f64,
            16 => MAX_CHANNEL16 as f64,
            _ => 1.0,
        };
        let custom = if is_custom {
            let c = match depth {
                8 => Px {
                    alpha: col8.alpha as f64,
                    red: col8.red as f64,
                    green: col8.green as f64,
                    blue: col8.blue as f64,
                },
                16 => {
                    let c = conv_8_to_16(&col8);
                    Px {
                        alpha: c.alpha as f64,
                        red: c.red as f64,
                        green: c.green as f64,
                        blue: c.blue as f64,
                    }
                }
                _ => {
                    let c = conv_8_to_32(&col8);
                    Px {
                        alpha: c.alpha as f64,
                        red: c.red as f64,
                        green: c.green as f64,
                        blue: c.blue as f64,
                    }
                }
            };
            Some(c)
        } else {
            None
        };

        fn read(layer: &ae::Layer, depth: i16, x: usize, y: usize) -> Px {
            match depth {
                8 => {
                    let p = layer.as_pixel8(x, y);
                    Px {
                        alpha: p.alpha as f64,
                        red: p.red as f64,
                        green: p.green as f64,
                        blue: p.blue as f64,
                    }
                }
                16 => {
                    let p = layer.as_pixel16(x, y);
                    Px {
                        alpha: p.alpha as f64,
                        red: p.red as f64,
                        green: p.green as f64,
                        blue: p.blue as f64,
                    }
                }
                _ => {
                    let p = layer.as_pixel32(x, y);
                    Px {
                        alpha: p.alpha as f64,
                        red: p.red as f64,
                        green: p.green as f64,
                        blue: p.blue as f64,
                    }
                }
            }
        }
        let write = |layer: &mut ae::Layer, x: usize, y: usize, p: Px| match depth {
            8 => {
                *layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                    alpha: p.alpha as u8,
                    red: p.red as u8,
                    green: p.green as u8,
                    blue: p.blue as u8,
                }
            }
            16 => {
                *layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                    alpha: p.alpha as u16,
                    red: p.red as u16,
                    green: p.green as u16,
                    blue: p.blue as u16,
                }
            }
            _ => {
                *layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                    alpha: p.alpha as f32,
                    red: p.red as f32,
                    green: p.green as f32,
                    blue: p.blue as f32,
                }
            }
        };

        // 横: 行ごと。
        for y in 0..h {
            let line: Vec<Px> = (0..w).map(|x| read(&out_layer, depth, x, y)).collect();
            let out = fill_line(&line, width, custom, treat_white, max);
            for x in 0..w {
                if out[x] != line[x] {
                    write(&mut out_layer, x, y, out[x]);
                }
            }
        }
        // 縦: 列ごと。横の結果を読む。
        for x in 0..w {
            let line: Vec<Px> = (0..h).map(|y| read(&out_layer, depth, x, y)).collect();
            let out = fill_line(&line, width, custom, treat_white, max);
            for y in 0..h {
                if out[y] != line[y] {
                    write(&mut out_layer, x, y, out[y]);
                }
            }
        }
        Ok(())
    }
}
