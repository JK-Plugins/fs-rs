//! F's EdgeBlur の移植。
//!
//! **アルファだけ**を内側へ縮めてからぼかす。絵の縁を柔らかく内側へ
//! 引っ込めるので、合成の境目をなじませるのに使う。RGB は触らない。
//!
//! 手順 (元の Exec):
//!
//! 1. 入力をそのままコピーする。ぼかし量が 0 以下なら終わり
//! 2. `offset` (= `blur + offset` を 0 で押さえたもの) が 0 より大きければ、
//!    アルファに半径 `offset` の最小値フィルタを横→縦に掛ける
//!    (縁が内側へ縮む)
//! 3. アルファに半径 `blur` のガウスぼかしを横→縦に掛ける
//!
//! 元から写した細かいところ:
//!
//! - `offset` はスライダーの値に `blur` を足したもの。負なら 0
//! - 両方 downsample を掛ける (整数割り算、丸めなし)
//! - 最小値フィルタの窓が画面の外に出る画素は **0 (透明) にする**。
//!   だから縁から `offset` 画素ぶんは必ず透明になる
//! - 最小値フィルタは「前の窓の最小値の位置」を覚えておいて、窓の中に
//!   残っていれば右端1つと比べるだけで済ます早回し
//! - ぼかしの重みの合計は、窓が画面の外に出ていても**数に入れる**
//!   (`count += gauss` が範囲の確かめの外)。だから縁ほど薄くなる
//! - 重みは `exp(-i²/(2*(blur/3)²)) * 256` の整数表
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **縦のぼかしが下の隣を「画面の幅」で止めている** (`if (ir <
//!    sub->width)`、高さのつもり)。**縦長の絵では幅より下の行が下側の
//!    隣を落とす** (144×256 の絵で 8318 画素が変わる)。横長の絵では
//!    範囲の外を読むが、走査線バッファに残っているのは直前の横ぼかしで
//!    最後の行から写した値で、絵の下端は透明なので足しても 0 になり、
//!    結果は変わらない。`height` にした
//!
//! 直っているように見えて**結果は変わらないところ**:
//!
//! - 最小値フィルタの早回しは、窓を総当たりで見る枝で `minValue` だけ
//!   更新して `minIndex` を置き忘れている。一見「窓の最小値ではない値を
//!   使ってしまう」ように見えるが、総当たりの枝に入るのは `minIndex` が
//!   窓より**左**にあるときだけで (位置は必ず窓の右端以下なので右に
//!   外れることはない)、置き忘れたまま窓が1つ右へ動いても `minIndex` は
//!   やはり窓の左の外にある。つまり一度総当たりに落ちたら、0 が見つかって
//!   位置が入るまで毎回総当たりになり、答えは常に正しい。ここでは位置を
//!   控えるようにしてあるが (早いだけ)、出る絵は元と同じ
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/EdgeBlur

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// アルファの並びと、元が使い回す走査線バッファ。
///
/// 元は `scanlineWidth * 3 / 2` 個ぶんのバッファを1枚取って、横・縦の
/// 4つの処理で使い回す。直したところ 1 を戻したときに、縦のぼかしが
/// 範囲の外を読んでそこに残っている値を拾うので、同じ使い回しを真似る。
struct Work {
    w: usize,
    h: usize,
    /// アルファだけ。深度に依らず f64。
    a: Vec<f64>,
    scan: Vec<f64>,
    max: f64,
}

impl Work {
    fn new(w: usize, h: usize, max: f64) -> Self {
        let sw = w.max(h);
        Self {
            w,
            h,
            a: vec![0.0; w * h],
            scan: vec![0.0; sw * 3 / 2],
            max,
        }
    }
    fn at(&self, o: usize, i: usize, vert: bool) -> usize {
        if vert {
            i * self.w + o
        } else {
            o * self.w + i
        }
    }
}

/// 元の horMin / verMin。アルファの最小値フィルタ。
fn min_pass(wk: &mut Work, vert: bool, offset: i64) {
    let (outer, inner) = if vert { (wk.w, wk.h) } else { (wk.h, wk.w) };
    for o in 0..outer {
        for i in 0..inner {
            let idx = wk.at(o, i, vert);
            wk.scan[i] = wk.a[idx];
        }
        // 元: 列 (行) ごとに持ち越さず、ここで初期化する。
        let mut min_index: i64 = -1;
        let mut min_value = wk.max;
        for i in 0..inner {
            let start = i as i64 - offset;
            let end = i as i64 + offset;
            if start < 0 || end >= inner as i64 {
                // 元: 窓が画面の外に出たら透明にする。
                min_value = 0.0;
                min_index = -1;
            } else if min_index >= start && min_index <= end {
                let e = end as usize;
                if wk.scan[e] <= wk.scan[min_index as usize] {
                    min_value = wk.scan[e];
                    min_index = end;
                } else {
                    min_value = wk.scan[min_index as usize];
                }
            } else {
                min_value = wk.max;
                for k in start..=end {
                    let v = wk.scan[k as usize];
                    if v == 0.0 {
                        min_value = 0.0;
                        min_index = k;
                        break;
                    }
                    if min_value > v {
                        min_value = v;
                        // 元はここで位置を控えていない (直したところ 2)。
                        if !FAITHFUL {
                            min_index = k;
                        }
                    }
                }
            }
            let idx = wk.at(o, i, vert);
            wk.a[idx] = min_value;
        }
    }
}

/// 元の makeTbl。
fn make_table(blur: i64) -> Option<Vec<i64>> {
    if blur <= 0 {
        return None;
    }
    let zone = blur as f64 / 3.0;
    Some(
        (0..=blur)
            .map(|i| ((-(i as f64) * i as f64 / (2.0 * zone * zone)).exp() * 256.0) as i64)
            .collect(),
    )
}

/// 元の horBlur / verBlur。
fn blur_pass(wk: &mut Work, vert: bool, blur: i64, tbl: &[i64], float: bool) {
    let (outer, inner) = if vert { (wk.w, wk.h) } else { (wk.h, wk.w) };
    // 元: 縦のぼかしだけ下の隣を幅で止めている (直したところ 1)。
    let far = if FAITHFUL && vert { wk.w } else { inner };
    for o in 0..outer {
        for i in 0..inner {
            let idx = wk.at(o, i, vert);
            wk.scan[i] = wk.a[idx];
        }
        for i in 0..inner {
            let mut count: i64 = 0;
            let mut sa = 0.0f64;
            let g0 = tbl[0];
            sa += wk.scan[i] * g0 as f64;
            count += g0;
            for k in 1..=blur as usize {
                let g = tbl[k];
                if i >= k {
                    sa += wk.scan[i - k] * g as f64;
                }
                // 元: 範囲の外でも重みは数に入れる。
                count += g;
                if i + k < far {
                    sa += wk.scan[i + k] * g as f64;
                }
                count += g;
            }
            if count > 0 {
                let v = sa / count as f64;
                let v = if float {
                    v
                } else {
                    v.trunc().clamp(0.0, wk.max)
                };
                let idx = wk.at(o, i, vert);
                wk.a[idx] = v;
            }
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Blur,
    Offset,
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
        // 元: PF_ADD_SLIDER("blur", 0, 100, 0, 50, 0)
        params.add(
            Params::Blur,
            "blur",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(50);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("offset", -50, 50, -36, 36, 0)
        params.add(
            Params::Offset,
            "offset",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-50);
                f.set_valid_max(50);
                f.set_slider_min(-36);
                f.set_slider_max(36);
                f.set_default(0);
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
                out_data.set_return_msg("fs-rs edgeblur");
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
        let down = |v: i64| -> i64 { v * ds.num as i64 / ds.den as i64 };
        // 元の GetParams: offset はスライダー + blur、負なら 0。
        let blur_raw = params.get(Params::Blur)?.as_slider()?.value() as i64;
        let off_raw = params.get(Params::Offset)?.as_slider()?.value() as i64;
        let blur = down(blur_raw);
        let offset = down((blur_raw + off_raw).max(0));

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
        if blur <= 0 || w == 0 || h == 0 {
            return Ok(());
        }
        let Some(tbl) = make_table(blur) else {
            return Ok(());
        };

        macro_rules! go {
            ($wr:ident, $max:expr, $float:expr, $get:expr, $put:expr) => {{
                let mut wk = Work::new(w, h, $max);
                for y in 0..h {
                    for x in 0..w {
                        wk.a[x + y * w] = $get(out_layer.$wr(x, y));
                    }
                }
                if offset > 0 {
                    min_pass(&mut wk, false, offset);
                    min_pass(&mut wk, true, offset);
                }
                blur_pass(&mut wk, false, blur, &tbl, $float);
                blur_pass(&mut wk, true, blur, &tbl, $float);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), wk.a[x + y * w]);
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                255.0,
                false,
                |p: &mut ae::Pixel8| p.alpha as f64,
                |p: &mut ae::Pixel8, v: f64| p.alpha = v as i64 as u8
            ),
            16 => go!(
                as_pixel16_mut,
                32768.0,
                false,
                |p: &mut ae::Pixel16| p.alpha as f64,
                |p: &mut ae::Pixel16, v: f64| p.alpha = v as i64 as u16
            ),
            _ => go!(
                as_pixel32_mut,
                1.0,
                true,
                |p: &mut ae::PixelF32| p.alpha as f64,
                |p: &mut ae::PixelF32, v: f64| p.alpha = v as f32
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work(w: usize, h: usize, fill: f64) -> Work {
        let mut wk = Work::new(w, h, 255.0);
        for v in wk.a.iter_mut() {
            *v = fill;
        }
        wk
    }

    /// 最小値フィルタは縁から offset 画素ぶんを透明にする。
    #[test]
    fn the_min_pass_eats_the_border() {
        let mut wk = work(16, 16, 255.0);
        min_pass(&mut wk, false, 3);
        min_pass(&mut wk, true, 3);
        for y in 0..16 {
            for x in 0..16 {
                let inside = (3..13).contains(&x) && (3..13).contains(&y);
                let v = wk.a[x + y * 16];
                if inside {
                    assert_eq!(v, 255.0, "({x},{y}) が削られている");
                } else {
                    assert_eq!(v, 0.0, "({x},{y}) が残っている");
                }
            }
        }
    }

    /// 穴のまわりも削られる。
    #[test]
    fn the_min_pass_widens_a_hole() {
        let mut wk = work(21, 1, 255.0);
        wk.a[10] = 0.0;
        min_pass(&mut wk, false, 2);
        for x in 8..=12 {
            assert_eq!(wk.a[x], 0.0, "x = {x}");
        }
        assert_eq!(wk.a[7], 255.0);
    }

    /// 早回しが効いていても、窓の最小値が正しく出る (直したところ 2)。
    #[test]
    fn the_running_min_is_right_everywhere() {
        // 階段状に下げて、早回しの枝と総当たりの枝を両方通す。
        let n = 40;
        let mut wk = work(n, 1, 255.0);
        for x in 0..n {
            wk.a[x] = (200 - (x as i64 % 7) * 25).max(10) as f64;
        }
        let src = wk.a.clone();
        let off = 3i64;
        min_pass(&mut wk, false, off);
        for x in 0..n {
            let (s, e) = (x as i64 - off, x as i64 + off);
            let want = if s < 0 || e >= n as i64 {
                0.0
            } else {
                (s..=e).map(|k| src[k as usize]).fold(f64::MAX, f64::min)
            };
            assert_eq!(wk.a[x], want, "x = {x}");
        }
    }

    /// ぼかしの重みの表。
    #[test]
    fn the_table_falls_off() {
        let t = make_table(6).unwrap();
        assert_eq!(t[0], 256);
        for i in 1..7 {
            assert!(t[i] < t[i - 1]);
        }
        assert!(make_table(0).is_none());
    }

    /// ぼかしは縁を薄くする (範囲の外も重みに数えるため)。
    #[test]
    fn the_blur_fades_the_border() {
        let mut wk = work(32, 32, 255.0);
        let t = make_table(5).unwrap();
        blur_pass(&mut wk, false, 5, &t, false);
        blur_pass(&mut wk, true, 5, &t, false);
        assert!(wk.a[16 + 16 * 32] > 250.0, "真ん中は濃いまま");
        assert!(wk.a[0] < 100.0, "角は薄くなる: {}", wk.a[0]);
    }

    /// 縦のぼかしは下の隣もちゃんと見る (直したところ 1)。
    /// 横長の絵で、下端の列が上側だけから作られていないことを見る。
    #[test]
    fn the_vertical_blur_looks_down() {
        // 幅 32・高さ 8。上半分だけ不透明。
        let mut wk = work(32, 8, 0.0);
        for y in 0..4 {
            for x in 0..32 {
                wk.a[x + y * 32] = 255.0;
            }
        }
        let t = make_table(3).unwrap();
        blur_pass(&mut wk, true, 3, &t, false);
        // 下側 (y = 4,5) にもにじむ。
        assert!(wk.a[16 + 4 * 32] > 0.0);
        assert!(wk.a[16 + 5 * 32] > 0.0);
        // y = 3 は上からも下からも引くので、真ん中より薄い。
        assert!(wk.a[16 + 3 * 32] < wk.a[16 + 1 * 32]);
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }
}
