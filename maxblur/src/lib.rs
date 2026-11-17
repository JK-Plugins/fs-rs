//! F's MaxBlur の移植。
//!
//! 「膨張 (または収縮) → ぼかし → アルファのカーブ」を一度に掛ける。
//! 内側の光を作る下ごしらえ用で、`InnerFlare_old` / `InnerFlareRot_old`
//! も同じ Tiny_* の部品を使っている。
//!
//! 手順 (元の Exec):
//!
//! 1. 入力をそのままコピーする
//! 2. `max` と `blur` に downsample を掛けて四捨五入する。両方効かなければ
//!    終わり
//! 3. **乗算済みにする** (`TinyMultM(FALSE)`)。アルファ 0 の画素は RGB も 0
//! 4. `max` が 0 でなければ van Herk / Gil-Werman の最大値 (負なら最小値)
//!    フィルタを横→縦に掛ける (`MaxFast` と同じ実装)
//! 5. `blur` が 0 より大きければ**箱ぼかしを3回**掛ける。1回ぶんは横→縦で、
//!    窓は `2*blur+1`、縁は端の画素を伸ばして埋める
//! 6. **乗算を戻す** (`TinyMultM(TRUE)`)
//! 7. `hyperbolic` が 0 でなければアルファに
//!    `v = (1+k)t / (1+k t)` のカーブを掛ける。カーブ自体は薄いほうを
//!    持ち上げる形だが、**正の値では「反転してから掛けて戻す」**ので
//!    向きが逆になり、半端なアルファが薄くなる。負の値では濃くなる
//!
//! 元から写した細かいところ:
//!
//! - 「最大」の測り方は `(r*0.299 + g*0.587 + b*0.114) * 10 + alpha/max`。
//!   重みつき和は double で計算してから float に落とす (元が昇格するため)
//! - 箱ぼかしは走る方向ごとに作業用のバッファへ書いてから書き戻す。
//!   足し引きは double、書くときに 0..max で押さえて切り捨て
//! - 箱ぼかしの縁は `clamp(i, 0, 幅-1)` で端の画素を伸ばす
//! - 乗算と乗算戻しは 0..max で押さえてから切り捨て。アルファが最大の
//!   画素は触らない
//! - アルファのカーブは float (f32) で計算し、`v * max` を切り捨てる
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **縦の最大最小フィルタで、窓が画面の高さ以上のときの明るさの式から
//!    `*10` が落ちている** (`Max_SubV` の全列の枝のみ。`MaxFast` の
//!    同じ関数には付いている)。先頭の画素だけ 10 倍のまま比べるので、
//!    ほぼ必ず**列の先頭が勝ち**、絵がいちばん上の行の色で埋まる。
//!    `max` が画面の高さに届くと起きる (256×144 の絵で 144 から)
//!
//! 元から直していないところ:
//!
//! - `HyperbolicAlphaImpl` が `new AEGP_SuiteHandler(...)` を解放して
//!   いない (呼ぶたびに漏れる)。移植では持ち方そのものが無いので、
//!   自然に無くなっている
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MaxBlur

use after_effects::{self as ae};
use libs::tiny::{box_blur, hyperbolic, minmax, mult, Buf, Px, Quirk};

/// 元の Max_SubV の「窓が高さ以上」の枝だけ、重みつき和の *10 が落ちている
/// のを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    MinMax,
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
        // 元: PF_ADD_SLIDER("max", -1000, 1000, -10, 10, 0)
        params.add(
            Params::MinMax,
            "max",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-1000);
                f.set_valid_max(1000);
                f.set_slider_min(-10);
                f.set_slider_max(10);
                f.set_default(0);
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
                out_data.set_return_msg("fs-rs maxblur");
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
        let down =
            |v: i64| -> i64 { (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64 };
        // 元の GetParams + Exec の頭。
        let minmax_v = down(params.get(Params::MinMax)?.as_slider()?.value() as i64);
        let blur = down(params.get(Params::Blur)?.as_slider()?.value() as i64);
        let hyp = params.get(Params::Hyperbolic)?.as_float_slider()?.value() as f64;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if (blur <= 0 && minmax_v == 0) || w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($wr:ident, $max:expr, $float:expr, $get:expr, $put:expr) => {{
                let mut buf = Buf {
                    w,
                    h,
                    px: Vec::with_capacity(w * h),
                    max: $max,
                    float: $float,
                };
                for y in 0..h {
                    for x in 0..w {
                        buf.px.push($get(out_layer.$wr(x, y)));
                    }
                }
                mult(&mut buf, false);
                minmax(
                    &mut buf,
                    minmax_v,
                    minmax_v > 0,
                    if FAITHFUL { Quirk::WholeNo10 } else { Quirk::None },
                );
                box_blur(&mut buf, blur);
                mult(&mut buf, true);
                hyperbolic(&mut buf, hyp);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), &buf.px[x + y * w]);
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                255.0,
                false,
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
            16 => go!(
                as_pixel16_mut,
                32768.0,
                false,
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
            ),
            _ => go!(
                as_pixel32_mut,
                1.0,
                true,
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
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// 部品そのものの試験は libs::tiny にある。ここでは直したところが
    /// 入っていることだけ見る。
    #[test]
    fn the_fix_is_on() {
        assert!(!super::FAITHFUL);
    }
}
