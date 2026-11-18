//! F's Paint の移植。
//!
//! 指定した1点から地続きで同じ色のところを塗りつぶす (バケツ)。
//! 塗りつぶし本体は `libs::paint` にある (`PaintMultPoint` と共有)。
//!
//! 手順 (元の Exec):
//!
//! 1. 入力をそのままコピーする
//! 2. `塗りつぶす` が入っていれば、位置の画素から地続きで同じ色の
//!    ところを指定色で塗る
//!
//! 元から写した細かいところ:
//!
//! - 位置は `PF_FixedPoint` を 16 ビット右シフトしたもの (切り下げ)
//! - 色のアルファは `255 * 不透明度` を固定小数のまま掛けて
//!   `>> 16` したもの
//! - 地続きの判定は RGB だけで、アルファは見ない
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **classic の `Render` が `Exec` を呼んでいない**。入力をコピーして
//!    パラメータを読むだけで終わっていて、塗りつぶしは SmartFX の
//!    経路にしかない。AE 7 以降は smart しか使わないので目には
//!    つかないが、classic でも塗るようにした (AEXCompat で塗りつぶしを
//!    突き合わせられるようになるのもここ)
//! 2. 元の塗りつぶしは種のキューを AE の `new_world` で取るのに、解放の
//!    条件が別のメンバを見ているので**ふつうはワールドが漏れる**
//!    (`libs::paint` の注を参照)。AEXCompat が元の smart 経路を回せない
//!    のもこれが理由。移植では持ち方そのものが無い
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Paint

use after_effects::{self as ae};
use libs::paint::{bucket, Px};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Executed,
    Pos,
    Color,
    Opacity,
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
        // 元: PF_ADD_CHECKBOX("塗りつぶす", "ON", TRUE)。名前は英語に。
        params.add(
            Params::Executed,
            "Paint",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("位置", 50, 50)
        params.add(
            Params::Pos,
            "Position",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("ペイント色", 0xFF, 0x00, 0x00)
        params.add(
            Params::Color,
            "Paint_Color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0,
                    blue: 0,
                });
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("不透明度(%)", 0, 100, 0, 100, 100, 1, 0, 0)
        params.add(
            Params::Opacity,
            "Opacity(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
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
                out_data.set_return_msg("fs-rs paint");
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
        // 元の GetParams。
        let executed = params.get(Params::Executed)?.as_checkbox()?.value();
        let (px, py) = params.get(Params::Pos)?.as_point()?.value();
        // 元: pos.x >> 16 (切り下げ)。
        let x = (libs::utils::flt2fix(px as f64) as i64) >> 16;
        let y = (libs::utils::flt2fix(py as f64) as i64) >> 16;
        let mut colour = params.get(Params::Color)?.as_color()?.value();
        // 元: RoundByteLong(255 * (PF_Fixed/100) >> 16)。
        let fx = libs::utils::flt2fix(
            params.get(Params::Opacity)?.as_float_slider()?.value() as f64 / 100.0,
        ) as i64;
        colour.alpha = ((255 * fx) >> 16).clamp(0, 255) as u8;

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if !executed || w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($wr:ident, $col:expr, $get:expr, $put:expr) => {{
                let mut data: Vec<Px> = Vec::with_capacity((w * h) as usize);
                for yy in 0..h {
                    for xx in 0..w {
                        data.push($get(out_layer.$wr(xx as usize, yy as usize)));
                    }
                }
                bucket(&mut data, w, h, x, y, $col);
                for yy in 0..h {
                    for xx in 0..w {
                        $put(
                            out_layer.$wr(xx as usize, yy as usize),
                            &data[(xx + yy * w) as usize],
                        );
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                Px {
                    a: colour.alpha as f64,
                    r: colour.red as f64,
                    g: colour.green as f64,
                    b: colour.blue as f64
                },
                |p: &mut ae::Pixel8| Px {
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
                // 元の paint16 は 8bit の色を CONVERT8TO16 で上げる。
                let c = libs::utils::conv_8_to_16(&colour);
                go!(
                    as_pixel16_mut,
                    Px {
                        a: c.alpha as f64,
                        r: c.red as f64,
                        g: c.green as f64,
                        b: c.blue as f64
                    },
                    |p: &mut ae::Pixel16| Px {
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
                let c = libs::utils::conv_8_to_32(&colour);
                go!(
                    as_pixel32_mut,
                    Px {
                        a: c.alpha as f64,
                        r: c.red as f64,
                        g: c.green as f64,
                        b: c.blue as f64
                    },
                    |p: &mut ae::PixelF32| Px {
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
