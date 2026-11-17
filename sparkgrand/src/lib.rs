//! F's SparkGrand の移植。
//!
//! 1点から出て、2点が決める線分のどこかへ落ちる太い稲妻を描く。
//! 枝の生やし方が他の Spark 系と違っていて、**いちど描いた折れ線の尻尾を
//! 切り落として、別の落ち先へ描き直す**のを折りの段数ぶん繰り返す。
//! 根元は共通で、先だけが枝分かれして見える。
//!
//! 手順 (元の Exec):
//!
//! 1. 位置用と揺らし用の2つの乱数の列に、それぞれ `seed + seedPos` と
//!    `seed + seedMove` を蒔く (どちらも 30000 で割った余り)
//! 2. `drawCount` 本ぶん繰り返す。1本目だけは落ち先が last1 と last2 の
//!    **ちょうど中点**で、**そのぶん揺らしの乱数を1つ捨てる**。2本目から
//!    は線分の上の乱数の位置
//! 3. 始点と落ち先に、それぞれの乱数の幅でずれを足す
//! 4. `foldCount` 回だけ折って (中点を乱数でずらしながら挿む)、`wipe` で
//!    途中まで消してから描く
//! 5. 折りの段数を1つ落としながら、`2^段数` 個ぶん尻尾を切って別の
//!    落ち先を継ぎ足し、太さ 0.7 倍・振れ幅 4/3 倍で折り直して描く
//!    (段数が 2 より大きいあいだ)
//! 6. 1本ぶんの終わりに、**使った乱数の数を 50 まで水増しする**
//!    (位置用と揺らし用それぞれ)。本ごとの絵が互いにずれないようにする
//!    ための調整で、数え方も元のまま
//!
//! 元から写した細かいところ:
//!
//! - フレーム番号はどこにも効かない (seed だけで絵が決まる)
//! - `wipe` は % 指定を 100 で割って 0..2 に丸める。1 で消さない、
//!   0 寄りだと後ろから、2 寄りだと前から消える
//! - ずれの幅・太さ・振れ幅は downsample を掛けたあと整数に丸める
//! - 始点と落ち先の**太さは元では初期化されていない** (`ParamInfo` の
//!   `PointInfo` に `p` だけ入れて `s` を触らない)。出荷 .aex と画素が
//!   合うのは 0 としたときなので、0 としている
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/SparkGrand

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
    wipe: f64,
    start: (i64, i64),
    last1: (i64, i64),
    last2: (i64, i64),
    draw_count: i64,
    start_rand_x: i64,
    start_rand_y: i64,
    last_rand_x: i64,
    last_rand_y: i64,
    line_size: f64,
    line_move: i64,
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

    // 元は ParamInfo の PointInfo の s を触らないので、太さは 0 のまま。
    let start = Point::new(info.start.0, info.start.1, 0.0);
    let last1 = Point::new(info.last1.0, info.last1.1, 0.0);
    let last2 = Point::new(info.last2.0, info.last2.1, 0.0);

    for dc in 0..info.draw_count {
        let mut p0 = start;
        // 元: 1本目だけ中点。使わない乱数を1つ捨てて並びを合わせる。
        let mut p1 = if dc == 0 {
            let p = PointList::point_inter(last1, last2);
            rng_m.next_double();
            p
        } else {
            PointList::point_inter_random(last1, last2, &mut rng_m)
        };
        p0 = PointList::random(p0, info.start_rand_x, info.start_rand_y, &mut rng);
        p1 = PointList::random(p1, info.last_rand_x, info.last_rand_y, &mut rng);
        ld.points.clear();
        ld.points.push(p0);
        ld.points.push(p1);
        for depth in 0..info.fold_count {
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
        ld.points.backup();

        let pcnt = ld.points.count() as i64;
        let mut ff = info.fold_count - 1;
        // 元: 使った乱数の数を数えて、あとで 50 まで水増しする。
        let mut rc: i64 = 0;
        let mut rcm: i64 = 0;
        while ff > 2 {
            let pos = pcnt - 2i64.pow(ff as u32);
            if pos < 0 {
                break;
            }
            ld.points.remove_tail(pos as usize);
            let mut p1 = PointList::point_inter_random(last1, last2, &mut rng_m);
            rcm += 1;
            p1 = PointList::random(p1, info.last_rand_x, info.last_rand_y, &mut rng);
            rc += 2;
            ld.points.push(p1);
            for depth in 0..ff {
                ld.points.calc_center_pos2(
                    info.line_size * 0.7,
                    info.line_move * 4 / 3,
                    info.offset,
                    depth,
                    (pos - 1).max(0) as usize,
                    &mut rng_m,
                );
                rcm += 3;
            }
            ld.points.wipe(info.wipe);
            ld.line_all();
            ff -= 1;
        }
        for _ in rc..50 {
            rng.next_double();
        }
        for _ in rcm..50 {
            rng_m.next_double();
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
    Wipe,
    Start,
    Last1,
    Last2,
    DrawCount,
    StartRandX,
    StartRandY,
    LastRandX,
    LastRandY,
    LineSize,
    LineMove,
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
        // 元: PF_ADD_FLOAT_SLIDER("wipe", 0, 200, 0, 200, 1, 100, 1, 0, 0)
        params.add(
            Params::Wipe,
            "wipe",
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
        // 元: PF_ADD_POINT("start", 25, 10)
        params.add(
            Params::Start,
            "start",
            ae::PointDef::setup(|f| {
                f.set_default((25.0, 10.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("last1", 15, 90)
        params.add(
            Params::Last1,
            "last1",
            ae::PointDef::setup(|f| {
                f.set_default((15.0, 90.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("last2", 85, 90)
        params.add(
            Params::Last2,
            "last2",
            ae::PointDef::setup(|f| {
                f.set_default((85.0, 90.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("drawCount", 1, 10, 1, 5, 2)
        params.add(
            Params::DrawCount,
            "drawCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(10);
                f.set_slider_min(1);
                f.set_slider_max(5);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("startRandX", 0, 400, 0, 100, 10)
        params.add(
            Params::StartRandX,
            "startRandX",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(400);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("startRandY", 0, 400, 0, 100, 10)
        params.add(
            Params::StartRandY,
            "startRandY",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(400);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("lastRandX", 0, 400, 0, 100, 10)
        params.add(
            Params::LastRandX,
            "lastRandX",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(400);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("laxtRandY", 0, 400, 0, 100, 10)
        params.add(
            Params::LastRandY,
            "laxtRandY",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(400);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("lineSize", 0, 300, 0, 100, 1, 15, 1, 0, 0)
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
                f.set_default(15.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("lineMove", 0, 2000, 0, 100, 60)
        params.add(
            Params::LineMove,
            "lineMove",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(2000);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(60);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("foldCount", 2, 9, 2, 6, 4)
        params.add(
            Params::FoldCount,
            "foldCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(9);
                f.set_slider_min(2);
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
                out_data.set_return_msg("fs-rs sparkgrand");
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
            // 元: % を 100 で割って 0..2 で切る。
            wipe: (params.get(Params::Wipe)?.as_float_slider()?.value() as f64 / 100.0)
                .clamp(0.0, 2.0),
            start: pt(Params::Start, params)?,
            last1: pt(Params::Last1, params)?,
            last2: pt(Params::Last2, params)?,
            draw_count: sl(Params::DrawCount, params)?,
            start_rand_x: (sl(Params::StartRandX, params)? as f64 * ds + 0.5) as i64,
            start_rand_y: (sl(Params::StartRandY, params)? as f64 * ds + 0.5) as i64,
            last_rand_x: (sl(Params::LastRandX, params)? as f64 * ds + 0.5) as i64,
            last_rand_y: (sl(Params::LastRandY, params)? as f64 * ds + 0.5) as i64,
            // 元は PF_FpLong の変数に (A_long) を代入しているので整数になる。
            line_size: ((params.get(Params::LineSize)?.as_float_slider()?.value() as f64) * ds
                + 0.5) as i64 as f64,
            line_move: (sl(Params::LineMove, params)? as f64 * ds + 0.5) as i64,
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
            wipe: 1.0,
            start: (64, 14),
            last1: (19, 115),
            last2: (108, 115),
            draw_count: 2,
            start_rand_x: 10,
            start_rand_y: 10,
            last_rand_x: 10,
            last_rand_y: 10,
            line_size: 15.0,
            line_move: 60,
            fold_count: 4,
        }
    }

    /// 稲妻が描かれる。
    #[test]
    fn the_bolt_draws_something() {
        let buf: Vec<u8> = draw(&info(), 128, 128);
        let drawn = buf.iter().filter(|v| **v < 255).count();
        assert!(drawn > 200, "drawn={drawn}");
    }

    /// 種を変えると絵が変わる。
    #[test]
    fn the_seed_changes_the_picture() {
        let a: Vec<u8> = draw(&info(), 128, 128);
        let mut i = info();
        i.seed = 9;
        let b: Vec<u8> = draw(&i, 128, 128);
        assert_ne!(a, b);
    }

    /// wipe を 0 にすると何も残らない。
    #[test]
    fn a_full_wipe_leaves_nothing() {
        let mut i = info();
        i.wipe = 0.0;
        let buf: Vec<u8> = draw(&i, 128, 128);
        assert!(buf.iter().all(|v| *v == 255));
    }

    /// wipe を途中にすると描かれる量が減る。
    #[test]
    fn a_half_wipe_draws_less() {
        let full: Vec<u8> = draw(&info(), 128, 128);
        let mut i = info();
        i.wipe = 0.5;
        let half: Vec<u8> = draw(&i, 128, 128);
        let n = |b: &Vec<u8>| b.iter().filter(|v| **v < 255).count();
        assert!(n(&half) < n(&full), "{} < {}", n(&half), n(&full));
    }

    /// 本数を増やすと描かれる量が増える。
    #[test]
    fn more_bolts_draw_more() {
        let one: Vec<u8> = {
            let mut i = info();
            i.draw_count = 1;
            draw(&i, 128, 128)
        };
        let four: Vec<u8> = {
            let mut i = info();
            i.draw_count = 4;
            draw(&i, 128, 128)
        };
        let n = |b: &Vec<u8>| b.iter().filter(|v| **v < 255).count();
        assert!(n(&four) > n(&one));
    }

    /// 乱数の水増しのおかげで、1本目の絵は本数を変えても同じ。
    #[test]
    fn the_padding_keeps_the_first_bolt_steady() {
        let mut i = info();
        i.draw_count = 1;
        let one: Vec<u8> = draw(&i, 128, 128);
        i.draw_count = 2;
        let two: Vec<u8> = draw(&i, 128, 128);
        // 1本目で描いたところは2本目でも暗いままのはず。
        for (a, b) in one.iter().zip(two.iter()) {
            if *a < 255 {
                assert!(*b <= *a);
            }
        }
    }
}
