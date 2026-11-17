//! F's SparkRing の移植。
//!
//! 楕円の上に点を並べ、隣どうしを「ギザギザに折った線」でつないで輪にする。
//! 電気が輪になって走っているような絵。
//!
//! 手順:
//!
//! 1. 位置用と揺らし用の2つの乱数の列に、それぞれ `seed + seedPos` と
//!    `seed + seedMove` を蒔く (どちらも 30000 で割った余り)
//! 2. 中心から `point_rot + 360/個数 * i` の向きに半径 `radius` (縦は
//!    `aspect` 倍)、全体を `rot` 度回したところに点を置く。点ごとに
//!    `point_rand` の幅で乱数のずれを足し、太さは `lineSize` × 乱数
//! 3. 隣り合う2点をつなぐ折れ線を `foldCount` 回だけ細かくして描く
//!    (中点を乱数でずらしながら挿んでいく)
//! 4. `subCount` 回ぶん、1つ飛ばした点とのあいだにも細い線を描く
//!    (太さは 1/3、振れ幅は 5/4、折る向きは逆)
//! 5. 下書き (白地に暗く描いた赤) をアルファに読み替えて色を塗る
//! 6. blend が入っていれば入力の上に重ねる。入っていなければ**入力は
//!    消える** (下書きが出力ワールドそのものなので)
//!
//! 元から写した細かいところ:
//!
//! - フレーム番号はどこにも効かない (seed だけで絵が決まる)
//! - 半径・太さ・振れ幅は downsample を掛けたあと**整数に丸める**
//!   (`(A_long)(v * ds + 0.5)`)
//! - 中心の座標は固定小数を 65536 で割って +0.5 して整数にする
//! - 点ごとの乱数のずれは、幅が 0 でも乱数を2つ捨てる
//! - 折れ線の太さは 1 で下限を切らない (元はその行をコメントアウトして
//!   いる)
//!
//! 元の `ParamsSetup` は中心の座標のスライダーにも `ID_OFFSET` と
//! `STR_OFFSET` を渡していて、名前が「offset」と重なっている
//! (パラメータは並び順で決まるので描画には効かない)。名前は "center" に
//! してある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/SparkRing

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
use libs::spark::{Chan, LineDraw, Point, PointList};

const POINT_COUNT_MAX: i32 = 36;

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    seed: i64,
    seed_pos: i64,
    seed_move: i64,
    offset: f64,
    center: (i64, i64),
    blur: i64,
    aspect: f64,
    point_count: i64,
    point_rot: f64,
    point_round: i64,
    rot: f64,
    line_size: f64,
    line_move: i64,
    fold_count: i64,
    sub_count: i64,
    draw_count: i64,
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

    let dr = 360.0 / info.point_count as f64;
    let mut points = [Point::default(); POINT_COUNT_MAX as usize];

    for _ in 0..info.draw_count {
        for pc in 0..info.point_count {
            let mut p = PointList::en_from_rot(
                info.center.0,
                info.center.1,
                info.point_rot + dr * pc as f64,
                info.blur,
                info.aspect,
                info.rot,
            );
            p = PointList::random(p, info.point_round, info.point_round, &mut rng);
            p.s = info.line_size * rng.next_double();
            points[pc as usize] = p;
        }
        for pc in 0..info.point_count {
            let p0 = points[pc as usize];
            let p1 = points[((pc + 1) % info.point_count) as usize];
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
            ld.line_all();

            for _ in 0..info.sub_count.max(0) {
                ld.points.clear();
                let p0 = points[pc as usize];
                let p1 = points[((pc + 2) % info.point_count) as usize];
                ld.points.push(p1);
                ld.points.push(p0);
                for depth in 0..info.fold_count.max(0) {
                    ld.points.calc_center_pos(
                        info.line_size / 3.0,
                        info.line_move * 5 / 4,
                        -info.offset,
                        depth,
                        &mut rng_m,
                    );
                }
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
    Center,
    Radius,
    Aspect,
    PointCount,
    PointRot,
    PointRand,
    Rot,
    LineSize,
    LineMove,
    FoldCount,
    SubCount,
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
        // 元: PF_ADD_ANGLE("offset", 0)
        params.add(
            Params::Offset,
            "offset",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("offset", 50, 50) — 名前と ID が offset と
        // 重なっているが、並び順で決まるので中心の座標。
        params.add(
            Params::Center,
            "center",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("radius", 10, 5000, 10, 1000, 500)
        params.add(
            Params::Radius,
            "radius",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(10);
                f.set_valid_max(5000);
                f.set_slider_min(10);
                f.set_slider_max(1000);
                f.set_default(500);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("aspect", 0, 10, 0, 2, 1, 1, 1, 0, 0)
        params.add(
            Params::Aspect,
            "aspect",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(10.0);
                f.set_slider_min(0.0);
                f.set_slider_max(2.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("point_count", 3, 36, 3, 24, 10)
        params.add(
            Params::PointCount,
            "point_count",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(3);
                f.set_valid_max(POINT_COUNT_MAX);
                f.set_slider_min(3);
                f.set_slider_max(24);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::PointRot,
            "point_rot",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("point_rand", 0, 300, 0, 100, 30)
        params.add(
            Params::PointRand,
            "point_rand",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(300);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(30);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::Rot,
            "rot",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
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
        // 元: PF_ADD_SLIDER("foldCount", 1, 9, 1, 6, 3)
        params.add(
            Params::FoldCount,
            "foldCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(9);
                f.set_slider_min(1);
                f.set_slider_max(6);
                f.set_default(3);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("subCount", 0, 20, 0, 10, 2)
        params.add(
            Params::SubCount,
            "subCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(20);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("drawCount", 1, 10, 1, 5, 1)
        params.add(
            Params::DrawCount,
            "drawCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(10);
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
                out_data.set_return_msg("fs-rs sparkring");
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
        let ds = ds.num as f64 / ds.den as f64;
        // 元の GetParams。角度は 0..360 に収め、長さは downsample を掛けて
        // 整数に丸める。
        let ang = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            let v = libs::utils::flt2fix(params.get(p)?.as_angle()?.value() as f64) as i64;
            let mut r = v % (360 << 16);
            if r < 0 {
                r += 360 << 16;
            }
            Ok(r as f64 / 65536.0)
        };
        let sl = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(params.get(p)?.as_slider()?.value() as i64)
        };
        let info = Info {
            seed: sl(Params::Seed, params)?,
            seed_pos: sl(Params::SeedPos, params)?,
            seed_move: sl(Params::SeedMove, params)?,
            offset: ang(Params::Offset, params)?,
            center: {
                let (x, y) = params.get(Params::Center)?.as_point()?.value();
                // 元: (int)(v.x / 65536 + 0.5)。
                (
                    ((x * 65536.0) as i64 as f64 / 65536.0 + 0.5) as i64,
                    ((y * 65536.0) as i64 as f64 / 65536.0 + 0.5) as i64,
                )
            },
            blur: (sl(Params::Radius, params)? as f64 * ds + 0.5) as i64,
            aspect: params.get(Params::Aspect)?.as_float_slider()?.value(),
            point_count: sl(Params::PointCount, params)?,
            point_rot: ang(Params::PointRot, params)?,
            point_round: sl(Params::PointRand, params)?,
            rot: ang(Params::Rot, params)?,
            // 元は PF_FpLong の変数に (A_long) を代入しているので整数になる。
            line_size: ((params.get(Params::LineSize)?.as_float_slider()?.value() as f64) * ds
                + 0.5) as i64 as f64,
            line_move: (sl(Params::LineMove, params)? as f64 * ds + 0.5) as i64,
            fold_count: sl(Params::FoldCount, params)?,
            sub_count: sl(Params::SubCount, params)?,
            draw_count: sl(Params::DrawCount, params)?,
        };
        let colour = params.get(Params::Color)?.as_color()?.value();
        let blend = params.get(Params::Blend)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! run {
            ($chan:ty, $rd:ident, $wr:ident, $mk:expr, $blend:expr) => {{
                let buf: Vec<$chan> = draw(&info, w as i64, h as i64);
                for y in 0..h {
                    for x in 0..w {
                        // 元の Colorize: 赤の暗さをアルファにして色で塗る。
                        let a = Chan::to_alpha(buf[x + y * w]);
                        let drawn = $mk(a);
                        let v = if blend {
                            $blend(*in_layer.$rd(x, y), drawn)
                        } else {
                            drawn
                        };
                        *out_layer.$wr(x, y) = v;
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
            center: (64, 64),
            blur: 40,
            aspect: 1.0,
            point_count: 10,
            point_rot: 0.0,
            point_round: 0,
            rot: 0.0,
            line_size: 10.0,
            line_move: 20,
            fold_count: 3,
            sub_count: 0,
            draw_count: 1,
        }
    }

    /// 輪が描かれて、下書きが暗くなる画素が出る。
    #[test]
    fn the_ring_draws_something() {
        let buf: Vec<u8> = draw(&info(), 128, 128);
        let drawn = buf.iter().filter(|v| **v < 255).count();
        assert!(drawn > 200, "drawn={drawn}");
    }

    /// 種を変えると絵が変わる。
    #[test]
    fn the_seed_changes_the_picture() {
        let a: Vec<u8> = draw(&info(), 128, 128);
        let mut i = info();
        i.seed = 5;
        let b: Vec<u8> = draw(&i, 128, 128);
        assert_ne!(a, b);
    }

    /// 位置の種と揺らしの種は別の列。揺らしだけ変えても点の位置は同じ
    /// なので、輪の通る範囲 (外接矩形) は変わらない。
    #[test]
    fn the_two_seeds_feed_different_streams() {
        let mut i = info();
        i.point_round = 20;
        let a: Vec<u8> = draw(&i, 128, 128);
        let mut j = i;
        j.seed_move = 11;
        let b: Vec<u8> = draw(&j, 128, 128);
        assert_ne!(a, b, "揺らしの種で絵は変わる");
        let mut k = i;
        k.seed_pos = 11;
        let c: Vec<u8> = draw(&k, 128, 128);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    /// 太さ 0 なら何も出ない。
    #[test]
    fn nothing_is_drawn_without_a_size() {
        let mut i = info();
        i.line_size = 0.0;
        let buf: Vec<u8> = draw(&i, 128, 128);
        assert!(buf.iter().all(|v| *v == 255));
    }

    /// aspect を小さくすると縦に潰れる。
    #[test]
    fn the_aspect_squashes_the_ring() {
        let mut i = info();
        i.fold_count = 1;
        i.line_move = 0;
        i.aspect = 0.2;
        let buf: Vec<u8> = draw(&i, 128, 128);
        let rows: Vec<usize> = (0..128)
            .filter(|y| (0..128).any(|x| buf[x + y * 128] < 255))
            .collect();
        let hgt = rows.last().unwrap() - rows.first().unwrap();
        assert!(hgt < 40, "hgt={hgt}");
    }

    /// 下書きのアルファは、描いたところで立つ。
    #[test]
    fn the_alpha_follows_the_sketch() {
        let buf: Vec<u8> = draw(&info(), 128, 128);
        assert!(buf.iter().any(|v| Chan::to_alpha(*v) > 0));
    }
}
