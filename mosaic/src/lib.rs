//! F's Mosaic の移植。
//!
//! size x size のブロックごとに平均色で塗りつぶす。位置で格子をずらし、
//! ちらつきを入れるとブロックごとに乱数で色を上下させる。
//!
//! 元の平均は**行ごとに合計して行幅で割り、それを足して行数で割る**整数
//! 演算なので、普通に総和/画素数とは結果が違う。そのまま移植してある。
//! アルファのちらつきは「もともと 229 未満のところだけ」に乗る
//! (元のコメント: 半透明のところのみランダムに)。
//!
//! 乱数はブロックごとに引く (ちらつきをグレーにすると1つ、そうでなければ
//! a,b,g,r の順に4つ)。種はフレーム毎に振る指定なら frame、そうでなければ 0。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしているので
//! 英語名にしてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Mosaic

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::fs_graphics::XorShift;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Size,
    Pos,
    Rand,
    RandGray,
    FrameRand,
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
        // 元: PF_ADD_SLIDER(サイズ, 1, 1920, 1, 256, 1, ID_SIZE)
        params.add(
            Params::Size,
            "Size",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(1920);
                f.set_slider_min(1);
                f.set_slider_max(256);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_POINT(位置, 50, 50, 0, ID_POS)
        params.add(
            Params::Pos,
            "Position",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FIXED(ちらつき, 0, 100, 0, 20, 0, 1, 0, 0, ID_RAND)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        params.add(
            Params::Rand,
            "Flicker(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(20.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;

        for (p, name) in [
            (Params::RandGray, "FlickerInGray"),
            (Params::FrameRand, "FlickerPerFrame"),
        ] {
            params.add(
                p,
                name,
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("ON");
                    f.set_default(false);
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
                out_data.set_return_msg("fs-rs mosaic");
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
        // 元の GetParams: サイズは downScale (+0.5 で丸め)、ちらつきは /100。
        let ds = in_data.downsample_x();
        let size = {
            let v = params.get(Params::Size)?.as_slider()?.value();
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        let (pos_x, pos_y) = {
            let (x, y) = params.get(Params::Pos)?.as_point()?.value();
            // 元は PF_Fixed を >>16 して整数にする (切り捨て)。
            (((x * 65536.0) as i64) >> 16, ((y * 65536.0) as i64) >> 16)
        };
        let rand = params.get(Params::Rand)?.as_float_slider()?.value() as f64 / 100.0;
        let rand_gray = params.get(Params::RandGray)?.as_checkbox()?.value();
        let frame_rand = params.get(Params::FrameRand)?.as_checkbox()?.value();

        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: サイズが 1 以下なら何もしない。
        if size <= 1 {
            return Ok(());
        }

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();
        let max: i64 = match depth {
            8 => MAX_CHANNEL8 as i64,
            16 => MAX_CHANNEL16 as i64,
            _ => 1, // 32bit は下で別に扱う
        };

        // 元の mosaic8: 格子の基点と個数。位置をサイズで割った余りから始める。
        let mut size_x = size;
        let mut size_y = size;
        let mut px = pos_x % size;
        if px > 0 {
            px -= size;
        }
        let mut cnt_x = (w - px) / size + i64::from((w - px) % size > 0);
        let mut py = pos_y % size;
        if py > 0 {
            py -= size;
        }
        let mut cnt_y = (h - py) / size + i64::from((h - py) % size > 0);
        // 元: ブロックが画面に対して大きすぎるときは画面1枚にまとめる。
        if px + size_x + size_x >= w {
            px = 0;
            size_x = w;
            cnt_x = 1;
        }
        if py + size_y + size_y >= h {
            py = 0;
            size_y = h;
            cnt_y = 1;
        }

        // 元: ちらつきの種はフレーム毎指定なら frame、そうでなければ 0。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = XorShift::new(if frame_rand { frame } else { 0 });
        // 元: randValue は深度の最大値 * rand。32bit は 1.0 ではなく
        // PF_MAX_CHAN16 の目盛りで引く。
        let rand_scale = if depth == 32 {
            MAX_CHANNEL16 as i64
        } else {
            max
        };
        let rand_value = (rand_scale as f64 * rand) as i64;

        // 深度ごとに「読む・書く」だけ分ける。値は深度の目盛りの整数で持つ
        // (32bit は 0..1 の実数なので別扱い)。
        for j in 0..cnt_y {
            for i in 0..cnt_x {
                let bx = px + i * size_x;
                let by = py + j * size_y;
                // ちらつきはブロックごとに引く。グレー指定なら1つ、
                // そうでなければ a, b, g, r の順に4つ。
                let (mut ra, mut rb, mut rg, mut rr) = (0i64, 0i64, 0i64, 0i64);
                if rand > 0.0 {
                    if rand_gray {
                        let v = rng.rand2(-rand_value, rand_value);
                        ra = v;
                        rb = v;
                        rg = v;
                        rr = v;
                    } else {
                        ra = rng.rand2(-rand_value, rand_value);
                        rb = rng.rand2(-rand_value, rand_value);
                        rg = rng.rand2(-rand_value, rand_value);
                        rr = rng.rand2(-rand_value, rand_value);
                    }
                }
                // 塗りつぶしは libs の `flat` (元の flat8/16/32) に任せる。
                // 32bit の加減値は 16bit の目盛りで引いた値を割ったもの。
                let k = if depth == 32 {
                    MAX_CHANNEL16 as f64
                } else {
                    1.0
                };
                libs::fs_mosaic::flat(
                    &out_layer,
                    depth,
                    bx,
                    by,
                    size_x,
                    size_y,
                    [ra as f64 / k, rr as f64 / k, rg as f64 / k, rb as f64 / k],
                );
            }
        }
        Ok(())
    }
}
