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
use libs::minmax_vhgw::{level, line_pass};

/// 元の Max_SubV の「窓が高さ以上」の枝だけ、重みつき和の *10 が落ちている
/// のを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 元の lineLevel から `*10` を外したもの (直したところ 1 の忠実版用)。
fn level_no10(r: f64, g: f64, b: f64, a: f32, max_chan: f32) -> f32 {
    ((r * 0.299 + g * 0.587 + b * 0.114) + (a / max_chan) as f64) as f32
}

/// 1画素。深度に依らず f64 で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Px {
    a: f64,
    r: f64,
    g: f64,
    b: f64,
}

/// 画素の並び。
struct Buf {
    w: usize,
    h: usize,
    px: Vec<Px>,
    max: f64,
    float: bool,
}

impl Buf {
    fn at(&self, o: usize, i: usize, vert: bool) -> usize {
        if vert {
            i * self.w + o
        } else {
            o * self.w + i
        }
    }
    /// 元の static_cast<CH>(値) 相当。0..max で押さえてから落とす。
    fn cast(&self, v: f64) -> f64 {
        let v = v.clamp(0.0, self.max);
        if self.float {
            v
        } else {
            v.trunc()
        }
    }
}

/// 元の TinyMultM。`un` が立っていれば乗算を戻す。
fn mult(buf: &mut Buf, un: bool) {
    let max = buf.max;
    let float = buf.float;
    for p in buf.px.iter_mut() {
        if p.a <= 0.0 {
            p.r = 0.0;
            p.g = 0.0;
            p.b = 0.0;
            continue;
        }
        if p.a >= max {
            continue;
        }
        let v = if un { max / p.a } else { p.a / max };
        let f = |x: f64| {
            let x = (x * v).clamp(0.0, max);
            if float {
                x
            } else {
                x.trunc()
            }
        };
        p.r = f(p.r);
        p.g = f(p.g);
        p.b = f(p.b);
    }
}

/// 元の Max_SubH / Max_SubV (Tiny_MinMax)。`MaxFast` と同じ実装。
fn minmax(buf: &mut Buf, radius: i64, is_max: bool) {
    if radius == 0 {
        return;
    }
    let radius = radius.abs();
    let max = buf.max as f32;
    for vert in [false, true] {
        let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
        let mut src: Vec<Px> = Vec::with_capacity(inner);
        let mut levels: Vec<f32> = Vec::with_capacity(inner);
        let mut dst: Vec<Px> = Vec::with_capacity(inner);
        for o in 0..outer {
            src.clear();
            levels.clear();
            for i in 0..inner {
                let p = buf.px[buf.at(o, i, vert)];
                levels.push(level(p.r, p.g, p.b, p.a as f32, max));
                src.push(p);
            }
            // 元は縦の「窓が高さ以上」の枝だけ、2番目以降の明るさから
            // *10 が落ちている (直したところ 1)。先頭だけ 10 倍のまま
            // なので、ほぼ必ず先頭が勝ってしまう。
            if FAITHFUL && vert && radius >= inner as i64 {
                for i in 1..inner {
                    let p = src[i];
                    levels[i] = level_no10(p.r, p.g, p.b, p.a as f32, max);
                }
            }
            dst.clear();
            dst.extend_from_slice(&src);
            line_pass(&src, &levels, &mut dst, radius, is_max);
            for i in 0..inner {
                let idx = buf.at(o, i, vert);
                buf.px[idx] = dst[i];
            }
        }
    }
}

/// 元の BoxBlurH / BoxBlurV。縁は端の画素を伸ばす。
fn box_pass(buf: &mut Buf, vert: bool, r: i64) {
    let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
    let n = inner as i64;
    let window = (r * 2 + 1) as f64;
    let mut tmp: Vec<Px> = Vec::with_capacity(inner);
    for o in 0..outer {
        let get = |i: i64| -> Px { buf.px[buf.at(o, i.clamp(0, n - 1) as usize, vert)] };
        let mut s = Px::default();
        for i in -r..=r {
            let p = get(i);
            s.a += p.a;
            s.r += p.r;
            s.g += p.g;
            s.b += p.b;
        }
        tmp.clear();
        for i in 0..n {
            tmp.push(Px {
                a: buf.cast(s.a / window),
                r: buf.cast(s.r / window),
                g: buf.cast(s.g / window),
                b: buf.cast(s.b / window),
            });
            let l = get(i - r);
            let rr = get(i + r + 1);
            s.a += rr.a - l.a;
            s.r += rr.r - l.r;
            s.g += rr.g - l.g;
            s.b += rr.b - l.b;
        }
        for i in 0..inner {
            let idx = buf.at(o, i, vert);
            buf.px[idx] = tmp[i];
        }
    }
}

/// 元の TinyBlueM。横→縦を3回。
fn box_blur(buf: &mut Buf, r: i64) {
    if r <= 0 {
        return;
    }
    for _ in 0..3 {
        box_pass(buf, false, r);
        box_pass(buf, true, r);
    }
}

/// 元の HyperbolicAlpha。正の `k` では反転してから掛けて戻す。
fn hyperbolic(buf: &mut Buf, k: f64) {
    if k == 0.0 {
        return;
    }
    let invert = k >= 0.0;
    let k = k.abs() as f32;
    let max = buf.max;
    let float = buf.float;
    for p in buf.px.iter_mut() {
        if p.a <= 0.0 || p.a >= max {
            continue;
        }
        let mut t = (p.a / max) as f32;
        t = t.clamp(0.0, 1.0);
        if invert {
            t = 1.0 - t;
        }
        let mut v = ((1.0 + k) * t) / (1.0 + k * t);
        v = v.clamp(0.0, 1.0);
        if invert {
            v = 1.0 - v;
        }
        p.a = if float {
            v as f64
        } else {
            (v as f64 * max).trunc()
        };
    }
}

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
                minmax(&mut buf, minmax_v, minmax_v > 0);
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
    use super::*;

    fn buf(w: usize, h: usize) -> Buf {
        Buf {
            w,
            h,
            px: vec![Px::default(); w * h],
            max: 255.0,
            float: false,
        }
    }

    fn white() -> Px {
        Px {
            a: 255.0,
            r: 255.0,
            g: 255.0,
            b: 255.0,
        }
    }

    /// 乗算と乗算戻し。アルファ 0 は RGB も 0 に。
    #[test]
    fn the_premultiply_clears_transparent_pixels() {
        let mut b = buf(2, 1);
        b.px[0] = Px {
            a: 0.0,
            r: 200.0,
            g: 100.0,
            b: 50.0,
        };
        b.px[1] = white();
        mult(&mut b, false);
        assert_eq!(b.px[0].r, 0.0);
        assert_eq!(b.px[1], white(), "不透明はそのまま");
    }

    /// 半透明は乗算で暗くなり、戻すとほぼ元へ (切り捨てぶんだけ下がる)。
    #[test]
    fn the_premultiply_round_trip_is_close() {
        let mut b = buf(1, 1);
        b.px[0] = Px {
            a: 128.0,
            r: 200.0,
            g: 200.0,
            b: 200.0,
        };
        mult(&mut b, false);
        assert!(b.px[0].r < 200.0);
        mult(&mut b, true);
        assert!(b.px[0].r <= 200.0 && b.px[0].r >= 196.0, "{:?}", b.px[0]);
    }

    /// 膨張は明るいところを広げる。
    #[test]
    fn the_max_spreads_the_light() {
        let mut b = buf(9, 1);
        b.px[4] = white();
        minmax(&mut b, 2, true);
        for x in 2..=6 {
            assert_eq!(b.px[x].r, 255.0, "x = {x}");
        }
        assert_eq!(b.px[1], Px::default());
    }

    /// 収縮は暗いところを広げる。
    #[test]
    fn the_min_spreads_the_dark() {
        let mut b = buf(9, 1);
        for p in b.px.iter_mut() {
            *p = white();
        }
        b.px[4] = Px::default();
        minmax(&mut b, -2, false);
        for x in 2..=6 {
            assert_eq!(b.px[x], Px::default(), "x = {x}");
        }
    }

    /// 箱ぼかしは縁で端の画素を伸ばすので、一様な絵は変わらない。
    #[test]
    fn the_box_blur_keeps_a_flat_image() {
        let mut b = buf(16, 8);
        for p in b.px.iter_mut() {
            *p = white();
        }
        box_blur(&mut b, 3);
        assert!(b.px.iter().all(|p| *p == white()), "{:?}", b.px[0]);
    }

    /// 箱ぼかしは点を広げる。3回掛けるので台形ではなく丸みのある形。
    #[test]
    fn the_box_blur_spreads_a_dot() {
        let mut b = buf(64, 1);
        b.px[32] = white();
        box_blur(&mut b, 4);
        assert!(b.px[32].r < 255.0);
        assert!(b.px[36].r > 0.0);
        // 3回ぶんの広がりは半径の3倍ほど。
        assert_eq!(b.px[20].r, 0.0);
    }

    /// アルファのカーブ: 正の k は半端なアルファを薄くする
    /// (カーブは薄いほうを持ち上げる形だが、正では反転して掛けるため)。
    #[test]
    fn a_positive_k_thins_the_alpha() {
        let mut b = buf(1, 1);
        b.px[0] = Px {
            a: 128.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        hyperbolic(&mut b, 4.0);
        assert!(b.px[0].a < 128.0, "{:?}", b.px[0]);
    }

    /// 負の k は濃くする。
    #[test]
    fn a_negative_k_thickens_the_alpha() {
        let mut b = buf(1, 1);
        b.px[0] = Px {
            a: 128.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
        };
        hyperbolic(&mut b, -4.0);
        assert!(b.px[0].a > 128.0, "{:?}", b.px[0]);
    }

    /// 0 と最大のアルファは触らない。
    #[test]
    fn the_curve_leaves_the_ends_alone() {
        let mut b = buf(2, 1);
        b.px[1] = white();
        hyperbolic(&mut b, 9.0);
        assert_eq!(b.px[0].a, 0.0);
        assert_eq!(b.px[1].a, 255.0);
    }
}
