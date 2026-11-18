//! F's SparkStorm の移植。
//!
//! Spark の双子で、描き方 (折る・細い線をぶら下げる) はそっくり同じ。
//! 違うのは**両端の決め方**で、始点は start0〜start1 の線分の上、終点は
//! last0〜last1 の線分の上を、**同じ乱数を元にした近い割合**で選ぶ
//! (元の `PointStrom`)。帯のあいだに稲妻が何本も走る絵になる。
//!
//! 手順 (元の Exec):
//!
//! 1. 乱数の列を3本蒔く。位置用に `seed + seedPos`、揺らし用に
//!    `seed + seedMove`、両端の選び方に `seedDraw` (どれも 30000 で
//!    割った余り)
//! 2. `drawCount` 本ぶん繰り返す。`PointStrom` で両端を選び、それぞれの
//!    幅でずれを足す。両端の太さは 0
//! 3. `foldCount` 回だけ折って、`wipe` で途中まで消してから描く
//! 4. `subCount` 本ぶん、同じ両端で太さ 1/3・振れ幅 5/4・折る向きを逆に
//!    した細い線を描く。点が4つ以上あるときは、2番目と後ろから2番目を
//!    本線のものに差し替える
//!
//! `PointStrom` の割合の取り方:
//!
//! - 始点は `start0 + (start1 - start0) * 乱数`
//! - 終点は**同じ乱数に ±0.1 の揺らぎを足したもの**で、0..1 に丸める。
//!   始点と終点が近い割合になるので、稲妻は帯を縦に横切る
//!
//! 元から写した細かいところ:
//!
//! - フレーム番号はどこにも効かない (seed だけで絵が決まる)
//! - `wipe` は % 指定を 100 で割って 0..2 に丸める
//! - ずれの幅・太さ・振れ幅は downsample を掛けたあと整数に丸める
//! - 元の `PointStrom` は点の太さを入れないので、直後に 0 を入れている
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/SparkStorm

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
#[allow(unused_imports)]
use libs::spark::{Chan, LineDraw, Point, PointList};

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    seed: i64,
    seed_pos: i64,
    seed_move: i64,
    offset: f64,
    seed_draw: i64,
    wipe: f64,
    /// 元の point[4]: start0, start1, last0, last1
    point: [(i64, i64); 4],
    draw_count: i64,
    start_rand_x: i64,
    start_rand_y: i64,
    last_rand_x: i64,
    last_rand_y: i64,
    line_size: f64,
    line_move: i64,
    sub_count: i64,
    fold_count: i64,
}

/// 元の Exec の描くところ。下書き (赤のチャンネル) だけを返す。
fn draw<C: Chan>(info: &Info, w: i64, h: i64) -> Vec<C> {
    let mut ld: LineDraw<C> = LineDraw::new(w, h);
    let wrap = |v: i64| {
        let r = v % 30000;
        if r < 0 {
            r + 30000
        } else {
            r
        }
    };
    let mut rng = XorShift::new(wrap(info.seed + info.seed_pos) as u32);
    let mut rng_m = XorShift::new(wrap(info.seed + info.seed_move) as u32);
    let mut rng_r = XorShift::new(wrap(info.seed_draw) as u32);

    let tbl = [
        Point::new(info.point[0].0, info.point[0].1, 0.0),
        Point::new(info.point[1].0, info.point[1].1, 0.0),
        Point::new(info.point[2].0, info.point[2].1, 0.0),
        Point::new(info.point[3].0, info.point[3].1, 0.0),
    ];

    for _ in 0..info.draw_count {
        let (mut p0, mut p1) = PointList::point_strom(&tbl, &mut rng_r);
        p0 = PointList::random(p0, info.start_rand_x, info.start_rand_y, &mut rng);
        p1 = PointList::random(p1, info.last_rand_x, info.last_rand_y, &mut rng);
        p0.s = 0.0;
        p1.s = 0.0;
        ld.points.clear();
        ld.points.push(p0);
        ld.points.push(p1);
        for depth in 0..info.fold_count.max(0) {
            ld.points.calc_center_pos(
                info.line_size,
                info.line_move,
                info.offset,
                depth,
                &mut rng_m,
            );
        }
        ld.points.wipe(info.wipe);
        ld.line_all();

        if info.sub_count > 0 {
            let q0 = ld.points.first(0);
            let q1 = ld.points.last(0);
            let ff = ld.points.count() >= 4;
            let (q2, q3) = if ff {
                (ld.points.first(1), ld.points.last(1))
            } else {
                (q0, q1)
            };
            for _ in 0..info.sub_count {
                ld.points.clear();
                ld.points.push(q0);
                ld.points.push(q1);
                for depth in 0..info.fold_count.max(0) {
                    ld.points.calc_center_pos(
                        info.line_size / 3.0,
                        info.line_move * 5 / 4,
                        -info.offset,
                        depth,
                        &mut rng_m,
                    );
                }
                if ff {
                    let c = ld.points.count();
                    let mut a = q2;
                    a.s = 0.0;
                    ld.points.set(1, a);
                    let mut b = q3;
                    b.s = 0.0;
                    ld.points.set(c - 2, b);
                }
                ld.points.wipe(info.wipe);
                ld.line_all();
            }
        }
    }
    ld.buf
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Seed,
    SeedPos,
    SeedMove,
    Offset,
    SeedDraw,
    Wipe,
    Start0,
    Start1,
    Last0,
    Last1,
    DrawCount,
    StartRandX,
    StartRandY,
    LastRandX,
    LastRandY,
    LineSize,
    LineMove,
    SubCount,
    FoldCount,
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
        // 元: PF_ADD_SLIDER(seed/seedPos/seedMove, -30000, 30000, 0, 1000, 0)
        for (p, name) in [
            (Params::Seed, "seed"),
            (Params::SeedPos, "seedPos"),
            (Params::SeedMove, "seedMove"),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(-30000);
                    f.set_valid_max(30000);
                    f.set_slider_min(0);
                    f.set_slider_max(1000);
                    f.set_default(0);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_ANGLE("offset", 0.0)
        params.add(
            Params::Offset,
            "offset",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("seedDraw", -30000, 30000, 0, 1000, 0)
        params.add(
            Params::SeedDraw,
            "seedDraw",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-30000);
                f.set_valid_max(30000);
                f.set_slider_min(0);
                f.set_slider_max(1000);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("wipe(%)", 0, 200, 0, 200, 1, 100, 1, 0, 0)
        params.add(
            Params::Wipe,
            "wipe(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("start0", 15, 10)
        params.add(
            Params::Start0,
            "start0",
            ae::PointDef::setup(|f| {
                f.set_default((15.0, 10.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("start1", 85, 10)
        params.add(
            Params::Start1,
            "start1",
            ae::PointDef::setup(|f| {
                f.set_default((85.0, 10.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("last0", 15, 90)
        params.add(
            Params::Last0,
            "last0",
            ae::PointDef::setup(|f| {
                f.set_default((15.0, 90.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("last1", 85, 90)
        params.add(
            Params::Last1,
            "last1",
            ae::PointDef::setup(|f| {
                f.set_default((85.0, 90.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("drawCount", 1, 100, 1, 10, 5)
        params.add(
            Params::DrawCount,
            "drawCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(100);
                f.set_slider_min(1);
                f.set_slider_max(10);
                f.set_default(5);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("first_randX", 0, 500, 0, 100, 10)
        params.add(
            Params::StartRandX,
            "first_randX",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(500);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("first_randY", 0, 500, 0, 100, 10)
        params.add(
            Params::StartRandY,
            "first_randY",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(500);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("last_randX", 0, 500, 0, 100, 10)
        params.add(
            Params::LastRandX,
            "last_randX",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(500);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("last_randY", 0, 500, 0, 100, 10)
        params.add(
            Params::LastRandY,
            "last_randY",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(500);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("lineSize", 0, 300, 0, 100, 1, 25, 1, 0, 0)
        params.add(
            Params::LineSize,
            "lineSize",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(25.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("lineMove", 0, 2000, 0, 100, 50)
        params.add(
            Params::LineMove,
            "lineMove",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(2000);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(50);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("subCount", 0, 10, 0, 5, 3)
        params.add(
            Params::SubCount,
            "subCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(10);
                f.set_slider_min(0);
                f.set_slider_max(5);
                f.set_default(3);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("foldCount", 1, 8, 1, 6, 4)
        params.add(
            Params::FoldCount,
            "foldCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(8);
                f.set_slider_min(1);
                f.set_slider_max(6);
                f.set_default(4);
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
                out_data.set_return_msg("fs-rs sparkstorm");
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
            seed: sl(Params::Seed, params)?,
            seed_pos: sl(Params::SeedPos, params)?,
            seed_move: sl(Params::SeedMove, params)?,
            offset: ang(Params::Offset, params)?,
            seed_draw: sl(Params::SeedDraw, params)?,
            // 元: % を 100 で割って 0..2 で切る。
            wipe: (params.get(Params::Wipe)?.as_float_slider()?.value() as f64 / 100.0)
                .clamp(0.0, 2.0),
            point: [
                pt(Params::Start0, params)?,
                pt(Params::Start1, params)?,
                pt(Params::Last0, params)?,
                pt(Params::Last1, params)?,
            ],
            draw_count: sl(Params::DrawCount, params)?,
            start_rand_x: (sl(Params::StartRandX, params)? as f64 * ds + 0.5) as i64,
            start_rand_y: (sl(Params::StartRandY, params)? as f64 * ds + 0.5) as i64,
            last_rand_x: (sl(Params::LastRandX, params)? as f64 * ds + 0.5) as i64,
            last_rand_y: (sl(Params::LastRandY, params)? as f64 * ds + 0.5) as i64,
            // 元は PF_FpLong の変数に (A_long) を代入しているので整数になる。
            line_size: ((params.get(Params::LineSize)?.as_float_slider()?.value() as f64) * ds
                + 0.5) as i64 as f64,
            line_move: (sl(Params::LineMove, params)? as f64 * ds + 0.5) as i64,
            sub_count: sl(Params::SubCount, params)?,
            fold_count: sl(Params::FoldCount, params)?,
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
            seed: 0,
            seed_pos: 0,
            seed_move: 0,
            offset: 0.0,
            seed_draw: 0,
            wipe: 1.0,
            point: [(19, 13), (108, 13), (19, 115), (108, 115)],
            draw_count: 5,
            start_rand_x: 10,
            start_rand_y: 10,
            last_rand_x: 10,
            last_rand_y: 10,
            line_size: 25.0,
            line_move: 50,
            sub_count: 3,
            fold_count: 4,
        }
    }

    #[test]
    fn the_storm_draws_something() {
        let buf: Vec<u8> = draw(&info(), 128, 128);
        let drawn = buf.iter().filter(|v| **v < 255).count();
        assert!(drawn > 500, "drawn={drawn}");
    }

    #[test]
    fn the_seed_changes_the_picture() {
        let a: Vec<u8> = draw(&info(), 128, 128);
        let mut i = info();
        i.seed = 17;
        let b: Vec<u8> = draw(&i, 128, 128);
        assert_ne!(a, b);
    }

    /// 両端の選び方の種は専用の列。
    #[test]
    fn the_draw_seed_moves_the_ends() {
        let a: Vec<u8> = draw(&info(), 128, 128);
        let mut i = info();
        i.seed_draw = 8;
        let b: Vec<u8> = draw(&i, 128, 128);
        assert_ne!(a, b);
    }

    /// 本数を増やすと描かれる量が増える。
    #[test]
    fn more_bolts_draw_more() {
        let n = |dc: i64| {
            let mut i = info();
            i.draw_count = dc;
            let b: Vec<u8> = draw(&i, 128, 128);
            b.iter().filter(|v| **v < 255).count()
        };
        assert!(n(20) > n(2));
    }

    /// 始点と終点は近い割合で選ばれるので、稲妻は帯を縦に横切る。
    #[test]
    fn the_ends_stay_near_the_same_fraction() {
        let mut i = info();
        i.start_rand_x = 0;
        i.start_rand_y = 0;
        i.last_rand_x = 0;
        i.last_rand_y = 0;
        let tbl = [
            Point::new(i.point[0].0, i.point[0].1, 0.0),
            Point::new(i.point[1].0, i.point[1].1, 0.0),
            Point::new(i.point[2].0, i.point[2].1, 0.0),
            Point::new(i.point[3].0, i.point[3].1, 0.0),
        ];
        let mut rng = XorShift::new(0);
        for _ in 0..20 {
            let (a, b) = PointList::point_strom(&tbl, &mut rng);
            // 横の位置のずれは帯の幅の 10% + 丸めぶん。
            assert!((a.x - b.x).abs() <= 11, "a={} b={}", a.x, b.x);
        }
    }

    #[test]
    fn a_full_wipe_leaves_nothing() {
        let mut i = info();
        i.wipe = 0.0;
        let buf: Vec<u8> = draw(&i, 128, 128);
        assert!(buf.iter().all(|v| *v == 255));
    }

    #[test]
    fn nothing_is_drawn_without_a_size() {
        let mut i = info();
        i.line_size = 0.0;
        let buf: Vec<u8> = draw(&i, 128, 128);
        assert!(buf.iter().all(|v| *v == 255));
    }
}
