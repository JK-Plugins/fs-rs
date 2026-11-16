//! F's ChannelNoise の移植。
//!
//! RGB それぞれに別の幅のノイズを足す。幅 (Noise%) と効き (Opacity%) を
//! チャンネルごとに持ち、アルファのある画素にだけ乗る。
//!
//! 元から写した細かいところ:
//!
//! - 種はフレーム番号 (`current_time / time_step`)。毎フレーム別の絵になる
//! - 乱数は F_RAND2(-幅, 幅)。**使わないチャンネルでも1回引いて進める**し、
//!   **アルファが 0 の画素でも3回引いて進める**。ここを飛ばすと以降の並びが
//!   全部ずれる
//! - 幅と効きは固定小数を `(PF_FpShort)f / (100<<16)` で f32 にする
//!   (0..100% が 0..1)
//! - 幅は深度の最大値を掛けて整数にする。16bit と 32bit はどちらも
//!   PF_MAX_CHAN16 の目盛り
//! - 乱数に効きを掛けたあと `(A_long)` で切り捨て、0 なら足さない
//! - 3チャンネルすべてが「幅 0 または効き 0」なら何もしない
//!
//! 直したバグ2つ:
//!
//! - **8/16bit の青で緑の不透明度を掛けている** (`infoP->green_opt`)。
//!   32bit は青を使っているので、そちらが正しいとみて直した
//! - **32bit だけ 65536 で割っている**。幅は PF_MAX_CHAN16 (32768) の目盛りで
//!   作っているので、32bit のノイズが 8/16bit の半分になってしまう。
//!   32768 で割るようにした。classic 経路は 8bit 専用なので、この修正は
//!   元と突き合わせて確かめたものではなく、深度どうしの食い違いを根拠に
//!   している
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ChannelNoise

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};
use libs::fs_graphics::XorShift;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    RedNoise,
    RedOpt,
    GreenNoise,
    GreenOpt,
    BlueNoise,
    BlueOpt,
}

/// 読み込んだパラメータ。幅は深度の目盛りの整数、効きは 0..2 の実数。
struct Noise {
    red_add: i64,
    green_add: i64,
    blue_add: i64,
    red_value: f32,
    green_value: f32,
    blue_value: f32,
    red_opt: f32,
    green_opt: f32,
    blue_opt: f32,
}

impl Noise {
    fn red_on(&self) -> bool {
        self.red_value > 0.0 && self.red_opt > 0.0
    }
    fn green_on(&self) -> bool {
        self.green_value > 0.0 && self.green_opt > 0.0
    }
    fn blue_on(&self) -> bool {
        self.blue_value > 0.0 && self.blue_opt > 0.0
    }

    /// 元の青の枝が掛ける不透明度。
    fn blue_opt_used(&self) -> f32 {
            // 元は 8/16bit の青のところで緑の不透明度を掛けてしまっている
            // (写し間違い)。32bit は青を使っているので、そちらが正しい。
            let opt = self.blue_opt;
        opt
    }
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
        // 元: PF_ADD_FIXED(ノイズ, 0, 300, 0, 100, 0, 1, 0, 0, ..)
        //     PF_ADD_FIXED(不透明度, 0, 200, 0, 100, 100, 1, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        for (p, name, vmax, dflt) in [
            (Params::RedNoise, "Red Noise(%)", 300.0, 0.0),
            (Params::RedOpt, "Red Opacity(%)", 200.0, 100.0),
            (Params::GreenNoise, "Green Noise(%)", 300.0, 0.0),
            (Params::GreenOpt, "Green Opacity(%)", 200.0, 100.0),
            (Params::BlueNoise, "Blue Noise(%)", 300.0, 0.0),
            (Params::BlueOpt, "Blue Opacity(%)", 200.0, 100.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(vmax);
                    f.set_slider_min(0.0);
                    f.set_slider_max(100.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
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
                out_data.set_return_msg("fs-rs channelnoise");
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
        // 元の GetParams: 固定小数を (100<<16) で割って f32 にする。
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f32, Error> {
            let f = libs::utils::flt2fix(params.get(p)?.as_float_slider()?.value());
            Ok(f as f32 / (100 * 65536) as f32)
        };
        let red_value = pct(Params::RedNoise, params)?;
        let red_opt = pct(Params::RedOpt, params)?;
        let green_value = pct(Params::GreenNoise, params)?;
        let green_opt = pct(Params::GreenOpt, params)?;
        let blue_value = pct(Params::BlueNoise, params)?;
        let blue_opt = pct(Params::BlueOpt, params)?;

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

        // 元: 幅は深度の最大値を掛ける。16bit と 32bit はどちらも
        // PF_MAX_CHAN16 の目盛り。
        let scale_i = if depth == 8 {
            MAX_CHANNEL8 as f32
        } else {
            MAX_CHANNEL16 as f32
        };
        let n = Noise {
            red_add: (scale_i * red_value) as i64,
            green_add: (scale_i * green_value) as i64,
            blue_add: (scale_i * blue_value) as i64,
            red_value,
            green_value,
            blue_value,
            red_opt,
            green_opt,
            blue_opt,
        };

        // 元: 3チャンネルすべてが「幅 0 または効き 0」なら何もしない。
        if !(n.red_on() || n.green_on() || n.blue_on()) {
            return Ok(());
        }

        // 元: 種はフレーム番号。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = XorShift::new(frame);

        // 元は 65536 で割っていて、幅を PF_MAX_CHAN16 (32768) の目盛りで
        // 作っているのと食い違う (32bit だけノイズが半分になる)。
        // 8/16bit と同じ強さになるよう 32768 で割る。
        let scale = MAX_CHANNEL16 as f32;
        for y in 0..h {
            for x in 0..w {
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        if p.alpha == 0 {
                            // 元: アルファが 0 でも3回引いて数を合わせる。
                            rng.next();
                            rng.next();
                            rng.next();
                            continue;
                        }
                        let f = |v: u8, add: i64, opt: f32, rng: &mut XorShift| -> u8 {
                            let av = rng.rand2(-add, add);
                            // 元: 掛けたあと (A_long) で切り捨て。
                            let av = (av as f32 * opt) as i64;
                            if av == 0 {
                                v
                            } else {
                                (v as i64 + av).clamp(0, MAX_CHANNEL8 as i64) as u8
                            }
                        };
                        if n.red_on() {
                            p.red = f(p.red, n.red_add, n.red_opt, &mut rng);
                        } else {
                            rng.next();
                        }
                        if n.green_on() {
                            p.green = f(p.green, n.green_add, n.green_opt, &mut rng);
                        } else {
                            rng.next();
                        }
                        if n.blue_on() {
                            p.blue = f(p.blue, n.blue_add, n.blue_opt_used(), &mut rng);
                        } else {
                            rng.next();
                        }
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        if p.alpha == 0 {
                            rng.next();
                            rng.next();
                            rng.next();
                            continue;
                        }
                        let f = |v: u16, add: i64, opt: f32, rng: &mut XorShift| -> u16 {
                            let av = rng.rand2(-add, add);
                            let av = (av as f32 * opt) as i64;
                            if av == 0 {
                                v
                            } else {
                                (v as i64 + av).clamp(0, MAX_CHANNEL16 as i64) as u16
                            }
                        };
                        if n.red_on() {
                            p.red = f(p.red, n.red_add, n.red_opt, &mut rng);
                        } else {
                            rng.next();
                        }
                        if n.green_on() {
                            p.green = f(p.green, n.green_add, n.green_opt, &mut rng);
                        } else {
                            rng.next();
                        }
                        if n.blue_on() {
                            p.blue = f(p.blue, n.blue_add, n.blue_opt_used(), &mut rng);
                        } else {
                            rng.next();
                        }
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        if p.alpha == 0.0 {
                            rng.next();
                            rng.next();
                            rng.next();
                            continue;
                        }
                        let f = |v: f32, add: i64, opt: f32, rng: &mut XorShift| -> f32 {
                            let av = rng.rand2(-add, add) as f32 / scale;
                            let av = av * opt;
                            if av == 0.0 {
                                v
                            } else {
                                // 元の RoundFpShortDouble は 0..32 でクランプ。
                                ((v + av) as f64).clamp(0.0, 32.0) as f32
                            }
                        };
                        if n.red_on() {
                            p.red = f(p.red, n.red_add, n.red_opt, &mut rng);
                        } else {
                            rng.next();
                        }
                        if n.green_on() {
                            p.green = f(p.green, n.green_add, n.green_opt, &mut rng);
                        } else {
                            rng.next();
                        }
                        // 32bit は元から青の不透明度を使っている。
                        if n.blue_on() {
                            p.blue = f(p.blue, n.blue_add, n.blue_opt, &mut rng);
                        } else {
                            rng.next();
                        }
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

    fn noise(red: f32, green: f32, blue: f32, opt: f32) -> Noise {
        Noise {
            red_add: 10,
            green_add: 10,
            blue_add: 10,
            red_value: red,
            green_value: green,
            blue_value: blue,
            red_opt: opt,
            green_opt: opt,
            blue_opt: opt,
        }
    }

    #[test]
    fn a_channel_is_off_when_either_the_width_or_the_opacity_is_zero() {
        let n = noise(1.0, 0.0, 1.0, 1.0);
        assert!(n.red_on());
        assert!(!n.green_on());
        assert!(n.blue_on());
        let n = noise(1.0, 1.0, 1.0, 0.0);
        assert!(!n.red_on());
        assert!(!n.green_on());
        assert!(!n.blue_on());
    }

    #[test]
    fn the_percentage_becomes_a_fraction() {
        // 元: (PF_FpShort)f / (100<<16)。100% で 1.0。
        let f = |v: f64| libs::utils::flt2fix(v) as f32 / (100 * 65536) as f32;
        assert_eq!(f(100.0), 1.0);
        assert_eq!(f(0.0), 0.0);
        assert!((f(50.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_jitter_truncates_after_the_multiply() {
        // 元: (A_long)((PF_FpShort)aV * opt) なので 0 方向へ切り捨て。
        let g = |av: i64, opt: f32| (av as f32 * opt) as i64;
        assert_eq!(g(3, 0.5), 1);
        assert_eq!(g(-3, 0.5), -1);
        assert_eq!(g(1, 0.5), 0);
    }
}
