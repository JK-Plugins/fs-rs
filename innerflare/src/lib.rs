//! F's InnerFlare の移植。
//!
//! 絵の**内側**にそのかたちの光をにじませる。アルファを反転したものを
//! 下書きにして、膨張 (収縮) → ぼかし → アルファのカーブを掛け、最後に
//! 元のアルファで切り抜いて指定色で塗る。
//!
//! 手順 (元の Exec):
//!
//! 1. `AlphaCopyM`: 入力のアルファを**反転**して RGBA 全部に入れる
//!    (`reverce` が入っていれば反転しない)
//! 2. `max` が 0 でなければ van Herk / Gil-Werman の最大値 (負なら最小値)
//!    フィルタを横→縦に掛ける
//! 3. `blur` が 0 より大きければ、乗算済みにして箱ぼかしを3回掛け、
//!    乗算を戻す
//! 4. `hyperbolic` が 0 でなければアルファにカーブを掛ける
//! 5. `AlphaCopyRM`: RGB を指定色にして、**アルファに入力のアルファ
//!    (`reverce` なら反転したもの) を掛ける**。内側だけが残る
//!
//! 元から写した細かいところ:
//!
//! - `max` と `blur` には downsample を掛けて四捨五入する
//! - 最後の掛け算は `出力のアルファ * 入力のアルファ / 最大値` を
//!    切り捨てる
//! - `AlphaCopy` の「白を抜く」枝は、呼び出し側が常に FALSE を渡すので
//!   通らない
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **`max` も `blur` も 0 だと出力に何も書かずに返る**
//!    (`if ((blur <= 0) && (minmax == 0)) return;` で、中の
//!    `CopyInToOut()` はコメントアウトされている)。既定値がどちらも 0
//!    なので、置いた直後は出力の世界が手つかずのまま返り、絵が壊れる。
//!    打ち切りをやめて、ぼかしも膨張も無しの道を通すようにした
//!    (`AlphaCopy` → 色塗りだけが走る)
//! 直っているように見えて**結果は変わらないところ**:
//!
//! - 縦の最大最小フィルタの明るさの式から `*10` が落ちている
//!   (`Max_SubV` のふつうの経路の `lineLevel`。`MaxBlur` の同じファイルに
//!   は付いている)。ただしこの効果が最大最小を掛けるのは `AlphaCopy` が
//!   作った **R = G = B = アルファ** の下書きなので、重みつき和は
//!   ちょうどアルファと同じ値になり、`*10` があってもなくても大小の順は
//!   変わらない。`libs::tiny::Quirk::BlockNo10` で元の式に戻せるように
//!   してあるが、出る絵は同じ (AEXCompat で 9通り確かめた)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/InnerFlare_old

use after_effects::{self as ae};
use libs::tiny::{box_blur, hyperbolic, minmax, mult, Buf, Px, Quirk};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 元の AlphaCopy。アルファを (反転して) RGBA 全部に入れる。
fn alpha_copy(buf: &mut Buf, src: &[Px], reverse: bool) {
    let max = buf.max;
    for (i, s) in src.iter().enumerate() {
        let a = if reverse { s.a } else { max - s.a };
        buf.px[i] = Px {
            a,
            r: a,
            g: a,
            b: a,
        };
    }
}

/// 元の AlphaCopyR。RGB を指定色にして、アルファに入力のアルファを掛ける。
fn alpha_copy_r(buf: &mut Buf, src: &[Px], colour: &Px, reverse: bool) {
    let max = buf.max;
    let float = buf.float;
    for (i, s) in src.iter().enumerate() {
        let a = if reverse { max - s.a } else { s.a };
        let p = &mut buf.px[i];
        p.r = colour.r;
        p.g = colour.g;
        p.b = colour.b;
        let v = p.a * a / max;
        p.a = if float { v } else { v.trunc() };
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Color,
    MinMax,
    Blur,
    Hyperbolic,
    Reverce,
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
        // 元: PF_ADD_COLOR("color", 0xff, 0xFF, PF_MAX_CHAN8) = 白。
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
        // 元: PF_ADD_CHECKBOX("reverce", "on", FALSE)
        params.add(
            Params::Reverce,
            "reverce",
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
                out_data.set_return_msg("fs-rs innerflare");
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
        let colour8 = params.get(Params::Color)?.as_color()?.value();
        let minmax_v = down(params.get(Params::MinMax)?.as_slider()?.value() as i64);
        let blur = down(params.get(Params::Blur)?.as_slider()?.value() as i64);
        let hyp = params.get(Params::Hyperbolic)?.as_float_slider()?.value() as f64;
        let reverse = params.get(Params::Reverce)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }
        // 元: 両方 0 なら出力に何も書かずに返る (直したところ 1)。
        if FAITHFUL && blur <= 0 && minmax_v == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $max:expr, $float:expr, $col:expr, $get:expr, $put:expr) => {{
                let mut src: Vec<Px> = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        src.push($get(in_layer.$rd(x, y)));
                    }
                }
                let mut buf = Buf {
                    w,
                    h,
                    px: src.clone(),
                    max: $max,
                    float: $float,
                };
                alpha_copy(&mut buf, &src, reverse);
                minmax(
                    &mut buf,
                    minmax_v,
                    minmax_v > 0,
                    if FAITHFUL { Quirk::BlockNo10 } else { Quirk::None },
                );
                if blur > 0 {
                    mult(&mut buf, false);
                    box_blur(&mut buf, blur);
                    mult(&mut buf, true);
                }
                hyperbolic(&mut buf, hyp);
                alpha_copy_r(&mut buf, &src, &$col, reverse);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), &buf.px[x + y * w]);
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                255.0,
                false,
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
                // 元の _CONVERT8TO16。
                let c = libs::utils::conv_8_to_16(&colour8);
                go!(
                    as_pixel16,
                    as_pixel16_mut,
                    32768.0,
                    false,
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
                // 元の _CONVERT8TO32 (小数4桁に量子化)。
                let c = libs::utils::conv_8_to_32(&colour8);
                go!(
                    as_pixel32,
                    as_pixel32_mut,
                    1.0,
                    true,
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
    use super::*;

    fn buf(px: Vec<Px>) -> Buf {
        Buf {
            w: px.len(),
            h: 1,
            px,
            max: 255.0,
            float: false,
        }
    }

    fn a(v: f64) -> Px {
        Px {
            a: v,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        }
    }

    /// 下書きはアルファを反転したもの。RGB にも同じ値が入る。
    #[test]
    fn the_sketch_is_the_inverted_alpha() {
        let src = vec![a(0.0), a(64.0), a(255.0)];
        let mut b = buf(src.clone());
        alpha_copy(&mut b, &src, false);
        assert_eq!(b.px[0], a(255.0).with_rgb(255.0));
        assert_eq!(b.px[2].a, 0.0);
        assert_eq!(b.px[1].r, 191.0);
    }

    /// reverce なら反転しない。
    #[test]
    fn the_reverce_switch_keeps_the_alpha() {
        let src = vec![a(0.0), a(255.0)];
        let mut b = buf(src.clone());
        alpha_copy(&mut b, &src, true);
        assert_eq!(b.px[0].a, 0.0);
        assert_eq!(b.px[1].a, 255.0);
    }

    /// 仕上げは色で塗って、入力のアルファで切り抜く。
    #[test]
    fn the_finish_cuts_with_the_input_alpha() {
        let src = vec![a(0.0), a(128.0), a(255.0)];
        let mut b = buf(vec![a(255.0), a(255.0), a(255.0)]);
        let col = Px {
            a: 255.0,
            r: 10.0,
            g: 20.0,
            b: 30.0,
        };
        alpha_copy_r(&mut b, &src, &col, false);
        assert_eq!(b.px[0].a, 0.0, "入力が透明なら残らない");
        assert_eq!(b.px[2].a, 255.0, "入力が不透明ならそのまま");
        assert_eq!(b.px[1].a, 128.0);
        assert_eq!((b.px[1].r, b.px[1].g, b.px[1].b), (10.0, 20.0, 30.0));
    }

    /// reverce なら切り抜きも反転する。
    #[test]
    fn the_reverce_switch_flips_the_cut() {
        let src = vec![a(0.0), a(255.0)];
        let mut b = buf(vec![a(255.0), a(255.0)]);
        let col = a(255.0);
        alpha_copy_r(&mut b, &src, &col, true);
        assert_eq!(b.px[0].a, 255.0);
        assert_eq!(b.px[1].a, 0.0);
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }

    trait WithRgb {
        fn with_rgb(self, v: f64) -> Px;
    }
    impl WithRgb for Px {
        fn with_rgb(mut self, v: f64) -> Px {
            self.r = v;
            self.g = v;
            self.b = v;
            self
        }
    }
}
