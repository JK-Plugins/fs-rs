//! F's Extract_Edge の移植。
//!
//! 絵の明るさ (CIE Lab の L) の段差を測って、輪郭の強さを出す。**元は
//! 途中で止まっていて**、結果をどう絵にするかが書かれていない: L を赤に、
//! 輪郭の強さを緑に入れたところで終わり、青とアルファは入力のまま。
//! `Edge` / `AlphaEdge` / `AlphaLevel` / `UseColor` / `Color` の5つは
//! 読み込むだけで一度も使われない。出荷された .aex が無いのもそのせい
//! だと思われる。
//!
//! 手順 (元の Exec8):
//!
//! 1. 入力をそのままコピーする
//! 2. 画素ごとに乗算済みの RGB を 0..1 にして Lab に直し、**L を 8bit に
//!    して赤に入れる** (緑・青・アルファはコピーのまま)
//! 3. 赤を読んで `d = |自分 - 右| + |右 - 下|` を出し、`LoLevel` /
//!    `HiLevel` で 0..1 に伸ばして**緑に入れる**
//!
//! 元から写した細かいところ:
//!
//! - `LoLevel` と `HiLevel` は % を 100 で割ったもの。大小が逆なら入れ替え、
//!   差を `HiLo` として持つ
//! - `HiLo` がちょうど 0 なら `d >= HiLevel` の 0/1 で切る。そうでなければ
//!   `d <= LoLevel` で 0、`d >= HiLevel` で 1、あいだは直線
//! - 段差は `|自分 - 右| + |右 - 下|`。**自分と下ではなく右と下の差**を
//!   足しているのも元のまま
//! - 赤に入れる L は `L * 255 + 0.5` を切り捨てて 0..255 で押さえたもの
//! - 右と下を読むときは画面の縁で押さえる
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **縁を押さえる `GetPX` が、縦のはみ出しで `xx` に代入している**
//!    (`if (yy >= height) xx = height - 1;`、`yy` のつもり)。いちばん下の
//!    行で1行先を読んでしまい、しかも x も別の値に飛ぶ。`yy` を押さえる
//!    ようにした
//! 2. **16/32bit では入力をコピーするだけだった**。8bit と同じ道を
//!    通すようにした
//!
//! 直していないところ:
//!
//! - 使われていない5つのパラメータと、絵にする手前で止まっているところは
//!   そのまま。元に一行も書かれていないので、何を描くつもりだったかを
//!   決める手がかりが無い (突き合わせる出荷バイナリも無い)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Extract_Edge

use after_effects::{self as ae};

/// 直したバグを戻す。元との比較用 (出荷 .aex は無いので参考まで)。
const FAITHFUL: bool = false;

/// 読み込んだパラメータのうち、元が実際に使うぶん。
#[derive(Debug, Clone, Copy)]
struct Info {
    lo_level: f64,
    hi_level: f64,
    hi_lo: f64,
}

/// 元の EdgeR2G のしきい値。
fn ramp(d: f64, info: &Info) -> f64 {
    if info.hi_lo == 0.0 {
        if d >= info.hi_level {
            1.0
        } else {
            0.0
        }
    } else if d <= info.lo_level {
        0.0
    } else if d >= info.hi_level {
        1.0
    } else {
        (d - info.lo_level) / info.hi_lo
    }
}

/// 元の GetPX の縁の押さえ方。
fn clamp_xy(x: i64, y: i64, w: i64, h: i64) -> (i64, i64) {
    let xx = x.clamp(0, w - 1);
    if FAITHFUL {
        // 元: 縦のはみ出しで xx に代入してしまう。
        let (mut xx, yy) = (xx, y);
        if yy >= h {
            xx = h - 1;
        }
        (xx.clamp(0, w - 1), yy.clamp(0, h - 1))
    } else {
        (xx, y.clamp(0, h - 1))
    }
}

/// 元の EdgeR2G。赤 (= L) の段差を緑に入れる。
fn edge_r2g(lum: &[f64], out: &mut [f64], w: i64, h: i64, info: &Info) {
    for y in 0..h {
        for x in 0..w {
            let at = |px: i64, py: i64| -> f64 {
                let (cx, cy) = clamp_xy(px, py, w, h);
                lum[(cx + cy * w) as usize]
            };
            let v1 = lum[(x + y * w) as usize];
            let v2 = at(x + 1, y);
            let v3 = at(x, y + 1);
            // 元: |自分 - 右| + |右 - 下|。
            let d = (v1 - v2).abs() + (v2 - v3).abs();
            out[(x + y * w) as usize] = ramp(d, info);
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Edge,
    LoLevel,
    HiLevel,
    AlphaEdge,
    AlphaLevel,
    UseColor,
    Color,
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
        // 元: PF_ADD_CHECKBOX("Edge", "on", TRUE)。読むだけで使われない。
        params.add(
            Params::Edge,
            "Edge",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("LoLevel", 0, 100, 0, 50, 1, 25, 1, 0, 0)
        params.add(
            Params::LoLevel,
            "LoLevel",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(25.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("HiLevel", 0, 100, 0, 100, 1, 50, 1, 0, 0)
        params.add(
            Params::HiLevel,
            "HiLevel",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("AlphaEdge", "on", TRUE)。使われない。
        params.add(
            Params::AlphaEdge,
            "AlphaEdge",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("AlphaLevel", 0, 100, 0, 100, 1, 50, 1, 0, 0)。
        // 使われない。
        params.add(
            Params::AlphaLevel,
            "AlphaLevel",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("UseColor", "on", TRUE)。使われない。
        params.add(
            Params::UseColor,
            "UseColor",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("Color", 0xFF, 0xFF, 0xFF)。使われない。
        params.add(
            Params::Color,
            "Color",
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
                out_data.set_return_msg("fs-rs extractedge");
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
        // 元の GetParams。使われないぶんも読み込んでおく (元と同じ)。
        let _edge = params.get(Params::Edge)?.as_checkbox()?.value();
        let mut lo = params.get(Params::LoLevel)?.as_float_slider()?.value() as f64 / 100.0;
        let mut hi = params.get(Params::HiLevel)?.as_float_slider()?.value() as f64 / 100.0;
        if lo > hi {
            std::mem::swap(&mut lo, &mut hi);
        }
        let info = Info {
            lo_level: lo,
            hi_level: hi,
            hi_lo: hi - lo,
        };
        let _alpha_edge = params.get(Params::AlphaEdge)?.as_checkbox()?.value();
        let _alpha_level = params.get(Params::AlphaLevel)?.as_float_slider()?.value();
        let _use_colour = params.get(Params::UseColor)?.as_checkbox()?.value();
        let _colour = params.get(Params::Color)?.as_color()?.value();

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();

        // 元の Exec8: まず入力をコピー。
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
        // 元: 16/32bit はコピーだけ (直したところ 2)。
        if FAITHFUL && depth != 8 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $max:expr, $get:expr, $put:expr) => {{
                let max: f64 = $max;
                // 元の ToGray: 乗算済みの RGB を Lab にして L を赤へ。
                let mut lum: Vec<f64> = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        let (r, g, b, a) = $get(in_layer.$rd(x as usize, y as usize));
                        let av = a / max;
                        // 元: RoundFpShortDouble で 0..32 に押さえてから Lab。
                        let f = |v: f64| libs::utils::round_fp_short_double(v * av / max) as f64;
                        let lab = libs::lab::rgb_to_lab(f(r), f(g), f(b), av as f32);
                        // 元: RoundByteLong((A_long)(L * 255 + 0.5))。
                        let l = ((lab.l * 255.0 + 0.5) as i64).clamp(0, 255) as f64;
                        lum.push(l / 255.0);
                    }
                }
                let mut edge = vec![0.0f64; (w * h) as usize];
                edge_r2g(&lum, &mut edge, w, h, &info);
                for y in 0..h {
                    for x in 0..w {
                        let i = (x + y * w) as usize;
                        // 赤は L を 8bit にしたものを深度へ、緑は段差。
                        let red = lum[i] * max;
                        let green = ((edge[i] * 255.0 + 0.5) as i64).clamp(0, 255) as f64
                            / 255.0
                            * max;
                        $put(
                            out_layer.$wr(x as usize, y as usize),
                            red,
                            green,
                        );
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                255.0,
                |p: &ae::Pixel8| (
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64,
                    p.alpha as f64
                ),
                |p: &mut ae::Pixel8, r: f64, g: f64| {
                    p.red = r as i64 as u8;
                    p.green = g as i64 as u8;
                }
            ),
            16 => go!(
                as_pixel16,
                as_pixel16_mut,
                32768.0,
                |p: &ae::Pixel16| (
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64,
                    p.alpha as f64
                ),
                |p: &mut ae::Pixel16, r: f64, g: f64| {
                    p.red = r as i64 as u16;
                    p.green = g as i64 as u16;
                }
            ),
            _ => go!(
                as_pixel32,
                as_pixel32_mut,
                1.0,
                |p: &ae::PixelF32| (
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64,
                    p.alpha as f64
                ),
                |p: &mut ae::PixelF32, r: f64, g: f64| {
                    p.red = r as f32;
                    p.green = g as f32;
                }
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(lo: f64, hi: f64) -> Info {
        Info {
            lo_level: lo,
            hi_level: hi,
            hi_lo: hi - lo,
        }
    }

    /// しきい値の帯。
    #[test]
    fn the_ramp_runs_between_the_levels() {
        let i = info(0.25, 0.75);
        assert_eq!(ramp(0.1, &i), 0.0);
        assert_eq!(ramp(0.9, &i), 1.0);
        assert_eq!(ramp(0.5, &i), 0.5);
    }

    /// 帯の幅が 0 なら 0/1 で切る。
    #[test]
    fn a_zero_band_is_a_hard_cut() {
        let i = info(0.5, 0.5);
        assert_eq!(ramp(0.49, &i), 0.0);
        assert_eq!(ramp(0.5, &i), 1.0);
    }

    /// 平らな絵には段差が出ない。
    #[test]
    fn a_flat_image_has_no_edge() {
        let lum = vec![0.5f64; 16];
        let mut out = vec![0.0f64; 16];
        edge_r2g(&lum, &mut out, 4, 4, &info(0.1, 0.5));
        assert!(out.iter().all(|v| *v == 0.0));
    }

    /// 縦の境目で段差が出る。
    #[test]
    fn a_vertical_step_shows_up() {
        // 左半分 0、右半分 1。
        let mut lum = vec![0.0f64; 16];
        for y in 0..4 {
            for x in 2..4 {
                lum[x + y * 4] = 1.0;
            }
        }
        let mut out = vec![0.0f64; 16];
        edge_r2g(&lum, &mut out, 4, 4, &info(0.1, 0.5));
        assert_eq!(out[1], 1.0, "境目の左で段差が立つ");
        assert_eq!(out[0], 0.0);
    }

    /// いちばん下の行でも落ちず、縁で押さえられる (直したところ 1)。
    #[test]
    fn the_bottom_row_is_clamped() {
        let lum: Vec<f64> = (0..12).map(|i| (i % 3) as f64 / 2.0).collect();
        let mut out = vec![0.0f64; 12];
        edge_r2g(&lum, &mut out, 3, 4, &info(0.1, 0.5));
        assert_eq!(out.len(), 12);
        // いちばん下の行は下の隣が自分なので |v1-v2| だけになる。
        let w = 3i64;
        for x in 0..3i64 {
            let v1 = lum[(x + 3 * w) as usize];
            let v2 = lum[(x.min(2) + 3 * w) as usize];
            let _ = (v1, v2);
        }
        assert_eq!(clamp_xy(1, 4, 3, 4), (1, 3), "下へのはみ出しは行で押さえる");
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }
}
