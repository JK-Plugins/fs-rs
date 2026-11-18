//! F's InnerFlareRot の移植。
//!
//! `InnerFlare` の向きつきの版。アルファを**指定した角度へずらして3枚
//! 重ねた**ものを下書きにするので、内側の光が片側へ流れる。
//!
//! 手順 (元の Exec):
//!
//! 1. `AlphaCopyDDM`: 画素ごとに、角度 `rot - 90°` の向きへ `length`、
//!    さらに `length * offset`、`length * offset * 2` だけ離れた3点の
//!    アルファを読み、**「透けぐあい」を掛け合わせる**:
//!    `(1-a1)(1-a2)(1-a3)`。それを RGBA 全部に入れる。読む位置は画面の
//!    縁で押さえる
//! 2. `blur` が 0 より大きければ、乗算済みにして箱ぼかしを3回掛け、
//!    乗算を戻す
//! 3. `hyperbolic` が 0 でなければアルファにカーブを掛ける
//! 4. `AlphaCopyRM`: RGB を指定色にして、アルファに入力のアルファを掛ける
//!    (内側だけが残る)
//!
//! 元から写した細かいところ:
//!
//! - 向きは `rot - 90°` で、`length` には downsample を掛ける
//!   (`offset` には掛けない)
//! - 読む位置は `(A_long)(座標 + ずれ + 0.5)` を 0..幅-1 で押さえる。
//!   負の側は 0 方向への切り捨てなので、きれいな四捨五入ではない
//! - 8/16bit の仕上げは `結果 * 最大値 + 0.5` を切り捨て、32bit は
//!   0..1 で押さえるだけ
//! - 最後の掛け算は `出力のアルファ * 入力のアルファ / 最大値` を切り捨て
//! - 同じフォルダに `Tiny_MinMax` も置かれているが、この効果からは
//!   呼ばれていない (`InnerFlare` のほうだけが使う)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/InnerFlareRot_old

use after_effects::{self as ae};
use libs::tiny::{box_blur, hyperbolic, mult, Buf, Px};

/// 元の AlphaCopyDD。3点の「透けぐあい」を掛け合わせる。
fn alpha_copy_dd(buf: &mut Buf, src: &[Px], rot: f64, length: f64, offset: f64) {
    let (w, h) = (buf.w as i64, buf.h as i64);
    let max = buf.max;
    let float = buf.float;
    let rad = (rot - 90.0) * std::f64::consts::PI / 180.0;
    let sin_v = rad.sin() * length;
    let cos_v = rad.cos() * length;
    // 元: (A_long)(v + 0.5) を 0..n-1 で押さえる。切り捨ては 0 方向。
    let pick = |v: f64, n: i64| -> i64 { ((v + 0.5) as i64).clamp(0, n - 1) };
    for y in 0..h {
        for x in 0..w {
            let at = |k: f64| -> Px {
                let px = pick(x as f64 + cos_v + cos_v * offset * k, w);
                let py = pick(y as f64 + sin_v + sin_v * offset * k, h);
                src[(px + py * w) as usize]
            };
            let v = (1.0 - at(0.0).a / max) * (1.0 - at(1.0).a / max) * (1.0 - at(2.0).a / max);
            let a = if float {
                v.clamp(0.0, 1.0)
            } else {
                (v * max + 0.5).clamp(0.0, max).trunc()
            };
            buf.px[(x + y * w) as usize] = Px {
                a,
                r: a,
                g: a,
                b: a,
            };
        }
    }
}

/// 元の AlphaCopyR。RGB を指定色にして、アルファに入力のアルファを掛ける。
fn alpha_copy_r(buf: &mut Buf, src: &[Px], colour: &Px) {
    let max = buf.max;
    let float = buf.float;
    for (i, s) in src.iter().enumerate() {
        let p = &mut buf.px[i];
        p.r = colour.r;
        p.g = colour.g;
        p.b = colour.b;
        let v = p.a * s.a / max;
        p.a = if float { v } else { v.trunc() };
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Color,
    Rot,
    Length,
    Offset,
    Blur,
    Hyperbolic,
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
        // 元: PF_ADD_COLOR("color", 0xff, 0xFF, PF_MAX_CHAN8) = 白。
        params.add(
            Params::Color,
            "color",
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
        // 元: PF_ADD_ANGLE("rot", 0)
        params.add(
            Params::Rot,
            "rot",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("length", 0, 1000, 0, 100, 1, 0, 1, 0, 0)
        params.add(
            Params::Length,
            "length",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("offset", 0, 10, 1, 5, 1, 1, 1, 2, 0)
        params.add(
            Params::Offset,
            "offset",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(10.0);
                f.set_slider_min(1.0);
                f.set_slider_max(5.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("blur", 0, 1000, 0, 100, 0)
        params.add(
            Params::Blur,
            "blur",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(1000);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("hyperbolic", -10, 50, -2, 10, 1, 0, 1, 0, 0)
        params.add(
            Params::Hyperbolic,
            "hyperbolic",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-10.0);
                f.set_valid_max(50.0);
                f.set_slider_min(-2.0);
                f.set_slider_max(10.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
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
                out_data.set_return_msg("fs-rs innerflarerot");
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
        // 元の GetParams + Exec の頭。
        let colour8 = params.get(Params::Color)?.as_color()?.value();
        // 元: FIX2FLT(PF_Fixed) なので 0..360 に丸めずそのまま度へ。
        let rot = libs::utils::flt2fix(params.get(Params::Rot)?.as_angle()?.value() as f64) as f64
            / 65536.0;
        let length = params.get(Params::Length)?.as_float_slider()?.value() as f64
            * ds.num as f64
            / ds.den as f64;
        let offset = params.get(Params::Offset)?.as_float_slider()?.value() as f64;
        let blur = (params.get(Params::Blur)?.as_slider()?.value() as f64 * ds.num as f64
            / ds.den as f64
            + 0.5) as i64;
        let hyp = params.get(Params::Hyperbolic)?.as_float_slider()?.value() as f64;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $max:expr, $float:expr, $col:expr, $get:expr, $put:expr) => {{
                let mut src: Vec<Px> = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        src.push($get(in_layer.$rd(x, y)));
                    }
                }
                let mut buf = Buf {
                    w,
                    h,
                    px: src.clone(),
                    max: $max,
                    float: $float,
                };
                alpha_copy_dd(&mut buf, &src, rot, length, offset);
                if blur > 0 {
                    mult(&mut buf, false);
                    box_blur(&mut buf, blur);
                    mult(&mut buf, true);
                }
                hyperbolic(&mut buf, hyp);
                alpha_copy_r(&mut buf, &src, &$col);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), &buf.px[x + y * w]);
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                255.0,
                false,
                Px {
                    a: colour8.alpha as f64,
                    r: colour8.red as f64,
                    g: colour8.green as f64,
                    b: colour8.blue as f64
                },
                |p: &ae::Pixel8| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::Pixel8, s: &Px| {
                    d.alpha = s.a as i64 as u8;
                    d.red = s.r as i64 as u8;
                    d.green = s.g as i64 as u8;
                    d.blue = s.b as i64 as u8;
                }
            ),
            16 => {
                let c = libs::utils::conv_8_to_16(&colour8);
                go!(
                    as_pixel16,
                    as_pixel16_mut,
                    32768.0,
                    false,
                    Px {
                        a: c.alpha as f64,
                        r: c.red as f64,
                        g: c.green as f64,
                        b: c.blue as f64
                    },
                    |p: &ae::Pixel16| Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64
                    },
                    |d: &mut ae::Pixel16, s: &Px| {
                        d.alpha = s.a as i64 as u16;
                        d.red = s.r as i64 as u16;
                        d.green = s.g as i64 as u16;
                        d.blue = s.b as i64 as u16;
                    }
                )
            }
            _ => {
                let c = libs::utils::conv_8_to_32(&colour8);
                go!(
                    as_pixel32,
                    as_pixel32_mut,
                    1.0,
                    true,
                    Px {
                        a: c.alpha as f64,
                        r: c.red as f64,
                        g: c.green as f64,
                        b: c.blue as f64
                    },
                    |p: &ae::PixelF32| Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64
                    },
                    |d: &mut ae::PixelF32, s: &Px| {
                        d.alpha = s.a as f32;
                        d.red = s.r as f32;
                        d.green = s.g as f32;
                        d.blue = s.b as f32;
                    }
                )
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: f64) -> Px {
        Px {
            a: v,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }

    fn work(src: &[Px], w: usize, h: usize) -> Buf {
        Buf {
            w,
            h,
            px: src.to_vec(),
            max: 255.0,
            float: false,
        }
    }

    /// ずれ 0 なら3点とも自分なので、下書きは「透けぐあいの3乗」。
    #[test]
    fn no_length_cubes_the_transparency() {
        let src = vec![a(0.0), a(255.0), a(128.0)];
        let mut b = work(&src, 3, 1);
        alpha_copy_dd(&mut b, &src, 0.0, 0.0, 1.0);
        assert_eq!(b.px[0].a, 255.0, "完全に透明なら下書きは最大");
        assert_eq!(b.px[1].a, 0.0, "不透明なら下書きは 0");
        // (1 - 128/255)^3 * 255 + 0.5 = 32.0 → 32
        assert_eq!(b.px[2].a, 32.0, "{:?}", b.px[2]);
    }

    /// 角度 0 は上を向く (rot - 90° なので cos/sin が (0, -1))。
    #[test]
    fn angle_zero_looks_up() {
        // 縦 5。真ん中だけ不透明。
        let src = vec![a(0.0), a(0.0), a(255.0), a(0.0), a(0.0)];
        let mut b = work(&src, 1, 5);
        alpha_copy_dd(&mut b, &src, 0.0, 2.0, 0.0);
        // y=4 から 2 つ上 (y=2) の不透明を拾うので下書きが 0 になる。
        assert_eq!(b.px[4].a, 0.0);
        // y=0 は上に出られないので縁で押さえられ、y=0 自身を見る。
        assert_eq!(b.px[0].a, 255.0);
    }

    /// 角度 180 は下を向く。
    #[test]
    fn angle_one_eighty_looks_down() {
        let src = vec![a(0.0), a(0.0), a(255.0), a(0.0), a(0.0)];
        let mut b = work(&src, 1, 5);
        alpha_copy_dd(&mut b, &src, 180.0, 2.0, 0.0);
        assert_eq!(b.px[0].a, 0.0, "上の画素が下の不透明を拾う");
        assert_eq!(b.px[4].a, 255.0);
    }

    /// offset を入れると3点が離れ、拾う範囲が広がる。
    #[test]
    fn the_offset_spreads_the_three_taps() {
        let mut src = vec![a(0.0); 16];
        src[8] = a(255.0);
        let hit = |off: f64| {
            let mut b = work(&src, 1, 16);
            alpha_copy_dd(&mut b, &src, 180.0, 1.0, off);
            (0..16).filter(|y| b.px[*y].a < 255.0).count()
        };
        assert!(hit(3.0) > hit(0.0), "{} > {}", hit(3.0), hit(0.0));
    }

    /// 仕上げは色で塗って、入力のアルファで切り抜く。
    #[test]
    fn the_finish_cuts_with_the_input_alpha() {
        let src = vec![a(0.0), a(128.0), a(255.0)];
        let mut b = work(&[a(255.0), a(255.0), a(255.0)], 3, 1);
        let col = Px {
            a: 255.0,
            r: 1.0,
            g: 2.0,
            b: 3.0,
        };
        alpha_copy_r(&mut b, &src, &col);
        assert_eq!(b.px[0].a, 0.0);
        assert_eq!(b.px[1].a, 128.0);
        assert_eq!(b.px[2].a, 255.0);
        assert_eq!((b.px[2].r, b.px[2].g, b.px[2].b), (1.0, 2.0, 3.0));
    }
}
