//! F's RandomShift の移植。
//!
//! 入力をそのままコピーしたうえで、長方形の切れ端を何度も別の場所に
//! 貼り直す。アナログのノイズというか、テープが傷んだときの横ずれの絵。
//!
//! 1回ぶんの手順 (元の Exec8/16/32 はどれも同じで、画素の型だけが違う):
//!
//! 1. 幅を `F_RAND2(横幅の最小, 横幅の最大)`、高さを
//!    `F_RAND2(縦幅の最小, 縦幅の最大)` で決める
//! 2. 元の位置を `F_RAND2(-幅, 画面の幅)` / `F_RAND2(-高さ, 画面の高さ)`
//!    で決める。**画面の外にはみ出した位置も出る**
//! 3. 横のずらし量を `F_RAND2(ずらしの最小, ずらしの最大)` で決め、
//!    もう1回引いて偶奇で左右の向きを決める
//! 4. 上下のずらし幅が 0 でなければ `F_RAND2(-上下, 上下)` で縦のずらし量を
//!    決める。**0 のときも乱数を1回捨てて並びを合わせる**
//! 5. 横も縦も 0 なら何もしない。そうでなければ長方形を貼る
//!
//! 元から写した細かいところ:
//!
//! - 種はフレーム番号 (`current_time / time_step`)。量が 0 なら種も蒔かない
//! - 最小値のほうが大きいときは入れ替える (元の `swapLong`)。
//!   入れ替えは3組それぞれ独立
//! - **読むほうは端で張り付く** (元の `getPixel8_F`)。画面の外から来た
//!   長方形は、いちばん端の1列・1行が伸びた絵になる
//! - **書くほうは画面の外を捨てる** (元の `setPixel8`)
//! - 貼るのは「コピーした出力」ではなく**入力から**なので、何度貼っても
//!   貼った結果をまた拾うことはない
//! - 深度によって乱数の引き方は変わらないので、8/16/32bit で同じ場所に
//!   同じ大きさの長方形が貼られる
//!
//! 元のパラメータ名のうち「横幅の最小値」のスライダーだけ、名前に
//! `STR_LENGTHMAX` (「横幅の最大値(px)」) を渡していて UI に同じ名前が
//! 2つ並ぶ。使われていない `STR_LEMGTHMIN` の定義が隣にあるので単なる
//! 書き間違い。ここでは正しい名前にしてある (描画には効かない)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/RandomShift

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;

/// 読み込んだパラメータ。元の ParamInfo のうち実際に使われるものだけ。
#[derive(Debug, Clone, Copy)]
struct Info {
    value: i64,
    shift_min: i64,
    shift_max: i64,
    length_min: i64,
    length_max: i64,
    height_min: i64,
    height_max: i64,
    vertual_shift: i64,
}

/// 貼る長方形1つぶん。元の FsPixelCopyParam。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    src_x: i64,
    src_y: i64,
    dst_x: i64,
    dst_y: i64,
    width: i64,
    height: i64,
}

/// 元の Exec8/16/32 の乱数のところ。深度に依らないので1つで済む。
///
/// 返すのは実際に貼る長方形だけ。横も縦もずれなかった回は飛ばす
/// (乱数はその回のぶんも引いてある)。
fn plan(info: &Info, w: i64, h: i64, frame: u32) -> Vec<Rect> {
    let mut out = Vec::new();
    if info.value <= 0 {
        return out;
    }
    // 元: 量が 0 より大きいときだけ F_SRAND(frame)。
    let mut rng = XorShift::new(frame);
    for _ in 0..info.value {
        let width = rng.rand2(info.length_min, info.length_max);
        let height = rng.rand2(info.height_min, info.height_max);
        // 元: 画面の外にはみ出した位置も出る。-幅 から 画面の幅 まで。
        let src_x = rng.rand2(-width, w);
        let src_y = rng.rand2(-height, h);
        let mut s = rng.rand2(info.shift_min, info.shift_max);
        // 元: もう1回引いて偶奇で向きを決める。
        if rng.next() % 2 == 1 {
            s = -s;
        }
        let sv = if info.vertual_shift != 0 {
            rng.rand2(-info.vertual_shift, info.vertual_shift)
        } else {
            // 元: 上下のずらしを使わないときも1回捨てる。ここを飛ばすと
            // 以降の並びが全部ずれる。
            rng.next();
            0
        };
        if s != 0 || sv != 0 {
            out.push(Rect {
                src_x,
                src_y,
                dst_x: src_x + s,
                dst_y: src_y + sv,
                width,
                height,
            });
        }
    }
    out
}

/// 元の CopyInToOut8/16/32。読むほうは端で張り付き、書くほうは外を捨てる。
fn blit<P: Copy>(src: &[P], dst: &mut [P], w: i64, h: i64, r: &Rect) {
    for j in 0..r.height {
        for i in 0..r.width {
            let dx = r.dst_x + i;
            let dy = r.dst_y + j;
            if dx < 0 || dy < 0 || dx >= w || dy >= h {
                continue;
            }
            // 元の getPixel8_F: 範囲外は端に寄せる。
            let sx = (r.src_x + i).clamp(0, w - 1);
            let sy = (r.src_y + j).clamp(0, h - 1);
            dst[(dx + dy * w) as usize] = src[(sx + sy * w) as usize];
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    ShiftMax,
    ShiftMin,
    LengthMax,
    LengthMin,
    HeightMax,
    HeightMin,
    VerShift,
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
        // 元: PF_ADD_SLIDER(名前, 入力の最小, 入力の最大, スライダーの最小,
        //     スライダーの最大, 既定値)。名前は元が非 ASCII なので英語。
        for (p, name, vmax, smax, dflt) in [
            (Params::Value, "Amount", 32000, 3000, 1000),
            (Params::ShiftMax, "Shift Max(px)", 2000, 100, 5),
            (Params::ShiftMin, "Shift Min(px)", 2000, 100, 1),
            (Params::LengthMax, "Width Max(px)", 2000, 100, 30),
            // 元はここに「横幅の最大値(px)」を渡していて名前が重なる。
            (Params::LengthMin, "Width Min(px)", 2000, 100, 5),
            (Params::HeightMax, "Height Max(px)", 1000, 50, 30),
            (Params::HeightMin, "Height Min(px)", 1000, 50, 5),
            (Params::VerShift, "Vertical Shift(px)", 1000, 50, 0),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(0);
                    f.set_valid_max(vmax);
                    f.set_slider_min(0);
                    f.set_slider_max(smax);
                    f.set_default(dflt);
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
                out_data.set_return_msg("fs-rs randomshift");
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
        let get = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(params.get(p)?.as_slider()?.value() as i64)
        };
        // 元の GetParams の順。最小のほうが大きければ入れ替える。
        let value = get(Params::Value, params)?;
        let (mut shift_max, mut shift_min) =
            (get(Params::ShiftMax, params)?, get(Params::ShiftMin, params)?);
        if shift_min > shift_max {
            std::mem::swap(&mut shift_min, &mut shift_max);
        }
        let (mut length_max, mut length_min) = (
            get(Params::LengthMax, params)?,
            get(Params::LengthMin, params)?,
        );
        if length_min > length_max {
            std::mem::swap(&mut length_min, &mut length_max);
        }
        let (mut height_max, mut height_min) = (
            get(Params::HeightMax, params)?,
            get(Params::HeightMin, params)?,
        );
        if height_min > height_max {
            std::mem::swap(&mut height_min, &mut height_max);
        }
        let vertual_shift = get(Params::VerShift, params)?;
        let info = Info {
            value,
            shift_min,
            shift_max,
            length_min,
            length_max,
            height_min,
            height_max,
            vertual_shift,
        };

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Render/Exec: まず入力をそのままコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if w == 0 || h == 0 {
            return Ok(());
        }

        // 元: 種はフレーム番号。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let rects = plan(&info, w as i64, h as i64, frame);
        if rects.is_empty() {
            return Ok(());
        }

        // 元は入力ワールドから読んで出力ワールドに書く。貼った結果を
        // また拾わないよう、こちらもコピー前の入力を控えておく。
        macro_rules! run {
            ($rd:ident, $wr:ident) => {{
                let mut src = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        src.push(*in_layer.$rd(x, y));
                    }
                }
                let mut dst = src.clone();
                for r in &rects {
                    blit(&src, &mut dst, w as i64, h as i64, r);
                }
                for y in 0..h {
                    for x in 0..w {
                        *out_layer.$wr(x, y) = dst[x + y * w];
                    }
                }
            }};
        }
        match depth {
            8 => run!(as_pixel8, as_pixel8_mut),
            16 => run!(as_pixel16, as_pixel16_mut),
            _ => run!(as_pixel32, as_pixel32_mut),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info {
            value: 1,
            shift_min: 5,
            shift_max: 5,
            length_min: 4,
            length_max: 4,
            height_min: 3,
            height_max: 3,
            vertual_shift: 0,
        }
    }

    /// 幅と高さを固定すれば、貼る長方形の大きさもそのまま。
    #[test]
    fn the_rect_takes_the_given_size() {
        let r = plan(&info(), 100, 100, 0);
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].width, r[0].height), (4, 3));
    }

    /// 横のずらし量は ±5 のどちらか。上下は使っていないので 0。
    #[test]
    fn the_shift_goes_either_way() {
        for frame in 0..40u32 {
            let r = plan(&info(), 100, 100, frame);
            let d = r[0].dst_x - r[0].src_x;
            assert!(d == 5 || d == -5, "frame={frame} d={d}");
            assert_eq!(r[0].dst_y, r[0].src_y);
        }
        // 両方の向きが出ること。
        let ds: Vec<i64> = (0..40u32)
            .map(|f| {
                let r = plan(&info(), 100, 100, f);
                r[0].dst_x - r[0].src_x
            })
            .collect();
        assert!(ds.contains(&5) && ds.contains(&-5));
    }

    /// ずらし量が 0 なら貼らない (量のぶん乱数は引く)。
    #[test]
    fn a_rect_that_does_not_move_is_skipped() {
        let mut i = info();
        i.shift_min = 0;
        i.shift_max = 0;
        i.value = 10;
        assert!(plan(&i, 100, 100, 1).is_empty());
    }

    /// 上下のずらしを使うかどうかで乱数の並びが変わらない
    /// (使わない回も1つ捨てているので、長方形の位置と大きさは同じ)。
    #[test]
    fn the_sequence_does_not_drift_without_the_vertical_shift() {
        let mut a = info();
        a.value = 20;
        let mut b = a;
        b.vertual_shift = 7;
        let (ra, rb) = (plan(&a, 100, 100, 3), plan(&b, 100, 100, 3));
        assert_eq!(ra.len(), rb.len());
        for (x, y) in ra.iter().zip(rb.iter()) {
            assert_eq!((x.src_x, x.src_y, x.dst_x), (y.src_x, y.src_y, y.dst_x));
            assert!((y.dst_y - y.src_y).abs() <= 7);
        }
    }

    /// 元の位置は画面の外にも出る。
    #[test]
    fn the_source_reaches_outside_the_frame() {
        let mut i = info();
        i.value = 400;
        let r = plan(&i, 32, 32, 1);
        assert!(r.iter().any(|r| r.src_x < 0 || r.src_y < 0));
        assert!(r.iter().any(|r| r.src_x + r.width > 32));
    }

    /// 読むほうは端で張り付く。画面の左外から来た長方形は左端の列が伸びる。
    #[test]
    fn the_read_clamps_at_the_edge() {
        let w = 4i64;
        let h = 1i64;
        let src = vec![10u8, 20, 30, 40];
        let mut dst = src.clone();
        blit(
            &src,
            &mut dst,
            w,
            h,
            &Rect {
                src_x: -2,
                src_y: 0,
                dst_x: 0,
                dst_y: 0,
                width: 3,
                height: 1,
            },
        );
        // -2,-1 はどちらも左端 (10) になる。
        assert_eq!(dst, vec![10, 10, 10, 40]);
    }

    /// 書くほうは画面の外を捨てる。
    #[test]
    fn the_write_clips_at_the_edge() {
        let w = 4i64;
        let h = 1i64;
        let src = vec![10u8, 20, 30, 40];
        let mut dst = src.clone();
        blit(
            &src,
            &mut dst,
            w,
            h,
            &Rect {
                src_x: 0,
                src_y: 0,
                dst_x: 2,
                dst_y: 0,
                width: 4,
                height: 1,
            },
        );
        assert_eq!(dst, vec![10, 20, 10, 20]);
    }

    /// 量が 0 なら何もしない。
    #[test]
    fn nothing_happens_without_the_amount() {
        let mut i = info();
        i.value = 0;
        assert!(plan(&i, 100, 100, 0).is_empty());
    }
}
