//! F's ChromaticAberration の移植。
//!
//! 中心からの距離に RGB 別の倍率をかけて拾い直し、色収差 (レンズの色ずれ) を
//! 作る。赤・緑・青をそれぞれ別の倍率で縮めたり広げたりして混ぜる。
//!
//! 元から写した細かいところ:
//!
//! - 拾う位置は `(x - cx) * 倍率 + cx`。倍率は % を 100 で割った値
//! - 拾い方はバイリニア (`getPixelFloat8`)。整数部は 0 方向への切り捨て、
//!   画像の外は端に寄せて読む。混ぜたあと +0.5 して切り捨て
//! - アルファは3つの標本を順に重ねる
//!   (`a + a2 - a * a2 / 255`、整数の割り算で切り捨て)
//! - 元は最後に出力の黒マットを外そうとするが、出力は一度も黒マットに
//!   していないので、状態 (`m_mat`) を見る元の `fromBlackMat*` は何もしない
//!
//! **修正したバグ**: 元は黒マットをかけた控え (`tmp`) を用意して、反復の
//! 入力にもそれを渡しているのに、**拾うときは `ae->in` (生の入力) を見ている**。
//! 控えは一度も読まれない。最後の `fromBlackMat*` も、出力が黒マット状態に
//! なっていないので何もしない。つまり**半透明のところは前後の重なりが
//! 混ざらないまま**になる。控えから拾って、最後にちゃんと外すようにした。
//! 全面不透明な絵では違いが出ない。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ChromaticAberration

use after_effects::{self as ae};
use libs::fs_graphics8::{round_byte_long, Image16, Image8, Px16, Px8, MAX16, MAX8};
use libs::fs_graphics::{Image, PixelF};

/// 元の getPixelFloat8 / 16 / 32 のバイリニア。`at` は端に寄せて読む。
fn sample(at: impl Fn(i64, i64) -> [f64; 4], x: f64, y: f64) -> [f64; 4] {
    // 元: (A_long)x なので 0 方向への切り捨て。
    let x0 = x as i64;
    let y0 = y as i64;
    let dx1 = x - x0 as f64;
    let dx0 = 1.0 - dx1;
    let dy1 = y - y0 as f64;
    let dy0 = 1.0 - dy1;
    let p = [
        at(x0, y0),
        at(x0 + 1, y0),
        at(x0, y0 + 1),
        at(x0 + 1, y0 + 1),
    ];
    let mut out = [0.0f64; 4];
    for (c, o) in out.iter_mut().enumerate() {
        *o = p[0][c] * dx0 * dy0
            + p[1][c] * dx1 * dy0
            + p[2][c] * dx0 * dy1
            + p[3][c] * dx1 * dy1;
    }
    out
}

/// 元のアルファの重ね方。`a + b - a*b/max` を整数の割り算で。
fn over_alpha(a: i64, b: i64, max: i64) -> i64 {
    a + b - a * b / max
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Red,
    Green,
    Blue,
    Center,
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
        // 元: PF_ADD_FLOAT_SLIDER(名前, 0, 150, 90, 110, 1, 既定, 1, 0, 0, ..)
        for (p, name, dflt) in [
            (Params::Red, "red", 98.0),
            (Params::Green, "green", 100.0),
            (Params::Blue, "blue", 102.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(150.0);
                    f.set_slider_min(90.0);
                    f.set_slider_max(110.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_POINT(STR_CENTER, 50, 50, 0, ..)
        params.add(
            Params::Center,
            "center",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
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
                out_data.set_return_msg("fs-rs chromaticaberration");
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
        _in_data: &ae::InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        // 元の GetParams: % は 100 で割る。中心は固定小数を実数に。
        let kr = params.get(Params::Red)?.as_float_slider()?.value() / 100.0;
        let kg = params.get(Params::Green)?.as_float_slider()?.value() / 100.0;
        let kb = params.get(Params::Blue)?.as_float_slider()?.value() / 100.0;
        let c = params.get(Params::Center)?.as_point()?.value();
        let (cx, cy) = (c.0 as f64, c.1 as f64);

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        match depth {
            8 => {
                let mut raw = Image8::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let p = in_layer.as_pixel8(x, y);
                        *raw.get_mut(x, y) = Px8::new(p.alpha, p.red, p.green, p.blue);
                    }
                }
        // 元はここで黒マットをかけた控えを用意しておきながら、拾うときは
        // 生の入力を見てしまう (下の説明を参照)。控えから拾うようにした。
        let mut src = raw.clone();
        src.to_black_mat8();
                let at = |x: i64, y: i64| -> [f64; 4] {
                    // 元の getPixel8_F: 端に寄せて読む。
                    let xx = x.clamp(0, w as i64 - 1) as usize;
                    let yy = y.clamp(0, h as i64 - 1) as usize;
                    let p = src.get(xx, yy);
                    [
                        p.alpha as f64,
                        p.red as f64,
                        p.green as f64,
                        p.blue as f64,
                    ]
                };
                let mut dst = Image8::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f64 - cx;
                        let dy = y as f64 - cy;
                        let pick = |k: f64| sample(&at, dx * k + cx, dy * k + cy);
                        let sr = pick(kr);
                        let sg = pick(kg);
                        let sb = pick(kb);
                        // 元: 混ぜたあと +0.5 して切り捨て、0..255 でクランプ。
                        let q = |v: f64| round_byte_long((v + 0.5) as i32);
                        let mut a = q(sr[0]) as i64;
                        a = over_alpha(a, q(sg[0]) as i64, MAX8 as i64);
                        a = round_byte_long(a as i32) as i64;
                        a = over_alpha(a, q(sb[0]) as i64, MAX8 as i64);
                        a = round_byte_long(a as i32) as i64;
                        *dst.get_mut(x, y) = Px8::new(a as u8, q(sr[1]), q(sg[2]), q(sb[3]));
                    }
                }
                // 控えから拾ったので掛かっている。ここで外す。
                dst.from_black_mat8();
                for y in 0..h {
                    for x in 0..w {
                        let p = dst.get(x, y);
                        *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                            alpha: p.alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        };
                    }
                }
            }
            16 => {
                let mut raw = Image16::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let p = in_layer.as_pixel16(x, y);
                        *raw.get_mut(x, y) = Px16::new(p.alpha, p.red, p.green, p.blue);
                    }
                }
        // 元はここで黒マットをかけた控えを用意しておきながら、拾うときは
        // 生の入力を見てしまう (下の説明を参照)。控えから拾うようにした。
        let mut src = raw.clone();
        src.to_black_mat16();
                let at = |x: i64, y: i64| -> [f64; 4] {
                    let xx = x.clamp(0, w as i64 - 1) as usize;
                    let yy = y.clamp(0, h as i64 - 1) as usize;
                    let p = src.get(xx, yy);
                    [
                        p.alpha as f64,
                        p.red as f64,
                        p.green as f64,
                        p.blue as f64,
                    ]
                };
                let mut dst = Image16::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f64 - cx;
                        let dy = y as f64 - cy;
                        let pick = |k: f64| sample(&at, dx * k + cx, dy * k + cy);
                        let sr = pick(kr);
                        let sg = pick(kg);
                        let sb = pick(kb);
                        let q = |v: f64| libs::fs_graphics8::round_short_i32((v + 0.5) as i32);
                        let mut a = q(sr[0]) as i64;
                        a = over_alpha(a, q(sg[0]) as i64, MAX16 as i64);
                        a = libs::fs_graphics8::round_short_i32(a as i32) as i64;
                        a = over_alpha(a, q(sb[0]) as i64, MAX16 as i64);
                        a = libs::fs_graphics8::round_short_i32(a as i32) as i64;
                        *dst.get_mut(x, y) = Px16::new(a as u16, q(sr[1]), q(sg[2]), q(sb[3]));
                    }
                }
                // 控えから拾ったので掛かっている。ここで外す。
                dst.from_black_mat16();
                for y in 0..h {
                    for x in 0..w {
                        let p = dst.get(x, y);
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha: p.alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        };
                    }
                }
            }
            _ => {
                let mut raw = Image::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let p = in_layer.as_pixel32(x, y);
                        *raw.get_mut(x, y) = PixelF::new(p.alpha, p.red, p.green, p.blue);
                    }
                }
        // 元はここで黒マットをかけた控えを用意しておきながら、拾うときは
        // 生の入力を見てしまう (下の説明を参照)。控えから拾うようにした。
        let mut src = raw.clone();
        src.to_black_mat();
                let at = |x: i64, y: i64| -> [f64; 4] {
                    let xx = x.clamp(0, w as i64 - 1) as usize;
                    let yy = y.clamp(0, h as i64 - 1) as usize;
                    let p = src.get(xx, yy);
                    [
                        p.alpha as f64,
                        p.red as f64,
                        p.green as f64,
                        p.blue as f64,
                    ]
                };
                let mut dst = Image::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let dx = x as f64 - cx;
                        let dy = y as f64 - cy;
                        let pick = |k: f64| sample(&at, dx * k + cx, dy * k + cy);
                        let sr = pick(kr);
                        let sg = pick(kg);
                        let sb = pick(kb);
                        // 32bit は +0.5 せずクランプだけ (元の PF_PixelFloat 版)。
                        let q = |v: f64| v.clamp(0.0, 1.0) as f32;
                        let a = {
                            let a0 = q(sr[0]) as f64;
                            let a1 = a0 + q(sg[0]) as f64 - a0 * q(sg[0]) as f64;
                            (a1 + q(sb[0]) as f64 - a1 * q(sb[0]) as f64).clamp(0.0, 1.0) as f32
                        };
                        *dst.get_mut(x, y) = PixelF::new(a, q(sr[1]), q(sg[2]), q(sb[3]));
                    }
                }
                // 控えから拾ったので掛かっている。ここで外す。
                dst.from_black_mat();
                for y in 0..h {
                    for x in 0..w {
                        let p = dst.get(x, y);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_picks_the_exact_pixel_on_a_whole_coordinate() {
        let at = |x: i64, y: i64| -> [f64; 4] { [0.0, (x * 10 + y) as f64, 0.0, 0.0] };
        assert_eq!(sample(&at, 3.0, 4.0)[1], 34.0);
    }

    #[test]
    fn sample_blends_halfway() {
        let at = |x: i64, _y: i64| -> [f64; 4] { [0.0, if x == 0 { 0.0 } else { 100.0 }, 0.0, 0.0] };
        let v = sample(&at, 0.5, 0.0)[1];
        assert!((v - 50.0).abs() < 1e-9);
    }

    #[test]
    fn over_alpha_screens_the_two_values() {
        // 片方が最大なら最大のまま。
        assert_eq!(over_alpha(255, 100, 255), 255);
        assert_eq!(over_alpha(0, 100, 255), 100);
        // 整数の割り算で切り捨てる。
        assert_eq!(over_alpha(128, 128, 255), 128 + 128 - 128 * 128 / 255);
    }
}
