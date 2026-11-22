//! F's ShineParallel の移植。
//!
//! `Shine` の平行版。光条が一点から放射するのではなく、**指定した角度の
//! 一方向**へ伸びる。下書きの作り方・足し方・戻し方は `Shine` と同じで、
//! 舐める向きだけが違う (`libs::shine` を共有)。
//!
//! 元から写した細かいところ:
//!
//! - 向きは `rot` を 0..360 に収めてから、長さ `length` のベクトルにする
//!   (0 が上、時計回り。象限ごとの場合分けも元の `CRot` のまま)
//! - 長さに downsample を掛けて四捨五入する
//! - そのほかは `Shine` と同じ (doc はそちらを参照)
//!
//! 元と違うところ:
//!
//! 1. **元のソースはそのままではビルドできない**。`ParamInfo` から
//!    `pos` を外して `rot` / `offsetX` / `offsetY` に替えたのに、
//!    光条を引く `sub8/16/32` と `subG8/16/32` が `infoP->pos.x` を
//!    読んだままになっている。出荷された .aex が無いのもそのためと
//!    思われる。`GetParams` が `CRot` で出している `offsetX` /
//!    `offsetY` (角度と長さのベクトル) こそが使うつもりだったものなので、
//!    舐める向きをそれにした
//! 2. `Shine` と同じく、長さか強さが 0 以下のときに出力へ何も書かずに
//!    返るところを、入力のコピーにした
//! 3. `Shine` と同じく、下書きのワールドが漏れるところは移植では無い
//!
//! なお `GetParams` には `if (r < 0) infoP->rot += (360 << 16);` という行が
//! あるが、`infoP->rot` はまだ値が入っていないうえ、直後に
//! `infoP->rot = r / 65536.0` で上書きされるので効かない。角度を
//! 0..360 に収めるのは `CRot::RoundRot` のほうでやっている。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ShineParallel

use after_effects::{self as ae};
use libs::shine::{
    copy_buf2_to_dst, copy_into_buf1, copy_into_buf1_gray, sub, sub_gray, Buf, Dir, Ops, Px,
};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Rot,
    Length,
    Strong,
    IsColor,
    Color,
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
        // 元: PF_ADD_ANGLE("rot", 45)
        params.add(
            Params::Rot,
            "rot",
            ae::AngleDef::setup(|f| {
                f.set_default(45.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("length", 0, 1000, 0, 200, 0)
        params.add(
            Params::Length,
            "length",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(1000);
                f.set_slider_min(0);
                f.set_slider_max(200);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER("strong", 0, 1000, 0, 200, 1, 100, 1, 0, 0)
        params.add(
            Params::Strong,
            "strong",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(1000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(200.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("useColor", "on", FALSE)
        params.add(
            Params::IsColor,
            "useColor",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
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
                out_data.set_return_msg("fs-rs shineparallel");
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
        let dsx = ds.num as f64 / ds.den as f64;
        // 元の GetParams。
        // 元: PF_Fixed を 0..360 に収めてから度にする。
        let r = libs::utils::flt2fix(params.get(Params::Rot)?.as_angle()?.value() as f64)
            as i64
            % (360 << 16);
        let rot = r as f64 / 65536.0;
        let length_raw = params.get(Params::Length)?.as_slider()?.value().max(0) as i64;
        let strong =
            (params.get(Params::Strong)?.as_float_slider()?.value() as f64 / 100.0).max(0.0);
        let is_colour = params.get(Params::IsColor)?.as_checkbox()?.value();
        let colour8 = params.get(Params::Color)?.as_color()?.value();
        // 元の Exec: 長さに downsample を掛けて四捨五入。
        let length = (length_raw as f64 * dsx + 0.5) as i64;

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }
        // 元は長さか強さが 0 以下のとき、出力に何も書かずに返る
        // (中の Copy はコメントアウトされている)。入力をコピーするようにした。
        if length <= 0 || strong <= 0.0 {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }
        // 元の GetParams が CRot で出している向きのベクトル。
        let (ox, oy) = libs::shine::rot_vector(rot, length as f64);
        let dir = Dir::Parallel { dx: ox, dy: oy };

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $col:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                let mut src = Buf::new(w, h);
                for y in 0..h {
                    for x in 0..w {
                        src.set(x, y, $get(in_layer.$rd(x as usize, y as usize)));
                    }
                }
                // 元: 下書きは縦横半分。
                let mut buf1 = Buf::new(w / 2, h / 2);
                let mut buf2 = Buf::new(w / 2, h / 2);
                let mut out = Buf::new(w, h);
                if is_colour {
                    copy_into_buf1_gray(&src, &mut buf1, &ops);
                    sub_gray(&buf1, &mut buf2, &dir, length, strong, &$col, &ops);
                    copy_buf2_to_dst(&buf2, &mut out, &ops);
                } else {
                    copy_into_buf1(&src, &mut buf1, &ops);
                    sub(&buf1, &mut buf2, &dir, length, strong, &ops);
                    copy_buf2_to_dst(&buf2, &mut out, &ops);
                    // 元: 色を使わないときだけ乗算を戻す。
                    for p in out.px.iter_mut() {
                        *p = ops.from_mat(p);
                    }
                }
                for y in 0..h {
                    for x in 0..w {
                        $put(
                            out_layer.$wr(x as usize, y as usize),
                            &out.px[(x + y * w) as usize],
                        );
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                Ops {
                    max: 255.0,
                    float: false
                },
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
                    Ops {
                        max: 32768.0,
                        float: false
                    },
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
                    Ops {
                        max: 1.0,
                        float: true
                    },
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
    /// 光条そのものの試験は libs::shine にある。
    #[test]
    fn the_parts_live_in_libs() {
        let (x, y) = libs::shine::rot_vector(90.0, 10.0);
        assert!((x - 10.0).abs() < 1e-9 && y.abs() < 1e-9);
    }
}
