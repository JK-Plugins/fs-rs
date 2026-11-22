//! F's EdgeLine-Hi の移植。
//!
//! 「対象色」の画素から選んだ向きへ最大 Length 画素ぶん光線を伸ばし、
//! 途中で「見本色」に当たったらその画素を「枠の色」で塗る。セル画の
//! 隣り合った塗り分けの境目に線を引くためのもの。
//!
//! 色の比較は RGB だけで、Level(%) を許容幅にした「だいたい同じ」判定
//! (アルファは見ない)。16/32bit でも 8bit に落としてから比べる。
//!
//! 元から写した細かいところ:
//!
//! - 光線は `i = 0..=Length` で、i=0 は自分自身。向きは単位ベクトルなので
//!   1画素ずつ進む。座標は `(long)(xL + xscan*i + 0.5)` と 0.5 を足して
//!   切り捨てる (負なら 0 方向へ)
//! - 画面の外に出たらそこで打ち切り。直前と同じ画素に落ちたら飛ばす
//! - GiveUpSoon が ON なら、見本色でもなく対象色でもない画素に当たった
//!   ところで打ち切る
//! - 読むのは入力のほうなので、塗った結果が後の判定に影響しない
//!
//! **元のソースはこのリポジトリのライブラリではコンパイルできない。**
//! 比較に使っている `compPix8E` / `compPix16E` / `compPix32E` が
//! どこにも定義されていないため。引数の形と使い方から
//! `compPix8Lv` (RGB の差がどれも lv 以下) と、16/32bit は
//! `compPix16_8Lv` / `compPix32_8Lv` (8bit に落としてから比べる) と
//! 同じものとみて書き、出荷 .aex と画素を突き合わせて確かめた。
//!
//! 同じフォルダの `EdgeLine-Hi_sub08.cpp` には走査線を使う別の実装が
//! あるが、`Exec` から呼ばれておらず、`ParamInfo` に無いメンバを使って
//! いるのでそもそもビルドできない。**死んだコードなので移植していない**
//! (縦方向の関数が横方向の値で早期 return する、縦の範囲を高さでなく幅で
//! 見ている、ループの終わりが横と違う、と3つ壊れている)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/EdgeLine-Hi

use after_effects::{self as ae};
use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32, rot_length};

/// 元の compPix8Lv。RGB の差がどれも lv 以下なら同じ色とみなす
/// (アルファは見ない)。
fn comp_rgb(a: [u8; 3], b: [u8; 3], lv: u8) -> bool {
    (0..3).all(|i| (a[i] as i32 - b[i] as i32).abs() <= lv as i32)
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TargetColor,
    SampleColor,
    Level,
    Length,
    Rot,
    DrawColor,
    ScanFlag,
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
        for (p, name, rgb) in [
            (Params::TargetColor, "TargetColor", [0xFF, 0xFF, 0x00]),
            (Params::SampleColor, "SampleColor", [0xFF, 0x00, 0x00]),
        ] {
            params.add(
                p,
                name,
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_LEVEL, 0, 100, 0, 50, 1, 0, 1, 0, 0, ..)
        params.add(
            Params::Level,
            "Level(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(STR_LENGTH, 0, 100, 0, 20, 0, ..)
        params.add(
            Params::Length,
            "Length(px)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(20);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_ANGLE(STR_ROT, 0, ..)
        params.add(
            Params::Rot,
            "Rot",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::DrawColor,
            "frame rander",
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
            Params::ScanFlag,
            "scanFlag",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("GiveUpSoon");
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
                out_data.set_return_msg("fs-rs edgelinehi");
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
        let target = params.get(Params::TargetColor)?.as_color()?.value();
        let sample = params.get(Params::SampleColor)?.as_color()?.value();
        let length = params.get(Params::Length)?.as_slider()?.value();
        let rot = libs::utils::flt2fix(params.get(Params::Rot)?.as_angle()?.value() as f64);
        // 元: level = (A_u_char)(255 * b/100 + 0.5)。
        let level = {
            let b = params.get(Params::Level)?.as_float_slider()?.value();
            (255.0 * b / 100.0 + 0.5) as u8
        };
        let draw = params.get(Params::DrawColor)?.as_color()?.value();
        let give_up_soon = params.get(Params::ScanFlag)?.as_checkbox()?.value();

        // 元の Exec: まず入力をそのままコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元: Length が 0 以下なら何もしない。
        // (元は GetParams で length で割っているので、ここを通らないと
        //  0 除算になる)
        if length <= 0 {
            return Ok(());
        }

        // 元の GetParams: 角度と距離から XY を出し、距離で割って単位ベクトルに。
        let (fx, fy) = rot_length(rot, length << 16);
        let xscan = (fx as f64 / 65536.0) / length as f64;
        let yscan = (fy as f64 / 65536.0) / length as f64;

        let (w, h) = (out_layer.width() as i32, out_layer.height() as i32);
        let depth = out_layer.bit_depth();

        let t8 = [target.red, target.green, target.blue];
        let s8 = [sample.red, sample.green, sample.blue];
        let draw16 = conv_8_to_16(&draw);
        let draw32 = conv_8_to_32(&draw);

        // その画素の色を 8bit で返す (16/32bit は元と同じやり方で落とす)。
        let rgb8 = |x: i32, y: i32| -> [u8; 3] {
            match depth {
                8 => {
                    let p = in_layer.as_pixel8(x as usize, y as usize);
                    [p.red, p.green, p.blue]
                }
                16 => {
                    let p = *in_layer.as_pixel16(x as usize, y as usize);
                    let c = conv_16_to_8(&p);
                    [c.red, c.green, c.blue]
                }
                _ => {
                    let p = *in_layer.as_pixel32(x as usize, y as usize);
                    let c = conv_32_to_8(&p);
                    [c.red, c.green, c.blue]
                }
            }
        };

        for y0 in 0..h {
            for x0 in 0..w {
                if !comp_rgb(rgb8(x0, y0), t8, level) {
                    continue;
                }
                // 元: 直前に見た画素を覚えておいて、同じなら飛ばす。
                let (mut xx, mut yy) = (-65536i32, -65536i32);
                for i in 0..=length {
                    // 元: (A_long)((double)xL + xscan*i + 0.5) ―― 0.5 を足して
                    // 0 方向へ切り捨て。
                    let x = (x0 as f64 + xscan * i as f64 + 0.5) as i32;
                    let y = (y0 as f64 + yscan * i as f64 + 0.5) as i32;
                    if x < 0 || x >= w || y < 0 || y >= h {
                        break;
                    }
                    if x == xx && y == yy {
                        continue;
                    }
                    let c = rgb8(x, y);
                    if comp_rgb(c, s8, level) {
                        match depth {
                            8 => *out_layer.as_pixel8_mut(x0 as usize, y0 as usize) = draw,
                            16 => {
                                *out_layer.as_pixel16_mut(x0 as usize, y0 as usize) = draw16
                            }
                            _ => *out_layer.as_pixel32_mut(x0 as usize, y0 as usize) = draw32,
                        }
                        break;
                    } else if give_up_soon && !comp_rgb(c, t8, level) {
                        break;
                    }
                    xx = x;
                    yy = y;
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
    fn comp_rgb_ignores_alpha_and_uses_the_level() {
        assert!(comp_rgb([10, 20, 30], [10, 20, 30], 0));
        assert!(!comp_rgb([10, 20, 30], [11, 20, 30], 0));
        assert!(comp_rgb([10, 20, 30], [11, 20, 30], 1));
        assert!(!comp_rgb([10, 20, 30], [12, 20, 30], 1));
        // どのチャンネルも幅のうちでないと駄目。
        assert!(!comp_rgb([10, 20, 30], [11, 25, 30], 1));
    }

    #[test]
    fn the_ray_steps_one_pixel_at_a_time() {
        // 0度 (上) で距離 5 なら、単位ベクトルは (0, -1)。
        let (fx, fy) = rot_length(0, 5 << 16);
        let xscan = (fx as f64 / 65536.0) / 5.0;
        let yscan = (fy as f64 / 65536.0) / 5.0;
        assert_eq!(xscan, 0.0);
        assert_eq!(yscan, -1.0);
        // i 番目の点は y が i だけ上。
        for i in 0..=5 {
            let y = (10.0 + yscan * i as f64 + 0.5) as i32;
            assert_eq!(y, 10 - i);
        }
    }

    #[test]
    fn the_level_rounds_the_percentage() {
        let lv = |b: f64| (255.0 * b / 100.0 + 0.5) as u8;
        assert_eq!(lv(0.0), 0);
        assert_eq!(lv(100.0), 255);
        assert_eq!(lv(50.0), 128);
    }
}
