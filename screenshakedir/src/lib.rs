//! F's ScreenShakeDir の移植。
//!
//! 画面全体を乱数で決めた向きと量だけずらして拾い直す。手ぶれというより
//! 「ガタッと揺れる」ほうで、**偶数フレームは向きが反転する**ので
//! 1フレームおきに行き来する。はみ出した縁の扱いは
//! 「透明 / 伸ばす / 繰り返す / 鏡」から選ぶ。
//!
//! 大きさはpx で指定する。
//!
//! ずらし量が縦横どちらも 0 なら入力をそのまま通す。拾うのは固定小数の
//! 座標なのでバイリニアで混ぜる (元の `FsScreenShake.h`、こちらでは
//! `libs::screen_shake`)。
//!
//! 直したバグ2つ (どちらも `libs::screen_shake::shift` にある):
//!
//! - **乱数を蒔く前に引いている**。元は揺れの大きさと向きを決めたあとで
//!   `F_SRAND(frame + Random Seed)` を呼ぶので、引いた2つには種が効かない。
//!   プラグインを読み込んだ直後は xorShift の状態が全部 0 で 0 を返し
//!   続けるため、大きさは最小値、向きは「ばらつきを引いた値」に張り付き、
//!   **Random Seed はどこにも効かない**。2回目以降の描画では前回の種が
//!   残っているので、同じフレームでも呼ばれ方で絵が変わる。先に種を蒔く
//!   ようにした
//! - **大きさの最小値の計算が 32bit で溢れる**。
//!   `大きさ * (100 - ばらつき) / 100` を A_long でやるので 328px あたりから
//!   破綻する。64bit で計算するようにした
//!
//! 元から写した細かいところ:
//!
//! - 向きは 0 度が上、90 度が右 (元の CRotCalc)
//! - 大きさは downsample を掛ける (`downScaleNoClip`)
//! - 方向のばらつきはスライダーの度を 16bit 左にずらして固定小数にする
//! - 8bit は4点の重みを掛けて足してから `>>16`、16bit は**項ごとに `>>16`**
//!   してから足す。16bit のほうが刻みが粗い
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ScreenShakeDir

use after_effects::{self as ae};
use libs::screen_shake::{get16, get32, get8, round_angle_360, Edge, Shake};
use libs::utils::flt2fix;

/// 真にすると元のままの振る舞い (バグ入り) になる。元と画素を突き合わせる
/// ためだけのもので、出荷するのは偽のほう。
const FAITHFUL: bool = false;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    ValueRand,
    Dir,
    DirRand,
    RandomSeed,
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
        // 元: PF_ADD_FIXED("大きさ(px)", 0, 2000, 0, 100, 10, 1, 0, 0)。
        // 名前は元が非 ASCII なので英語。
        params.add(
            Params::Value,
            "Size(px)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(2000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(10.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("大きさのばらつき(%)", 0, 100, 0, 100, 10)。
        params.add(
            Params::ValueRand,
            "Size Variation(%)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_ANGLE("方向(Rot)", 15.0)。
        params.add(
            Params::Dir,
            "Direction",
            ae::AngleDef::setup(|f| {
                f.set_default(15.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("方向のばらつき(Rot)", 0, 360, 0, 45, 10)。
        params.add(
            Params::DirRand,
            "Direction Variation",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(360);
                f.set_slider_min(0);
                f.set_slider_max(45);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("Random Seed", 0, 99, 0, 30, 0)。
        params.add(
            Params::RandomSeed,
            "Random Seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(99);
                f.set_slider_min(0);
                f.set_slider_max(30);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP("縁の処理", 4, 1, "透明|伸ばす|繰り返す1|繰り返す2")。
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
                out_data.set_return_msg("fs-rs screenshakedir");
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
        // 元の GetParams の順。
        let value = {
            let v = flt2fix(params.get(Params::Value)?.as_float_slider()?.value());
            // 元の downScaleNoClip。
            (v as i64 * ds.num as i64 / ds.den as i64) as i32
        };
        let value_rand = params.get(Params::ValueRand)?.as_slider()?.value();
        let dir = round_angle_360(flt2fix(params.get(Params::Dir)?.as_angle()?.value() as f64));
        // 元: 度のスライダーを 16bit 左にずらして固定小数にする。
        let dir_rand = params.get(Params::DirRand)?.as_slider()?.value() << 16;
        let randomseed = params.get(Params::RandomSeed)?.as_slider()?.value();
        let edge = Edge::from_popup(params.get(Params::Edge)?.as_popup()?.value());

        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { time / step } else { 0 };
        let shake = Shake {
            value,
            value_rand,
            dir,
            dir_rand,
            randomseed,
        };
        let (sx, sy) = libs::screen_shake::shift(&shake, frame, FAITHFUL);

        // 元の Exec: 縦横どちらも 0 なら素通し。
        if sx == 0 && sy == 0 {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! run {
            ($rd:ident, $wr:ident, $get:ident) => {{
                let mut src = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        src.push(*in_layer.$rd(x, y));
                    }
                }
                for y in 0..h {
                    for x in 0..w {
                        let nx = ((x as i32) << 16).wrapping_add(sx);
                        let ny = ((y as i32) << 16).wrapping_add(sy);
                        *out_layer.$wr(x, y) = $get(&src, w as i32, h as i32, edge, nx, ny);
                    }
                }
            }};
        }
        match depth {
            8 => run!(as_pixel8, as_pixel8_mut, get8),
            16 => run!(as_pixel16, as_pixel16_mut, get16),
            _ => run!(as_pixel32, as_pixel32_mut, get32),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// 角度の正規化はライブラリ側に置いてあるが、ここでも一度通しておく。
    #[test]
    fn the_angle_wraps_round() {
        assert_eq!(super::round_angle_360(-(90 << 16)), 270 << 16);
        assert_eq!(super::round_angle_360(450 << 16), 90 << 16);
    }

    /// 出荷するのは直した版。
    #[test]
    fn the_shipped_build_is_the_fixed_one() {
        assert!(!super::FAITHFUL);
    }
}
