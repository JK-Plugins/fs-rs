//! F's Max の移植。
//!
//! 最大・最小フィルタ (`Minimax`) をかけるだけのもの。値が正なら明るいほうへ
//! 広がり (膨張)、負なら暗いほうへ縮む。走査の向きと対象チャンネルを選べて、
//! OutOnly を入れると「広がったぶんのアルファ」だけが残る。
//!
//! 元から写した細かいところ:
//!
//! - 値は `downScaleNoClip` (整数の掛け算と割り算、丸めなし)。0 なら何もしない
//! - 比べる明るさは RGB を 0.29891 / 0.58661 / 0.11448 で混ぜて +0.5 して
//!   切り捨てた整数。アルファはそのまま比べる
//! - 窓は `[i-value, i+value-1]` と左右で非対称 (元の `Minimax_*` のまま)
//! - 正なら黒マット、負なら白マットを通してから戻す
//! - OutOnly は `出力のアルファ - 入力のアルファ` を取り、0..最大でクランプ
//!   (8bit は RoundByteFpLong、16bit は RoundShort)
//!
//! **元と違うところ**: 白マットの青チャンネルのバグは再現していない。
//! 元の `toWhiteMat*` は青の行で `p.blue` ではなく (直前に書き換えた)
//! `p.green` を読むので、青に白マットが二重にかかる。`libs` が深度をまたいで
//! 直してあるのでこちらもそのまま
//! ([`libs::fs_graphics8::Image8::to_white_mat8`] の説明を参照)。
//! **値が負のときだけ**出る違いで、画素比較でもそのぶんだけ青がずれる。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Max

use after_effects::{self as ae};
use libs::fs_graphics::{Image, PixelF, ScanLine, TargetChannel};
use libs::fs_graphics8::{round_short_i32, Image16, Image8, Px16, Px8, MAX8};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Max,
    Dir,
    Channel,
    OutOnly,
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
        // 元: PF_ADD_SLIDER(STR_MAX, -512, 512, -50, 50, 0, ..)
        params.add(
            Params::Max,
            "value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-512);
                f.set_valid_max(512);
                f.set_slider_min(-50);
                f.set_slider_max(50);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP(STR_DIR1, 3, 1, "Horizon+Vurtual|Horizon|Vurtual", ..)
        // 元の綴りのまま (Vurtual)。保存した値を引き継ぐため直していない。
        params.add(
            Params::Dir,
            "ScanLine",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizon+Vurtual", "Horizon", "Vurtual"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP(STR_CH1, 3, 1, "RGB+Alpha|RGB|alpha", ..)
        params.add(
            Params::Channel,
            "Channel",
            ae::PopupDef::setup(|f| {
                f.set_options(&["RGB+Alpha", "RGB", "alpha"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::OutOnly,
            "OutOnly",
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
                out_data.set_return_msg("fs-rs max");
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
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        // 元の GetParams: downScaleNoClip は整数の掛け算と割り算 (丸めなし)。
        let value = {
            let v = params.get(Params::Max)?.as_slider()?.value();
            let ds = in_data.downsample_x();
            v * ds.num as i32 / ds.den as i32
        };
        let mode = match params.get(Params::Dir)?.as_popup()?.value() {
            2 => ScanLine::Hor,
            3 => ScanLine::Ver,
            _ => ScanLine::HorAndVer,
        };
        let target = match params.get(Params::Channel)?.as_popup()?.value() {
            2 => TargetChannel::Rgb,
            3 => TargetChannel::Alpha,
            _ => TargetChannel::RgbAndAlpha,
        };
        let out_only = params.get(Params::OutOnly)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をそのままコピー。値が 0 なら何もしない。
        if value == 0 {
            for y in 0..h {
                for x in 0..w {
                    match depth {
                        8 => *out_layer.as_pixel8_mut(x, y) = *in_layer.as_pixel8(x, y),
                        16 => *out_layer.as_pixel16_mut(x, y) = *in_layer.as_pixel16(x, y),
                        _ => *out_layer.as_pixel32_mut(x, y) = *in_layer.as_pixel32(x, y),
                    }
                }
            }
            return Ok(());
        }

        match depth {
            8 => {
                let mut img = Image8::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let p = in_layer.as_pixel8(x, y);
                        *img.get_mut(x, y) = Px8::new(p.alpha, p.red, p.green, p.blue);
                    }
                }
                img.minimax8_with(value, mode, target);
                for y in 0..h {
                    for x in 0..w {
                        let p = img.get(x, y);
                        let alpha = if out_only {
                            // 元: 出力のアルファから入力のアルファを引く。
                            let a = p.alpha as i32 - in_layer.as_pixel8(x, y).alpha as i32;
                            a.clamp(0, MAX8) as u8
                        } else {
                            p.alpha
                        };
                        *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                            alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        };
                    }
                }
            }
            16 => {
                let mut img = Image16::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let p = in_layer.as_pixel16(x, y);
                        *img.get_mut(x, y) = Px16::new(p.alpha, p.red, p.green, p.blue);
                    }
                }
                img.minimax16_with(value, mode, target);
                for y in 0..h {
                    for x in 0..w {
                        let p = img.get(x, y);
                        let alpha = if out_only {
                            let a = p.alpha as i32 - in_layer.as_pixel16(x, y).alpha as i32;
                            round_short_i32(a)
                        } else {
                            p.alpha
                        };
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha,
                            red: p.red,
                            green: p.green,
                            blue: p.blue,
                        };
                    }
                }
            }
            _ => {
                let mut img = Image::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        let p = in_layer.as_pixel32(x, y);
                        *img.get_mut(x, y) = PixelF::new(p.alpha, p.red, p.green, p.blue);
                    }
                }
                img.minimax(value, mode, target);
                for y in 0..h {
                    for x in 0..w {
                        let p = img.get(x, y);
                        let alpha = if out_only {
                            // 元の InOut32 も引き算してクランプする。
                            let a = p.alpha - in_layer.as_pixel32(x, y).alpha;
                            a.clamp(0.0, 1.0)
                        } else {
                            p.alpha
                        };
                        *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                            alpha,
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
    fn the_popups_map_to_the_right_options() {
        // 元: 1 = Horizon+Vurtual, 2 = Horizon, 3 = Vurtual。
        for (v, want) in [
            (1, ScanLine::HorAndVer),
            (2, ScanLine::Hor),
            (3, ScanLine::Ver),
        ] {
            let got = match v {
                2 => ScanLine::Hor,
                3 => ScanLine::Ver,
                _ => ScanLine::HorAndVer,
            };
            assert_eq!(got, want);
        }
        for (v, want) in [
            (1, TargetChannel::RgbAndAlpha),
            (2, TargetChannel::Rgb),
            (3, TargetChannel::Alpha),
        ] {
            let got = match v {
                2 => TargetChannel::Rgb,
                3 => TargetChannel::Alpha,
                _ => TargetChannel::RgbAndAlpha,
            };
            assert_eq!(got, want);
        }
    }

    #[test]
    fn out_only_keeps_just_the_grown_alpha() {
        // 元: 出力 - 入力 を 0..最大でクランプ。
        let f = |out: i32, inp: i32| (out - inp).clamp(0, MAX8);
        assert_eq!(f(255, 0), 255);
        assert_eq!(f(255, 255), 0);
        // 縮んだとき (負) は 0 で止まる。
        assert_eq!(f(0, 255), 0);
    }

    #[test]
    fn a_positive_value_dilates_the_alpha() {
        let mut img = Image8::new(6, 1);
        *img.get_mut(2, 0) = Px8::new(255, 255, 255, 255);
        img.minimax8_with(2, ScanLine::Hor, TargetChannel::Alpha);
        let got: Vec<u8> = img.data.iter().map(|p| p.alpha).collect();
        assert_eq!(got, vec![0, 255, 255, 255, 255, 0]);
    }
}
