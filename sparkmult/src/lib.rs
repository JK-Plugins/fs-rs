//! F's SparkMult の移植。
//!
//! 最大7つの座標を順に結んだ折れ線をギザギザにして描く。両端は細く、
//! 途中の点は太さが乱数で振れる。本線を `drawCount` 本、そのそれぞれに
//! 細い線を `subCount` 本ぶら下げる。
//!
//! 手順 (元の Exec):
//!
//! 1. 乱数の列を2本蒔く。位置用に `seed + seedPos`、揺らし用に
//!    `seed + seedMove` (どちらも 30000 で割った余り)
//! 2. `pointCount` 個の点を順に置く。点ごとに決まった幅のずれを足し、
//!    **両端の太さは 0 (そのとき乱数を1つ捨てる)**、途中は
//!    `lineSize * (0.2 + 0.8 * 乱数)`
//! 3. `foldCount` 回だけ折って、`wipe` で途中まで消してから描く
//! 4. `subCount` 本ぶん、同じ点の並びにもう一度ずれを足し直して、
//!    太さ 1/3・振れ幅 5/4・折る向きを逆にした細い線を描く
//!
//! ずれの幅は3組のスライダーから配る。最初の点が `first_rand*`、
//! 最後の点 (`pointCount - 1` 番目) が `last_rand*`、残りが `mid_rand*`。
//!
//! 直したバグ:
//!
//! - **途中の点の縦のずれ幅に mid_randX を読んでいる**
//!   (`GetADD(ID_MID_RX, &my)`)。`mid_randY` はどこからも読まれず、
//!   縦も横のスライダーで動く。`mid_randY` を読むようにした
//!
//! 元から写した細かいところ:
//!
//! - フレーム番号はどこにも効かない (seed だけで絵が決まる)
//! - `wipe` は % 指定を 100 で割って 0..2 に丸める
//! - ずれの幅・太さ・振れ幅は downsample を掛けたあと整数に丸める
//! - 点の並びの入れ物だけ 2048 個 (他の Spark 系は 1024 個)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/SparkMult

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
#[allow(unused_imports)]
use libs::spark::{Chan, LineDraw, Point, PointList};

/// 真にすると元のままの振る舞い (バグ入り) になる。元と画素を突き合わせる
/// ためだけのもので、出荷するのは偽のほう。
const FAITHFUL: bool = false;

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    seed: i64,
    seed_pos: i64,
    seed_move: i64,
    offset: f64,
    wipe: f64,
    point_count: i64,
    point: [(i64, i64); 7],
    rand_x: [i64; 7],
    rand_y: [i64; 7],
    line_size: f64,
    line_move: i64,
    sub_count: i64,
    fold_count: i64,
    draw_count: i64,
}

/// 元の Exec の描くところ。下書き (赤のチャンネル) だけを返す。
fn draw<C: Chan>(info: &Info, w: i64, h: i64) -> Vec<C> {
    // 元: CLineDraw(.., 2048)。
    let mut ld: LineDraw<C> = LineDraw::with_point_size(w, h, 2048);
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

    let pc = info.point_count;
    for _ in 0..info.draw_count {
        ld.points.clear();
        let mut sub = [Point::default(); 10];
        for k in 0..pc {
            let ki = k as usize;
            let mut p = Point::new(info.point[ki].0, info.point[ki].1, 0.0);
            p = PointList::random(p, info.rand_x[ki], info.rand_y[ki], &mut rng);
            if k == 0 || k == pc - 1 {
                p.s = 0.0;
                // 元: 両端では太さの乱数を使わないが1つ捨てる。
                rng.next_double();
            } else {
                p.s = info.line_size * (0.2 + 0.8 * rng.next_double());
            }
            ld.points.push(p);
            sub[ki] = p;
        }
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

        for _ in 0..info.sub_count.max(0) {
            ld.points.clear();
            for k in 0..pc {
                let ki = k as usize;
                let p = PointList::random(sub[ki], info.rand_x[ki], info.rand_y[ki], &mut rng);
                ld.points.push(p);
            }
            for depth in 0..info.fold_count.max(0) {
                ld.points.calc_center_pos(
                    info.line_size / 3.0,
                    info.line_move * 5 / 4,
                    -info.offset,
                    depth,
                    &mut rng_m,
                );
            }
            ld.points.wipe(info.wipe);
            ld.line_all();
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
    PointCount,
    Point0,
    Point1,
    Point2,
    Point3,
    Point4,
    Point5,
    Point6,
    StartRandX,
    StartRandY,
    MidRandX,
    MidRandY,
    LastRandX,
    LastRandY,
    LineSize,
    LineMove,
    SubCount,
    FoldCount,
    DrawCount,
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
        // 元: PF_ADD_SLIDER("pointCount", 2, 7, 2, 7, 5)
        params.add(
            Params::PointCount,
            "pointCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(7);
                f.set_slider_min(2);
                f.set_slider_max(7);
                f.set_default(5);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point0", 0, 0)
        params.add(
            Params::Point0,
            "point0",
            ae::PointDef::setup(|f| {
                f.set_default((0.0, 0.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point1", 10, 10)
        params.add(
            Params::Point1,
            "point1",
            ae::PointDef::setup(|f| {
                f.set_default((10.0, 10.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point2", 20, 20)
        params.add(
            Params::Point2,
            "point2",
            ae::PointDef::setup(|f| {
                f.set_default((20.0, 20.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point3", 30, 30)
        params.add(
            Params::Point3,
            "point3",
            ae::PointDef::setup(|f| {
                f.set_default((30.0, 30.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point4", 40, 40)
        params.add(
            Params::Point4,
            "point4",
            ae::PointDef::setup(|f| {
                f.set_default((40.0, 40.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point5", 50, 50)
        params.add(
            Params::Point5,
            "point5",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("point6", 60, 60)
        params.add(
            Params::Point6,
            "point6",
            ae::PointDef::setup(|f| {
                f.set_default((60.0, 60.0));
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
        // 元: PF_ADD_SLIDER("mid_randX", 0, 500, 0, 100, 10)
        params.add(
            Params::MidRandX,
            "mid_randX",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(500);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("mid_randY", 0, 500, 0, 100, 10)
        params.add(
            Params::MidRandY,
            "mid_randY",
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
        // 元: PF_ADD_SLIDER("subCount", 0, 10, 0, 5, 2)
        params.add(
            Params::SubCount,
            "subCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(10);
                f.set_slider_min(0);
                f.set_slider_max(5);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("foldCount", 1, 5, 1, 5, 3)
        params.add(
            Params::FoldCount,
            "foldCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(5);
                f.set_slider_min(1);
                f.set_slider_max(5);
                f.set_default(3);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("drawCount", 1, 50, 1, 5, 1)
        params.add(
            Params::DrawCount,
            "drawCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(50);
                f.set_slider_min(1);
                f.set_slider_max(5);
                f.set_default(1);
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
                out_data.set_return_msg("fs-rs sparkmult");
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
        let down = |v: i64| (v as f64 * ds + 0.5) as i64;
        let point_count = sl(Params::PointCount, params)?;
        let mut rand_x = [0i64; 7];
        let mut rand_y = [0i64; 7];
        {
            let mx = down(sl(Params::MidRandX, params)?);
            // 元は縦にも mid_randX を読んでいて (GetADD(ID_MID_RX, &my))、
            // mid_randY はどこからも読まれない。直してある。
            let my = if FAITHFUL {
                down(sl(Params::MidRandX, params)?)
            } else {
                down(sl(Params::MidRandY, params)?)
            };
            for i in 0..7 {
                rand_x[i] = mx;
                rand_y[i] = my;
            }
            rand_x[0] = down(sl(Params::StartRandX, params)?);
            rand_y[0] = down(sl(Params::StartRandY, params)?);
            let p = (point_count - 1).clamp(0, 6) as usize;
            rand_x[p] = down(sl(Params::LastRandX, params)?);
            rand_y[p] = down(sl(Params::LastRandY, params)?);
        }
        let info = Info {
            seed: sl(Params::Seed, params)?,
            seed_pos: sl(Params::SeedPos, params)?,
            seed_move: sl(Params::SeedMove, params)?,
            offset: ang(Params::Offset, params)?,
            // 元: % を 100 で割って 0..2 で切る。
            wipe: (params.get(Params::Wipe)?.as_float_slider()?.value() as f64 / 100.0)
                .clamp(0.0, 2.0),
            point_count,
            point: [
                pt(Params::Point0, params)?,
                pt(Params::Point1, params)?,
                pt(Params::Point2, params)?,
                pt(Params::Point3, params)?,
                pt(Params::Point4, params)?,
                pt(Params::Point5, params)?,
                pt(Params::Point6, params)?,
            ],
            rand_x,
            rand_y,
            // 元は PF_FpLong の変数に (A_long) を代入しているので整数になる。
            line_size: ((params.get(Params::LineSize)?.as_float_slider()?.value() as f64) * ds
                + 0.5) as i64 as f64,
            line_move: down(sl(Params::LineMove, params)?),
            sub_count: sl(Params::SubCount, params)?,
            fold_count: sl(Params::FoldCount, params)?,
            draw_count: sl(Params::DrawCount, params)?,
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
            point_count: 5,
            point: [
                (0, 0),
                (13, 13),
                (26, 26),
                (38, 38),
                (51, 51),
                (64, 64),
                (77, 77),
            ],
            rand_x: [10, 10, 10, 10, 10, 10, 10],
            rand_y: [10, 10, 10, 10, 10, 10, 10],
            line_size: 25.0,
            line_move: 50,
            sub_count: 2,
            fold_count: 3,
            draw_count: 1,
        }
    }

    #[test]
    fn the_bolt_draws_something() {
        let buf: Vec<u8> = draw(&info(), 128, 128);
        let drawn = buf.iter().filter(|v| **v < 255).count();
        assert!(drawn > 200, "drawn={drawn}");
    }

    #[test]
    fn the_seed_changes_the_picture() {
        let a: Vec<u8> = draw(&info(), 128, 128);
        let mut i = info();
        i.seed = 23;
        let b: Vec<u8> = draw(&i, 128, 128);
        assert_ne!(a, b);
    }

    /// 点を増やすと折れ線が伸びる。
    #[test]
    fn more_points_make_a_longer_line() {
        let n = |c: i64| {
            let mut i = info();
            i.point_count = c;
            let b: Vec<u8> = draw(&i, 128, 128);
            b.iter().filter(|v| **v < 255).count()
        };
        assert!(n(7) > n(2));
    }

    /// 細い線を増やすと描かれる量が増える。
    #[test]
    fn more_sub_lines_draw_more() {
        let n = |s: i64| {
            let mut i = info();
            i.sub_count = s;
            let b: Vec<u8> = draw(&i, 128, 128);
            b.iter().filter(|v| **v < 255).count()
        };
        assert!(n(6) > n(0));
    }

    /// 縦のずれ幅は横とは別に効く (直したところ)。
    #[test]
    fn the_middle_spread_has_its_own_height() {
        let mut i = info();
        i.rand_x = [0; 7];
        i.rand_y = [0; 7];
        let flat: Vec<u8> = draw(&i, 128, 128);
        let mut j = i;
        j.rand_y = [0, 40, 40, 40, 0, 0, 0];
        let tall: Vec<u8> = draw(&j, 128, 128);
        assert_ne!(flat, tall);
    }

    #[test]
    fn a_full_wipe_leaves_nothing() {
        let mut i = info();
        i.wipe = 0.0;
        let buf: Vec<u8> = draw(&i, 128, 128);
        assert!(buf.iter().all(|v| *v == 255));
    }

    /// 出荷するのは直した版。
    #[test]
    fn the_shipped_build_is_the_fixed_one() {
        assert!(!FAITHFUL);
    }
}
