//! F's NoiseHiLo_RGB の移植。
//!
//! 画素の輝度を見て、暗部 / 中間 / 明部それぞれに別の強さでノイズを乗せる。
//! HightPos / LoPos が帯の幅、HighLevel / MidLevel / LoLevel が強さ。
//! 帯の境界では強さが三角形に減衰する。
//!
//! 乱数は元と同じ xorShift で、**フレーム番号で1回種をまき、画素ごとに
//! 3つ引く** グローバルな列。つまり画素をどの順に回るかで結果が変わるので、
//! ここでは iterate (回る順の保証がない) ではなく自前の行優先ループで走る。
//! 引く順は元のまま a, b, m で、使う順 (hi=a, mid=m, lo=b) とは違う。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/NoiseHiLo_RGB

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::fs_graphics::XorShift;
use libs::utils::{conv_8_to_32_channel, round_byte_long, round_fp_short, round_short};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    HiPos,
    LoPos,
    HiLv,
    MdLv,
    LoLv,
}

/// GetParams 相当。どれも /100 済み。
#[derive(Clone, Copy)]
struct Settings {
    hi_pos: f64,
    lo_pos: f64,
    hi_lv: f64,
    md_lv: f64,
    lo_lv: f64,
}

impl Settings {
    /// 元の画素関数の本体。v は 0..1 の輝度、(a, b, m) は引いた乱数。
    ///
    /// 戻り値は明部・中間・暗部それぞれの「足す量」(0..1 スケール) で、
    /// **元が足す順**に並ぶ。元は帯ごとに足してその都度丸めとクランプを
    /// かけるので、まとめて足すと結果が変わる (実測で ±1〜4 ずれた)。
    fn contribs(&self, v: f64, a: f64, b: f64, m: f64) -> [Option<f64>; 3] {
        let mut out = [None, None, None];

        // 明部: st..1 の間で、md を頂点に三角形に減衰する。
        let mut a = a * self.hi_lv;
        if a != 0.0 {
            let st = 1.0 - 2.0 * self.hi_pos;
            if v > st {
                let md = 1.0 - self.hi_pos;
                if v == md {
                    // そのまま
                } else if v < md {
                    a *= (v - st) / (md - st);
                } else {
                    a *= (1.0 - v) / (1.0 - md);
                }
                out[0] = Some(a);
            }
        }

        // 中間: st..lt の間。md1..md2 は平坦。
        let mut m = m * self.md_lv;
        if m != 0.0 {
            let st = self.lo_pos;
            let mut md1 = self.lo_pos * 2.0;
            let mut md2 = 1.0 - self.hi_pos * 2.0;
            if md1 > md2 {
                // 元のまま: 幅が逆転したら両方を中点に寄せる。
                md1 = (md1 - md2) / 2.0;
                md2 = md1;
            }
            let lt = 1.0 - self.hi_pos;
            if v > st && v < lt {
                if v >= md1 && v <= md2 {
                    // そのまま
                } else if v < md1 {
                    m *= (v - st) / (md1 - st);
                } else {
                    m *= 1.0 - (v - md2) / (lt - md2);
                }
                out[1] = Some(m);
            }
        }

        // 暗部: 0..lt の間で、md を頂点に減衰する。
        let mut b = b * self.lo_lv;
        if b != 0.0 {
            let lt = self.lo_pos * 2.0;
            if v < lt {
                let md = self.lo_pos;
                if v == md {
                    // そのまま
                } else if v < md {
                    b *= v / md;
                } else {
                    b *= 1.0 - (v - md) / (lt - md);
                }
                out[2] = Some(b);
            }
        }
        out
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
        // 元: PF_ADD_FLOAT_SLIDER(STR_HI_POS, 5, 30, 5, 30, 1, 20, 1, 0, 0, ..)
        for (p, name) in [(Params::HiPos, "HightPos(%)"), (Params::LoPos, "LoPos(%)")] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(5.0);
                    f.set_valid_max(30.0);
                    f.set_slider_min(5.0);
                    f.set_slider_max(30.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(20.0);
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_FLOAT_SLIDER(STR_HI_LV, 0, 100, 0, 5, 1, 0, 1, 0, 0, ..)
        for (p, name) in [
            (Params::HiLv, "HighLevel(%)"),
            (Params::MdLv, "MidLevel(%)"),
            (Params::LoLv, "LoLevel(%)"),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(5.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(0.0);
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
                out_data.set_return_msg("fs-rs noisehilorgb");
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
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() / 100.0)
        };
        let s = Settings {
            hi_pos: pct(Params::HiPos, params)?,
            lo_pos: pct(Params::LoPos, params)?,
            hi_lv: pct(Params::HiLv, params)?,
            md_lv: pct(Params::MdLv, params)?,
            lo_lv: pct(Params::LoLv, params)?,
        };

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元: F_SRAND(ae->frame()) で、frame = current_time / time_step。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = XorShift::new(frame);

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        for y in 0..h {
            for x in 0..w {
                // 元は 3つ引いてから早期リターンを判定するので、
                // どの画素でもちょうど 3つ消費する。
                let a = rng.next_d1();
                let b = rng.next_d1();
                let m = rng.next_d1();
                match depth {
                    8 => {
                        let p = out_layer.as_pixel8_mut(x, y);
                        if p.alpha == 0 {
                            continue;
                        }
                        let v = {
                            let f = |c: u8| conv_8_to_32_channel(c) as f64;
                            (0.29891 * f(p.red) + 0.58661 * f(p.green) + 0.11448 * f(p.blue))
                                .clamp(0.0, 1.0)
                        };
                        if v <= 0.0 || v >= 1.0 {
                            continue;
                        }
                        for c in s.contribs(v, a, b, m).into_iter().flatten() {
                            let add = (MAX_CHANNEL8 as f64 * c) as i32;
                            p.red = round_byte_long(p.red as i32 + add);
                            p.green = round_byte_long(p.green as i32 + add);
                            p.blue = round_byte_long(p.blue as i32 + add);
                        }
                    }
                    16 => {
                        let p = out_layer.as_pixel16_mut(x, y);
                        if p.alpha == 0 {
                            continue;
                        }
                        let v = {
                            let k = 1.0 / MAX_CHANNEL16 as f64;
                            (0.29891 * p.red as f64 * k
                                + 0.58661 * p.green as f64 * k
                                + 0.11448 * p.blue as f64 * k)
                                .clamp(0.0, 1.0)
                        };
                        if v <= 0.0 || v >= 1.0 {
                            continue;
                        }
                        for c in s.contribs(v, a, b, m).into_iter().flatten() {
                            let add = (MAX_CHANNEL16 as f64 * c) as i32;
                            p.red = round_short(p.red as i32 + add);
                            p.green = round_short(p.green as i32 + add);
                            p.blue = round_short(p.blue as i32 + add);
                        }
                    }
                    _ => {
                        let p = out_layer.as_pixel32_mut(x, y);
                        if p.alpha <= 0.0 {
                            continue;
                        }
                        let v = (0.29891 * p.red as f64
                            + 0.58661 * p.green as f64
                            + 0.11448 * p.blue as f64)
                            .clamp(0.0, 1.0);
                        if v <= 0.0 || v >= 1.0 {
                            continue;
                        }
                        for c in s.contribs(v, a, b, m).into_iter().flatten() {
                            let d = c as f32;
                            p.red = round_fp_short(p.red + d);
                            p.green = round_fp_short(p.green + d);
                            p.blue = round_fp_short(p.blue + d);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
