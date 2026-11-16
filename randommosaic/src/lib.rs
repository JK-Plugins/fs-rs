//! F's RandomMosaic の移植。
//!
//! 四角いモザイクをフレームごとの乱数でばらまく。塗りつぶし自体は
//! `Mosaic` と同じ `libs::fs_mosaic::flat` (元の flat8/16/32)。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしているので
//! 英語名にしてある。
//!
//! 元から写した細かいところ:
//!
//! - 種はフレーム番号 (`current_time / time_step`)。毎フレーム別の絵になる
//! - 乱数は F_RAND2 (`min + (F_RAND() * (max-min+1) / (1+0x7FFF))`、
//!   最後に切り捨て)。引く順まで合わせてある
//! - **ばらつきを使わない設定でも乱数を捨てて進める**。縦横のばらつきが 0 の
//!   ときは2回、明るさのばらつきが 0 のときは1回。ここを飛ばすと以降の
//!   並びがずれる
//! - 大きさは最大・最小とも `downScale` (+0.5 で丸め) してから、最小が最大を
//!   超えていたら入れ替える
//! - 縦横のばらつきは `(最大 - 最小) * ばらつき / 100` を幅として、縦と横に
//!   別々に足す。2 より小さくなったら 2 で止める
//! - 位置は `-大きさ .. 画面の幅` の乱数なので、左や上にはみ出した矩形も出る
//!   (はみ出したぶんは `flat` が詰める)
//! - 明るさのばらつきは 16/32bit では 8bit から 16bit に直した値で引く。
//!   32bit はそれを PF_MAX_CHAN16 で割って実数にする
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/RandomMosaic

use after_effects::{self as ae, MAX_CHANNEL16};
use libs::fs_graphics::XorShift;
use libs::fs_mosaic::flat;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    SizeMax,
    SizeMin,
    Aspect,
    RandColor,
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
        // 元: PF_ADD_SLIDER(量, 0, 2048, 0, 512, 0, ..)
        params.add(
            Params::Value,
            "Count",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(2048);
                f.set_slider_min(0);
                f.set_slider_max(512);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(サイズ(最大), 3, 1024, 12, 64, 24, ..)
        params.add(
            Params::SizeMax,
            "Size(max)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(3);
                f.set_valid_max(1024);
                f.set_slider_min(12);
                f.set_slider_max(64);
                f.set_default(24);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(サイズ(最小), 3, 1024, 6, 64, 6, ..)
        params.add(
            Params::SizeMin,
            "Size(min)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(3);
                f.set_valid_max(1024);
                f.set_slider_min(6);
                f.set_slider_max(64);
                f.set_default(6);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(縦横のばらつき, 0, 100, 0, 100, 0, ..)
        params.add(
            Params::Aspect,
            "AspectRand(%)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(明るさのばらつき, 0, 255, 0, 16, 0, ..)
        params.add(
            Params::RandColor,
            "BrightnessRand",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(255);
                f.set_slider_min(0);
                f.set_slider_max(16);
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
                out_data.set_return_msg("fs-rs randommosaic");
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
        let value = params.get(Params::Value)?.as_slider()?.value() as i64;
        let ds = in_data.downsample_x();
        // 元の downScale: 0 はそのまま、他は +0.5 で丸め。
        let down = |v: i64| -> i64 {
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        let mut size_max = down(params.get(Params::SizeMax)?.as_slider()?.value() as i64);
        let mut size_min = down(params.get(Params::SizeMin)?.as_slider()?.value() as i64);
        // 元: 最小が最大を超えていたら入れ替える。
        if size_min > size_max {
            std::mem::swap(&mut size_min, &mut size_max);
        }
        let aspect = params.get(Params::Aspect)?.as_slider()?.value() as i64;
        let rand_color = params.get(Params::RandColor)?.as_slider()?.value() as i64;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をそのままコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        if value <= 0 {
            return Ok(());
        }

        // 元: 種はフレーム番号 (current_time / time_step)。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = XorShift::new(frame);

        // 元: 8bit はそのまま、16/32bit は 8bit から 16bit に直した値。
        let pr_value = if depth == 8 {
            rand_color
        } else {
            // 元の FS_CONVERT8TO16: (v * 32768 + 128) / 255。
            (rand_color * MAX_CHANNEL16 as i64 + 128) / 255
        };

        let (iw, ih) = (w as i64, h as i64);
        for _ in 0..value {
            // 大きさ。
            let mut size_x = rng.rand2(size_min, size_max);
            let mut size_y = size_x;
            if aspect > 0 {
                let temp = (size_max - size_min) * aspect / 100;
                size_x += rng.rand2(-temp, temp);
                size_y += rng.rand2(-temp, temp);
                if size_x < 2 {
                    size_x = 2;
                }
                if size_y < 2 {
                    size_y = 2;
                }
            } else {
                // 元: 使わなくても2回引いて進める。
                rng.next();
                rng.next();
            }
            // 位置。左や上にはみ出すこともある。
            let x = rng.rand2(-size_x, iw);
            let y = rng.rand2(-size_y, ih);
            // 明るさのばらつき。a, r, g, b に同じ値。
            let r = if pr_value > 0 {
                rng.rand2(-pr_value, pr_value)
            } else {
                // 元: 使わなくても1回引いて進める。
                rng.next();
                0
            };
            // 元の flat32 は 16bit の目盛りで引いた値を PF_MAX_CHAN16 で割る。
            let k = if depth == 32 {
                MAX_CHANNEL16 as f64
            } else {
                1.0
            };
            let o = r as f64 / k;
            flat(&out_layer, depth, x, y, size_x, size_y, [o, o, o, o]);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand2_covers_the_whole_range() {
        let mut rng = XorShift::new(0);
        let (mut lo, mut hi) = (i64::MAX, i64::MIN);
        for _ in 0..2000 {
            let v = rng.rand2(-5, 5);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        assert_eq!((lo, hi), (-5, 5));
    }

    #[test]
    fn skipping_the_unused_draws_keeps_the_sequence_aligned() {
        // ばらつきを使わない経路でも同じ回数引くので、位置の並びが揃う。
        let mut a = XorShift::new(7);
        let mut b = XorShift::new(7);
        // a: ばらつき無しの経路 (捨てる2回)
        let _ = a.rand2(6, 24);
        a.next();
        a.next();
        let ax = a.rand2(-10, 256);
        // b: 同じ回数だけ引く
        let _ = b.rand2(6, 24);
        let _ = b.rand2(-3, 3);
        let _ = b.rand2(-3, 3);
        let bx = b.rand2(-10, 256);
        assert_eq!(ax, bx);
    }

    #[test]
    fn the_sixteen_bit_jitter_is_converted_from_eight_bit() {
        // 元の FS_CONVERT8TO16。
        let f = |v: i64| (v * 32768 + 128) / 255;
        assert_eq!(f(0), 0);
        assert_eq!(f(255), 32768);
        assert_eq!(f(16), (16 * 32768 + 128) / 255);
    }
}
