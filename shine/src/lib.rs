//! F's Shine の移植。
//!
//! 指定した点から**外へ向かう光条**を描く。絵を縦横半分に縮めた下書きを
//! 作り、画素ごとに「自分から中心へ向かう線」を舐めながら明るさを足し、
//! 元の大きさへ戻す。
//!
//! 手順 (元の Exec):
//!
//! 1. 長さに downsample を掛けて四捨五入する
//! 2. 縦横半分の下書き2枚を用意する
//! 3. `useColor` が入っていなければ、入力を乗算済みにして 2×2 のうち
//!    **いちばん明るい画素**を下書き1へ。入っていれば、乗算済みの灰色の
//!    いちばん明るい値をアルファに入れた白を下書き1へ
//! 4. 下書き1から下書き2へ光条を引く。画素ごとに自分と中心のあいだを
//!    `length/2` 歩ぶん舐めて、`(残り歩数 / 歩数)` の重みで足し、
//!    `strong / 歩数` を掛ける
//! 5. 下書き2を双一次で出力の大きさへ戻す
//! 6. `useColor` が入っていなければ乗算を戻す。入っていれば指定色で塗る
//!
//! 元から写した細かいところ:
//!
//! - 中心の座標は下書きが半分なので 1/2 にする
//! - 舐める歩数は `len2 = min(length/2 を丸めたもの, 中心までの距離)`。
//!   **ループは `i = 1` から `len2` の手前まで**なので、中心そのものの
//!   画素は入らない
//! - 中心にぴったり乗った画素は `入力 * strong` で、アルファは RGB の最大
//! - 乗算は `値 * アルファ / 最大値 + 0.5` を切り捨て (つまり四捨五入)、
//!   乗算戻しは `値 / (アルファ/最大値)` を切り捨て
//! - 下書きの読み出しは双一次で、縁は端の画素を伸ばす
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **長さか強さが 0 以下のとき、出力に何も書かずに返る**
//!    (`infoP->outP->Copy(infoP->inP);` がコメントアウトされている)。
//!    長さの既定値が 0 なので、置いた直後は出力の世界が手つかずのまま
//!    返り絵が壊れる。入力をコピーするようにした
//! 2. 元は下書き2枚を `new_world` で作って**一度も解放しない**
//!    (`NFsWorld` のデストラクタは自前の行テーブルだけを片付ける)。
//!    1フレームごとにワールドが2つ漏れる。AEXCompat が元を回せないのも
//!    これが理由で、仕分けの oracle が `none` だった。移植では持ち方
//!    そのものが無い
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Shine

use after_effects::{self as ae};
use libs::shine::{
    copy_buf2_to_dst, copy_into_buf1, copy_into_buf1_gray, sub, sub_gray, Buf, Dir, Ops, Px,
};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Pos,
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
        // 元: PF_ADD_POINT("pos", 50, 50)
        params.add(
            Params::Pos,
            "pos",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
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
                out_data.set_return_msg("fs-rs shine");
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
        let (px, py) = params.get(Params::Pos)?.as_point()?.value();
        // 元: fixed / 65536。
        let pos_x = libs::utils::flt2fix(px as f64) as f64 / 65536.0;
        let pos_y = libs::utils::flt2fix(py as f64) as f64 / 65536.0;
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
        // 元: 下書きが半分なので中心も半分にする。
        let dir = Dir::ToPoint {
            cx: pos_x / 2.0,
            cy: pos_y / 2.0,
        };

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
