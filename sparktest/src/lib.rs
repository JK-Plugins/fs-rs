//! F's Spark_Test の移植。
//!
//! Spark 系の部品 (`libs::spark`) の試し描き用。2つの点を太さつきで結ぶ
//! 「ふくらんだ線」を1本だけ引く。乱数も折りもなく、位置と太さを直に
//! 指定する。
//!
//! 手順 (元の Exec):
//!
//! 1. 始点と終点を押し込んで `Line()` (= `line_all`) で引く
//! 2. 下書きの「暗さ」をアルファにして色で塗る (`Colorize`)
//! 3. `blend` が入っていれば入力の上に重ねる (`RedBlendIn`)
//!
//! 元から写した細かいところ:
//!
//! - 点の座標は `(int)(fixed / 65536 + 0.5)`
//! - 太さには downsample を掛ける (丸めない)
//!
//! **出荷された .aex が無い**ので、元と画素を突き合わせる相手がいない。
//! 描く部品は `Spark` / `SparkRing` などで出荷 .aex と一致を確かめた
//! `libs::spark` そのままなので、ここはパラメータの読みと1本ぶんの
//! 呼び出しだけが新しい。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Spark_Test

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
#[allow(unused_imports)]
use libs::spark::{Chan, LineDraw, Point, PointList};

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    start: (i64, i64),
    start_s: f64,
    last: (i64, i64),
    last_s: f64,
}

/// 元の Exec の描くところ。下書き (赤のチャンネル) だけを返す。
fn draw<C: Chan>(info: &Info, w: i64, h: i64) -> Vec<C> {
    let mut ld: LineDraw<C> = LineDraw::new(w, h);
    ld.points.push(Point::new(info.start.0, info.start.1, info.start_s));
    ld.points.push(Point::new(info.last.0, info.last.1, info.last_s));
    ld.line_all();
    ld.buf
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Start,
    StartS,
    Last,
    LastS,
    Color,
    Blend,
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
        // 元: PF_ADD_POINT("startPoint", 25, 25)
        params.add(
            Params::Start,
            "startPoint",
            ae::PointDef::setup(|f| {
                f.set_default((25.0, 25.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("startSize", 0, 300, 0, 100, 1, 10, 1, 0, 0)
        params.add(
            Params::StartS,
            "startSize",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(10.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("lastPoint", 75, 75)
        params.add(
            Params::Last,
            "lastPoint",
            ae::PointDef::setup(|f| {
                f.set_default((75.0, 75.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("lastSize", 0, 300, 0, 100, 1, 10, 1, 0, 0)
        params.add(
            Params::LastS,
            "lastSize",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(10.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("color", 0xFF, 0xFF, 0xFF)
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
        // 元: PF_ADD_CHECKBOX("blend", "on", FALSE)
        params.add(
            Params::Blend,
            "blend",
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
                out_data.set_return_msg("fs-rs sparktest");
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
        #[allow(unused_variables)]
        let ds = {
            let d = in_data.downsample_x();
            d.num as f64 / d.den as f64
        };
        // 元の GetParams。角度は 0..360 に収める。
        #[allow(unused)]
        let ang = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            let v = libs::utils::flt2fix(params.get(p)?.as_angle()?.value() as f64) as i64;
            let mut r = v % (360 << 16);
            if r < 0 {
                r += 360 << 16;
            }
            Ok(r as f64 / 65536.0)
        };
        #[allow(unused)]
        let sl = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(params.get(p)?.as_slider()?.value() as i64)
        };
        // 元: (int)(v.x / 65536 + 0.5)。
        #[allow(unused)]
        let pt = |p: Params, params: &mut ae::Parameters<Params>| -> Result<(i64, i64), Error> {
            let (x, y) = params.get(p)?.as_point()?.value();
            Ok((
                ((x * 65536.0) as i64 as f64 / 65536.0 + 0.5) as i64,
                ((y * 65536.0) as i64 as f64 / 65536.0 + 0.5) as i64,
            ))
        };
        let info = Info {
            start: pt(Params::Start, params)?,
            // 元: 太さには downsample を掛けるだけ (丸めない)。
            start_s: params.get(Params::StartS)?.as_float_slider()?.value() as f64 * ds,
            last: pt(Params::Last, params)?,
            last_s: params.get(Params::LastS)?.as_float_slider()?.value() as f64 * ds,
        };
        let colour = params.get(Params::Color)?.as_color()?.value();
        let blend = params.get(Params::Blend)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        // 元の Colorize + RedBlendIn。下書きの「暗さ」をアルファにして
        // 色で塗り、blend が入っていれば入力の上に重ねる。
        macro_rules! run {
            ($chan:ty, $rd:ident, $wr:ident, $mk:expr, $bl:expr) => {{
                let buf: Vec<$chan> = draw(&info, w as i64, h as i64);
                for y in 0..h {
                    for x in 0..w {
                        let a = Chan::to_alpha(buf[x + y * w]);
                        let drawn = $mk(a);
                        *out_layer.$wr(x, y) = if blend {
                            $bl(*in_layer.$rd(x, y), drawn)
                        } else {
                            drawn
                        };
                    }
                }
            }};
        }
        match depth {
            8 => {
                let c = colour;
                run!(
                    u8,
                    as_pixel8,
                    as_pixel8_mut,
                    |a: u8| ae::Pixel8 {
                        alpha: a,
                        red: c.red,
                        green: c.green,
                        blue: c.blue
                    },
                    libs::spark::red_blend_in8
                )
            }
            16 => {
                let c = libs::utils::conv_8_to_16(&colour);
                run!(
                    u16,
                    as_pixel16,
                    as_pixel16_mut,
                    |a: u16| ae::Pixel16 {
                        alpha: a,
                        red: c.red,
                        green: c.green,
                        blue: c.blue
                    },
                    libs::spark::red_blend_in16
                )
            }
            _ => {
                let c = libs::utils::conv_8_to_32(&colour);
                run!(
                    f32,
                    as_pixel32,
                    as_pixel32_mut,
                    |a: f32| ae::PixelF32 {
                        alpha: a,
                        red: c.red,
                        green: c.green,
                        blue: c.blue
                    },
                    libs::spark::red_blend_in32
                )
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info {
            start: (20, 20),
            start_s: 10.0,
            last: (100, 60),
            last_s: 4.0,
        }
    }

    /// 線が引かれる。
    #[test]
    fn the_line_draws_something() {
        let buf: Vec<u8> = draw(&info(), 128, 80);
        let drawn = buf.iter().filter(|v| **v < 255).count();
        assert!(drawn > 200, "drawn={drawn}");
    }

    /// 太さを 0 にすると何も出ない。
    #[test]
    fn nothing_is_drawn_without_a_size() {
        let mut i = info();
        i.start_s = 0.0;
        i.last_s = 0.0;
        let buf: Vec<u8> = draw(&i, 128, 80);
        assert!(buf.iter().all(|v| *v == 255));
    }

    /// 始点のまわりは終点のまわりより太い。
    #[test]
    fn the_start_end_is_fatter() {
        let buf: Vec<u8> = draw(&info(), 128, 80);
        let span = |x: usize| (0..80).filter(|y| buf[x + y * 128] < 255).count();
        assert!(span(20) > span(100), "{} > {}", span(20), span(100));
    }

    /// 両端を入れ替えても同じところに引かれる。
    #[test]
    fn the_line_is_symmetric() {
        let a: Vec<u8> = draw(&info(), 128, 80);
        let mut i = info();
        std::mem::swap(&mut i.start, &mut i.last);
        std::mem::swap(&mut i.start_s, &mut i.last_s);
        let b: Vec<u8> = draw(&i, 128, 80);
        let n = |v: &Vec<u8>| v.iter().filter(|x| **x < 255).count();
        // 走る向きが変わるので画素は完全一致しないが、量はほぼ同じ。
        let (na, nb) = (n(&a) as i64, n(&b) as i64);
        assert!((na - nb).abs() * 20 < na, "{na} vs {nb}");
    }
}
