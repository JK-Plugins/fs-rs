//! F's ColorMatKey の移植。
//!
//! 色を4つまで指定して、その色に近い画素だけを残すキーヤー。近さは RGB の
//! まま測るのではなく **CIE Lab での距離**で測るので、明るさの違う同系色が
//! まとめて拾える。
//!
//! 1画素ぶんの手順 (元の ColorMatKey8/16/32):
//!
//! 1. RGB にアルファを掛けて (乗算済みにして) 0..1 にし、Lab に直す
//! 2. 有効な色ごとに `v = 1 - 距離 / √3` を出す。距離は Lab の3成分の
//!    ユークリッド距離
//! 3. `v` を Border と Softness で 0..1 に伸ばす。Softness が 0 なら
//!    Border で 0/1 に切るだけ。そうでなければ
//!    `Start = max(Border - Softness, 0)` から Border までを直線で渡す
//! 4. 4色ぶんを**足して** 1 で止める
//! 5. アルファを `v2 - (1 - 元のアルファ)` にする。元が半透明なぶんは
//!    そのまま引かれる。RGB は触らない
//! 6. InvertAlpha が入っていればアルファを反転する
//!
//! 元から写した細かいところ:
//!
//! - 4色とも無効なら入力をそのまま流す
//! - Border と Softness は % を 100 で割って 0..1 で押さえる
//! - 指定色は 8bit の値を 0..1 にしてから Lab に直す (アルファは無視)
//! - 8/16bit の乗算済み RGB は **float (f32)** で計算される
//!   (`PF_FpShort`)。double でやると丸め境界で 1 ずれる
//! - 8bit は `RoundByteLong((A_long)(v2*255 + 0.5 - a))`。double を整数へ
//!   落とすのが 0 方向への切り捨てなので、負の側では「-0.5 して切り上げ」
//!   のような振る舞いになる。16bit も同じ形
//! - 32bit だけは `RoundFpShortDouble(v2 - (1 - a))` で、丸めずに
//!   0..32 で押さえる
//!
//! 元と違うところ:
//!
//! - `RgbToLab` の暗部のバグ (`libs::lab` のコメント参照: 線形側にも
//!   `-0.16` を引いていて L がしきい値で 0 から 0.08 へ飛ぶ) を直した
//!   `libs::lab::rgb_to_lab` を使っている。暗い色を指定したときや暗部の
//!   拾い方が元と変わる。`FAITHFUL` を立てると元どおりになる
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ColorMatKey

use after_effects::{self as ae};
use libs::lab::{rgb_to_lab_with, LABA};

/// 元の RgbToLab の暗部のバグごと再現する。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 1色ぶんの設定。元の CInfo。
#[derive(Debug, Clone, Copy)]
struct CInfo {
    enabled: bool,
    border: f64,
    softness: f64,
    start: f64,
    lab: LABA,
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    ci: [CInfo; 4],
    rev: bool,
}

/// 元の「色の距離測定」からしきい値までを1色ぶん。戻りは 0..1。
fn coverage(c: &CInfo, lab: &LABA) -> f64 {
    let d = ((lab.l - c.lab.l).powi(2) + (lab.a - c.lab.a).powi(2) + (lab.b - c.lab.b).powi(2))
        .sqrt();
    let v = 1.0 - d / 3.0f64.sqrt();
    if c.softness == 0.0 {
        if v >= c.border {
            1.0
        } else {
            0.0
        }
    } else if v <= c.start {
        0.0
    } else if v >= c.border {
        1.0
    } else {
        (v - c.start) / c.softness
    }
}

/// 元のループ: 有効な色ぶんを足して 1 で止める。
fn total(info: &Info, lab: &LABA) -> f64 {
    let mut v2 = 0.0f64;
    for c in &info.ci {
        if c.enabled {
            v2 += coverage(c, lab);
            if v2 > 1.0 {
                v2 = 1.0;
            }
        }
    }
    v2
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Rev,
    Enabled0,
    Color0,
    Border0,
    Soft0,
    Enabled1,
    Color1,
    Border1,
    Soft1,
    Enabled2,
    Color2,
    Border2,
    Soft2,
    Enabled3,
    Color3,
    Border3,
    Soft3,
}

/// スロットの並びを4色ぶんまとめたもの。
const GROUPS: [(Params, Params, Params, Params); 4] = [
    (
        Params::Enabled0,
        Params::Color0,
        Params::Border0,
        Params::Soft0,
    ),
    (
        Params::Enabled1,
        Params::Color1,
        Params::Border1,
        Params::Soft1,
    ),
    (
        Params::Enabled2,
        Params::Color2,
        Params::Border2,
        Params::Soft2,
    ),
    (
        Params::Enabled3,
        Params::Color3,
        Params::Border3,
        Params::Soft3,
    ),
];

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
        // 元: PF_ADD_CHECKBOX("InvertAlpha", "on", FALSE)
        params.add(
            Params::Rev,
            "InvertAlpha",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: 4色ぶん「Enabled / Color / Border / Softness」の順。既定の色は
        // 赤・緑・青・マゼンタ。
        let colours = [
            (0xFFu8, 0x00u8, 0x00u8),
            (0x00, 0xFF, 0x00),
            (0x00, 0x00, 0xFF),
            (0xFF, 0x00, 0xFF),
        ];
        for (i, g) in GROUPS.iter().enumerate() {
            params.add(
                g.0,
                &format!("Enabled{i}"),
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(false);
                    f.set_value(f.default());
                }),
            )?;
            let (r, gr, b) = colours[i];
            params.add(
                g.1,
                &format!("Color{i}"),
                ae::ColorDef::setup(move |f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: r,
                        green: gr,
                        blue: b,
                    });
                    f.set_value(f.default());
                }),
            )?;
            // 元: PF_ADD_FLOAT_SLIDER("Border", 50, 100, 50, 100, 1, 100, 1, 0, 0)
            params.add(
                g.2,
                &format!("Border{i}"),
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(50.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(50.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(100.0);
                    f.set_value(f.default());
                }),
            )?;
            // 元: PF_ADD_FLOAT_SLIDER("Softness", 0, 100, 0, 100, 1, 15, 1, 0, 0)
            params.add(
                g.3,
                &format!("Softness{i}"),
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(15.0);
                    f.set_value(f.default());
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
                out_data.set_return_msg("fs-rs colormatkey");
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
        // 元の GetParams。
        let rev = params.get(Params::Rev)?.as_checkbox()?.value();
        let mut ci = [CInfo {
            enabled: false,
            border: 0.0,
            softness: 0.0,
            start: 0.0,
            lab: LABA {
                l: 0.0,
                a: 0.0,
                b: 0.0,
                alpha: 0.0,
            },
        }; 4];
        for (i, g) in GROUPS.iter().enumerate() {
            let enabled = params.get(g.0)?.as_checkbox()?.value();
            let c = params.get(g.1)?.as_color()?.value();
            let cf = libs::utils::conv_8_to_32(&c);
            let lab = rgb_to_lab_with(
                cf.red as f64,
                cf.green as f64,
                cf.blue as f64,
                cf.alpha,
                FAITHFUL,
            );
            let border = (params.get(g.2)?.as_float_slider()?.value() as f64 / 100.0).clamp(0.0, 1.0);
            let softness =
                (params.get(g.3)?.as_float_slider()?.value() as f64 / 100.0).clamp(0.0, 1.0);
            ci[i] = CInfo {
                enabled,
                border,
                softness,
                start: (border - softness).max(0.0),
                lab,
            };
        }
        let info = Info { ci, rev };

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず画面をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        // 元: 4色とも無効なら何もしない。
        if info.ci.iter().all(|c| !c.enabled) || w == 0 || h == 0 {
            return Ok(());
        }

        for y in 0..h {
            for x in 0..w {
                match depth {
                    8 => {
                        let p = *in_layer.as_pixel8(x, y);
                        // 元は PF_FpShort (f32) で乗算済みにする。
                        let m = |c: u8| -> f32 { c as f32 * p.alpha as f32 / 255.0 };
                        let lab = rgb_to_lab_with(
                            (m(p.red) / 255.0) as f64,
                            (m(p.green) / 255.0) as f64,
                            (m(p.blue) / 255.0) as f64,
                            p.alpha as f32 / 255.0,
                            FAITHFUL,
                        );
                        let v2 = total(&info, &lab);
                        let a = 255.0f32 - p.alpha as f32;
                        let mut al =
                            libs::utils::round_byte_long((v2 * 255.0 + 0.5 - a as f64) as i32);
                        if info.rev {
                            al = 255 - al;
                        }
                        out_layer.as_pixel8_mut(x, y).alpha = al;
                    }
                    16 => {
                        let p = *in_layer.as_pixel16(x, y);
                        let m = |c: u16| -> f32 { c as f32 * p.alpha as f32 / 32768.0 };
                        let lab = rgb_to_lab_with(
                            (m(p.red) / 32768.0) as f64,
                            (m(p.green) / 32768.0) as f64,
                            (m(p.blue) / 32768.0) as f64,
                            p.alpha as f32 / 32768.0,
                            FAITHFUL,
                        );
                        let v2 = total(&info, &lab);
                        let a = 32768.0f32 - p.alpha as f32;
                        let mut al =
                            libs::utils::round_short((v2 * 32768.0 + 0.5 - a as f64) as i32);
                        if info.rev {
                            al = 32768 - al;
                        }
                        out_layer.as_pixel16_mut(x, y).alpha = al;
                    }
                    _ => {
                        let p = *in_layer.as_pixel32(x, y);
                        let a = p.alpha.clamp(0.0, 1.0);
                        let lab = rgb_to_lab_with(
                            (p.red * a) as f64,
                            (p.green * a) as f64,
                            (p.blue * a) as f64,
                            a,
                            FAITHFUL,
                        );
                        let v2 = total(&info, &lab);
                        let mut al = libs::utils::round_fp_short_double(v2 - (1.0 - a as f64));
                        if info.rev {
                            al = 1.0 - al;
                        }
                        out_layer.as_pixel32_mut(x, y).alpha = al;
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

    fn lab_of(r: f64, g: f64, b: f64) -> LABA {
        rgb_to_lab_with(r, g, b, 1.0, false)
    }

    fn cinfo(r: f64, g: f64, b: f64, border: f64, softness: f64) -> CInfo {
        CInfo {
            enabled: true,
            border,
            softness,
            start: (border - softness).max(0.0),
            lab: lab_of(r, g, b),
        }
    }

    /// 指定色そのものは距離 0 なので必ず通る。
    #[test]
    fn the_target_colour_passes() {
        let c = cinfo(1.0, 0.0, 0.0, 1.0, 0.15);
        assert_eq!(coverage(&c, &lab_of(1.0, 0.0, 0.0)), 1.0);
    }

    /// 遠い色は落ちる。
    #[test]
    fn a_far_colour_is_cut() {
        let c = cinfo(1.0, 0.0, 0.0, 1.0, 0.15);
        assert_eq!(coverage(&c, &lab_of(0.0, 1.0, 0.0)), 0.0);
    }

    /// Softness が 0 なら 0 か 1 しか出ない。
    #[test]
    fn no_softness_means_a_hard_edge() {
        let c = cinfo(1.0, 0.0, 0.0, 0.9, 0.0);
        for rgb in [(1.0, 0.0, 0.0), (0.9, 0.1, 0.1), (0.0, 0.0, 1.0)] {
            let v = coverage(&c, &lab_of(rgb.0, rgb.1, rgb.2));
            assert!(v == 0.0 || v == 1.0, "{rgb:?} -> {v}");
        }
    }

    /// Softness の帯の中は 0..1 の途中の値になる。
    #[test]
    fn the_soft_band_fades() {
        // Border を下げて帯を広く取り、近いが同じではない赤を見る。
        let c = cinfo(1.0, 0.0, 0.0, 0.95, 0.3);
        let v = coverage(&c, &lab_of(0.75, 0.1, 0.1));
        assert!(v > 0.0 && v < 1.0, "v = {v}");
    }

    /// 色は足し合わされて 1 で止まる。
    #[test]
    fn the_colours_add_up_and_stop_at_one() {
        let mut info = Info {
            ci: [cinfo(1.0, 0.0, 0.0, 1.0, 0.15); 4],
            rev: false,
        };
        info.ci[1] = cinfo(0.0, 1.0, 0.0, 1.0, 0.15);
        info.ci[2].enabled = false;
        info.ci[3].enabled = false;
        // 赤は1色目で 1 になり、そこで止まる。
        assert_eq!(total(&info, &lab_of(1.0, 0.0, 0.0)), 1.0);
        // 緑は2色目で拾われる。
        assert_eq!(total(&info, &lab_of(0.0, 1.0, 0.0)), 1.0);
        // どちらでもない色は 0。
        assert_eq!(total(&info, &lab_of(0.0, 0.0, 1.0)), 0.0);
    }

    /// 無効な色は数に入らない。
    #[test]
    fn a_disabled_colour_is_skipped() {
        let mut info = Info {
            ci: [cinfo(1.0, 0.0, 0.0, 1.0, 0.15); 4],
            rev: false,
        };
        for c in info.ci.iter_mut() {
            c.enabled = false;
        }
        assert_eq!(total(&info, &lab_of(1.0, 0.0, 0.0)), 0.0);
    }

    /// Start は負にならない (Border より Softness が大きいとき)。
    #[test]
    fn the_start_never_goes_below_zero() {
        let c = cinfo(1.0, 0.0, 0.0, 0.5, 1.0);
        assert_eq!(c.start, 0.0);
        // 帯の下端が 0 なので、どんなに遠い色でも 0 より上になる。
        assert!(coverage(&c, &lab_of(0.0, 1.0, 0.0)) > 0.0);
    }

    #[test]
    fn the_fix_is_on() {
        assert!(!FAITHFUL);
    }
}
