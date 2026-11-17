//! F's MaxFast の移植。
//!
//! 最大値フィルタ (膨張) と最小値フィルタ (収縮) を、van Herk / Gil-Werman
//! 法で半径に依らない速さで掛ける。パラメータは `max` ひとつだけで、
//! 正なら膨張、負なら収縮、0 なら素通し。
//!
//! 手順 (元の MaxFast_SubImpl):
//!
//! 1. 入力をそのままコピーする
//! 2. 半径に downsample を掛けて整数に丸める。0 なら何もしない
//! 3. **乗算済みにする** (`MaxMult`)。アルファが 0 の画素は RGB も 0 に、
//!    アルファが最大の画素はそのまま
//! 4. 横に1行ずつ、続けて縦に1列ずつ、半径ぶんの窓で最大 (または最小) の
//!    画素を選んで置き換える
//! 5. **乗算を戻す** (`MaxUnMult`)
//!
//! 「最大」の測り方は、RGB の重みつき和を 10 倍したものにアルファの
//! 0..1 を足した値 (元の `lineLevel`):
//!
//! ```text
//! (red*0.299 + green*0.587 + blue*0.114) * 10 + alpha / maxChan
//! ```
//!
//! つまりほぼ明るさ順で、同じ明るさならアルファの高いほうが勝つ。選ばれた
//! 画素は RGBA まとめて持ってくるので、色は混ざらない。
//!
//! 元から写した細かいところ:
//!
//! - 重みつき和は **double で計算してから float に落とす** (元の
//!   `(float)red * 0.299` は double に昇格する)。f32 のまま計算すると
//!   丸め境界で選ばれる画素が変わる
//! - 窓が画面より広いとき (半径 ≥ 幅/高さ) は、その行・列の最大の画素で
//!   全部を塗る。走査は先頭から、比較は**狭義**なので同点なら先頭が勝つ
//! - ブロックの前向き・後ろ向きの表は同点のとき前の値を引き継ぐ
//!   (最大は `>=`、最小は `<=`)。前向きは早い位置、後ろ向きは遅い位置が
//!   残るので、同点の画素が違う色だとここで差が出る
//! - 乗算と乗算戻しのキャストは丸めずに切り捨てる。半透明の画素では
//!   往復で値がわずかに下がる
//! - 縦の走査は横の結果の上で動く (元も同じ世界を続けて潰している)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MaxFast

use after_effects::{self as ae};
use libs::minmax_vhgw::{level, line_pass};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Max,
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
        // 元: PF_ADD_SLIDER("max", -2000, 2000, -200, 200, 0)
        params.add(
            Params::Max,
            "max",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-2000);
                f.set_valid_max(2000);
                f.set_slider_min(-200);
                f.set_slider_max(200);
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
                out_data.set_return_msg("fs-rs maxfast");
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
        // 元の GetParams: 負なら符号を落として「最小値フィルタ」にする。
        let v = params.get(Params::Max)?.as_slider()?.value() as i64;
        let is_max = v >= 0;
        let ds = in_data.downsample_x();
        // 元: 符号を落としたあとに downsample を掛けて丸める。
        let radius = (v.abs() as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if radius == 0 || w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! run {
            ($wr:ident, $max_chan:expr, $mult:expr, $unmult:expr, $lev:expr) => {{
                let max_chan: f32 = $max_chan;
                // 元の MaxMult。
                for y in 0..h {
                    for x in 0..w {
                        let p = out_layer.$wr(x, y);
                        $mult(p);
                    }
                }
                // 横に1行ずつ。
                let mut src = Vec::with_capacity(w);
                let mut levels = Vec::with_capacity(w);
                let mut dst = Vec::with_capacity(w);
                for y in 0..h {
                    src.clear();
                    levels.clear();
                    for x in 0..w {
                        let p = *out_layer.$wr(x, y);
                        levels.push($lev(&p, max_chan));
                        src.push(p);
                    }
                    dst.clear();
                    dst.extend_from_slice(&src);
                    line_pass(&src, &levels, &mut dst, radius, is_max);
                    for x in 0..w {
                        *out_layer.$wr(x, y) = dst[x];
                    }
                }
                // 続けて縦に1列ずつ。
                let mut src = Vec::with_capacity(h);
                let mut levels = Vec::with_capacity(h);
                let mut dst = Vec::with_capacity(h);
                for x in 0..w {
                    src.clear();
                    levels.clear();
                    for y in 0..h {
                        let p = *out_layer.$wr(x, y);
                        levels.push($lev(&p, max_chan));
                        src.push(p);
                    }
                    dst.clear();
                    dst.extend_from_slice(&src);
                    line_pass(&src, &levels, &mut dst, radius, is_max);
                    for y in 0..h {
                        *out_layer.$wr(x, y) = dst[y];
                    }
                }
                // 元の MaxUnMult。
                for y in 0..h {
                    for x in 0..w {
                        let p = out_layer.$wr(x, y);
                        $unmult(p);
                    }
                }
            }};
        }

        match depth {
            8 => run!(
                as_pixel8_mut,
                255.0,
                |p: &mut ae::Pixel8| {
                    if p.alpha == 0 {
                        p.red = 0;
                        p.green = 0;
                        p.blue = 0;
                    } else if p.alpha < 255 {
                        let v = p.alpha as f32 / 255.0;
                        // 元: (A_u_char)(float) なので切り捨て。
                        p.red = (p.red as f32 * v) as i32 as u8;
                        p.green = (p.green as f32 * v) as i32 as u8;
                        p.blue = (p.blue as f32 * v) as i32 as u8;
                    }
                },
                |p: &mut ae::Pixel8| {
                    if p.alpha == 0 {
                        p.red = 0;
                        p.green = 0;
                        p.blue = 0;
                    } else if p.alpha < 255 {
                        let v = 255.0f32 / p.alpha as f32;
                        p.red = (p.red as f32 * v) as i32 as u8;
                        p.green = (p.green as f32 * v) as i32 as u8;
                        p.blue = (p.blue as f32 * v) as i32 as u8;
                    }
                },
                |p: &ae::Pixel8, mc: f32| level(
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64,
                    p.alpha as f32,
                    mc
                )
            ),
            16 => run!(
                as_pixel16_mut,
                32768.0,
                |p: &mut ae::Pixel16| {
                    if p.alpha == 0 {
                        p.red = 0;
                        p.green = 0;
                        p.blue = 0;
                    } else if p.alpha < 32768 {
                        let v = p.alpha as f32 / 32768.0;
                        p.red = (p.red as f32 * v) as i32 as u16;
                        p.green = (p.green as f32 * v) as i32 as u16;
                        p.blue = (p.blue as f32 * v) as i32 as u16;
                    }
                },
                |p: &mut ae::Pixel16| {
                    if p.alpha == 0 {
                        p.red = 0;
                        p.green = 0;
                        p.blue = 0;
                    } else if p.alpha < 32768 {
                        let v = 32768.0f32 / p.alpha as f32;
                        p.red = (p.red as f32 * v) as i32 as u16;
                        p.green = (p.green as f32 * v) as i32 as u16;
                        p.blue = (p.blue as f32 * v) as i32 as u16;
                    }
                },
                |p: &ae::Pixel16, mc: f32| level(
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64,
                    p.alpha as f32,
                    mc
                )
            ),
            _ => run!(
                as_pixel32_mut,
                1.0,
                |p: &mut ae::PixelF32| {
                    if p.alpha <= 0.0 {
                        p.red = 0.0;
                        p.green = 0.0;
                        p.blue = 0.0;
                    } else if p.alpha != 1.0 {
                        p.red *= p.alpha;
                        p.green *= p.alpha;
                        p.blue *= p.alpha;
                    }
                },
                |p: &mut ae::PixelF32| {
                    if p.alpha <= 0.0 {
                        p.red = 0.0;
                        p.green = 0.0;
                        p.blue = 0.0;
                    } else if p.alpha != 1.0 {
                        let v = 1.0f32 / p.alpha;
                        p.red *= v;
                        p.green *= v;
                        p.blue *= v;
                    }
                },
                |p: &ae::PixelF32, mc: f32| level(
                    p.red as f64,
                    p.green as f64,
                    p.blue as f64,
                    p.alpha,
                    mc
                )
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 明るさだけの一次元の列で試す。
    fn run(vals: &[u8], radius: i64, is_max: bool) -> Vec<u8> {
        let levels: Vec<f32> = vals
            .iter()
            .map(|v| level(*v as f64, *v as f64, *v as f64, 255.0, 255.0))
            .collect();
        let mut out = vals.to_vec();
        line_pass(vals, &levels, &mut out, radius, is_max);
        out
    }

    /// 最大値フィルタは明るいところを半径ぶん広げる。
    #[test]
    fn the_max_spreads_the_bright_spot() {
        let v = [0u8, 0, 0, 9, 0, 0, 0];
        assert_eq!(run(&v, 1, true), vec![0, 0, 9, 9, 9, 0, 0]);
        assert_eq!(run(&v, 2, true), vec![0, 9, 9, 9, 9, 9, 0]);
    }

    /// 最小値フィルタは暗いところを広げる。
    #[test]
    fn the_min_spreads_the_dark_spot() {
        let v = [9u8, 9, 9, 0, 9, 9, 9];
        assert_eq!(run(&v, 1, false), vec![9, 9, 0, 0, 0, 9, 9]);
    }

    /// ブロックの境界をまたいでも窓は正しい。L = 2*r+1 = 3 で境界は 3, 6。
    #[test]
    fn the_window_crosses_the_block_boundary() {
        let v = [0u8, 0, 5, 0, 0, 0, 0, 0, 0];
        // 5 は index 2 (ブロック 0 の末尾)。半径 1 なら 1..3 に広がる。
        assert_eq!(run(&v, 1, true), vec![0, 5, 5, 5, 0, 0, 0, 0, 0]);
    }

    /// 窓が列より広ければ全体がいちばん明るい画素になる。
    #[test]
    fn a_window_wider_than_the_line_flattens_it() {
        let v = [1u8, 7, 3];
        assert_eq!(run(&v, 3, true), vec![7, 7, 7]);
        assert_eq!(run(&v, 99, false), vec![1, 1, 1]);
    }

    /// 半径 0 なら触らない。
    #[test]
    fn no_radius_changes_nothing() {
        let v = [1u8, 7, 3];
        assert_eq!(run(&v, 0, true), vec![1, 7, 3]);
    }

    /// 選ばれるのは画素まるごと。色は混ざらない。
    #[test]
    fn the_whole_pixel_is_carried_over() {
        let px = [
            ae::Pixel8 {
                alpha: 255,
                red: 0,
                green: 0,
                blue: 0,
            },
            ae::Pixel8 {
                alpha: 255,
                red: 200,
                green: 10,
                blue: 30,
            },
            ae::Pixel8 {
                alpha: 255,
                red: 0,
                green: 0,
                blue: 0,
            },
        ];
        let levels: Vec<f32> = px
            .iter()
            .map(|p| level(p.red as f64, p.green as f64, p.blue as f64, p.alpha as f32, 255.0))
            .collect();
        let mut out = px.to_vec();
        line_pass(&px, &levels, &mut out, 1, true);
        for p in &out {
            assert_eq!((p.red, p.green, p.blue), (200, 10, 30));
        }
    }

    /// 同じ明るさならアルファの高いほうが勝つ。
    #[test]
    fn alpha_breaks_the_tie() {
        let a = ae::Pixel8 {
            alpha: 10,
            red: 100,
            green: 100,
            blue: 100,
        };
        let b = ae::Pixel8 {
            alpha: 250,
            red: 100,
            green: 100,
            blue: 100,
        };
        let la = level(100.0, 100.0, 100.0, 10.0, 255.0);
        let lb = level(100.0, 100.0, 100.0, 250.0, 255.0);
        assert!(lb > la);
        let px = [a, b, a];
        let levels = vec![la, lb, la];
        let mut out = px.to_vec();
        line_pass(&px, &levels, &mut out, 1, true);
        for p in &out {
            assert_eq!(p.alpha, 250);
        }
    }
}
