//! F's GuideFrame の移植。
//!
//! 入力の上に、2つの座標で決まる長方形の枠 (太さ 2px) を描くだけの
//! 絵コンテ用の補助。枠の色は指定できる。
//!
//! Check を入れると、枠を描く前に**不透明な画素の外接矩形の中にある
//! 透明な画素**を薄い青 (アルファ 64/255・青だけ最大) で埋める。
//! 絵の抜けを探すためのもの。
//!
//! 元から写した細かいところ:
//!
//! - 座標は固定小数を `>>16` して整数にする。右下だけ 1 引く
//!   (長方形の内側に収めるため)
//! - 枠は「1px の矩形を内側へ 2 回」描く (元の `box8` が `lineHeiht` を
//!   1 に落としてから太さぶん回すため)。重なるところは2回合成される
//! - 描くときは `PixelBlend8` で下地に乗せる。枠の色が不透明なら下は消える
//! - 幅や高さが太さの2倍より小さいときは太さを縮める (元の `box8`)
//! - 外接矩形を探すところは元のまま、縦・横それぞれ両端から1行/1列ずつ
//!   走って最初に不透明な画素が見つかったところで止める
//!
//! **Smooth のチェックボックスはどこからも読まれていない** (元のまま
//! 残してある。パラメータ構成を出荷 .aex と合わせる必要があるため)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/GuideFrame

use after_effects::{self as ae};
use libs::utils::{conv_8_to_16, conv_8_to_32};

/// 深度ごとの違いをまとめたもの。
trait Px: Copy {
    /// 元の PixelBlend*。`self` が下地、`over` が乗せる色。
    fn blend(self, over: Self) -> Self;
    fn is_clear(self) -> bool;
    /// Check で埋める色 (アルファ 64/255、青だけ最大)。
    fn check_colour() -> Self;
    /// 枠の色。元の CONV8TO16 / CONV8TO32。
    fn from8(c: ae::Pixel8) -> Self;
}

impl Px for ae::Pixel8 {
    fn blend(self, over: Self) -> Self {
        let (ss, dd) = (self, over);
        if ss.alpha == 0 || dd.alpha == 255 {
            return dd;
        }
        if dd.alpha == 0 {
            return ss;
        }
        let as2 = ((256 - dd.alpha as i32) * ss.alpha as i32) >> 8;
        let ad2 = (dd.alpha as i32 + as2).min(255);
        if ad2 <= 0 {
            return ae::Pixel8 {
                alpha: 0,
                red: 0,
                green: 0,
                blue: 0,
            };
        }
        let da = dd.alpha as i32;
        let mix = |d: u8, s: u8| (((d as i32 * da + s as i32 * as2) / ad2).min(255)) as u8;
        ae::Pixel8 {
            alpha: ad2 as u8,
            red: mix(dd.red, ss.red),
            green: mix(dd.green, ss.green),
            blue: mix(dd.blue, ss.blue),
        }
    }
    fn is_clear(self) -> bool {
        self.alpha == 0
    }
    fn check_colour() -> Self {
        ae::Pixel8 {
            alpha: 64,
            red: 0,
            green: 0,
            blue: 255,
        }
    }
    fn from8(c: ae::Pixel8) -> Self {
        c
    }
}

impl Px for ae::Pixel16 {
    fn blend(self, over: Self) -> Self {
        let (ss, dd) = (self, over);
        let max = ae::MAX_CHANNEL16 as i32;
        // 元はここを PF_MAX_CHAN8 (255) と書き間違えているが、
        // リポジトリの他の移植 (libs::fs_graphics8::pixel_blend16) と
        // 揃えて 16bit の最大値で見ている。
        if ss.alpha == 0 || dd.alpha as i32 == max {
            return dd;
        }
        if dd.alpha == 0 {
            return ss;
        }
        // 元は 8bit 版と違って `>>15` の前に +1 しない。
        let as2 = ((max - dd.alpha as i32) * ss.alpha as i32) >> 15;
        let ad2 = (dd.alpha as i32 + as2).min(max);
        if ad2 <= 0 {
            return ae::Pixel16 {
                alpha: 0,
                red: 0,
                green: 0,
                blue: 0,
            };
        }
        let da = dd.alpha as i32;
        let mix = |d: u16, s: u16| {
            ((d as i64 * da as i64 + s as i64 * as2 as i64) / ad2 as i64).min(max as i64) as u16
        };
        ae::Pixel16 {
            alpha: ad2 as u16,
            red: mix(dd.red, ss.red),
            green: mix(dd.green, ss.green),
            blue: mix(dd.blue, ss.blue),
        }
    }
    fn is_clear(self) -> bool {
        self.alpha == 0
    }
    fn check_colour() -> Self {
        // 元: {FS_CONVERT8TO16(64), 0, 0, FS_CONVERT8TO16(255)}
        let c8 = ae::Pixel8 {
            alpha: 64,
            red: 0,
            green: 0,
            blue: 255,
        };
        let c = conv_8_to_16(&c8);
        ae::Pixel16 {
            alpha: c.alpha,
            red: 0,
            green: 0,
            blue: c.blue,
        }
    }
    fn from8(c: ae::Pixel8) -> Self {
        conv_8_to_16(&c)
    }
}

impl Px for ae::PixelF32 {
    fn blend(self, over: Self) -> Self {
        let (ss, dd) = (self, over);
        if ss.alpha == 0.0 || dd.alpha >= 1.0 {
            return dd;
        }
        if dd.alpha == 0.0 {
            return ss;
        }
        let as2 = (1.0 - dd.alpha) * ss.alpha;
        let ad2 = (dd.alpha + as2).min(1.0);
        if ad2 <= 0.0 {
            return ae::PixelF32 {
                alpha: 0.0,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            };
        }
        let mix = |d: f32, s: f32| ((d * dd.alpha + s * as2) / ad2).min(1.0);
        ae::PixelF32 {
            alpha: ad2,
            red: mix(dd.red, ss.red),
            green: mix(dd.green, ss.green),
            blue: mix(dd.blue, ss.blue),
        }
    }
    fn is_clear(self) -> bool {
        self.alpha == 0.0
    }
    fn check_colour() -> Self {
        // 元: {64/255, 0, 0, 1}。ここだけ FS_CONVERT8TO32 を通さない。
        ae::PixelF32 {
            alpha: 64.0 / 255.0,
            red: 0.0,
            green: 0.0,
            blue: 1.0,
        }
    }
    fn from8(c: ae::Pixel8) -> Self {
        conv_8_to_32(&c)
    }
}

/// 元の XLine*。太さ 1 ぶん。
fn xline<P: Px>(buf: &mut [P], w: i64, h: i64, x0: i64, x1: i64, y: i64, col: P) {
    if y < 0 || y >= h {
        return;
    }
    let (mut a, mut b) = if x0 > x1 { (x1, x0) } else { (x0, x1) };
    if a >= w || b < 0 {
        return;
    }
    a = a.max(0);
    b = b.min(w - 1);
    for x in a..=b {
        let i = (x + y * w) as usize;
        buf[i] = buf[i].blend(col);
    }
}

/// 元の YLine*。太さ 1 ぶん。
fn yline<P: Px>(buf: &mut [P], w: i64, h: i64, x: i64, y0: i64, y1: i64, col: P) {
    if x < 0 || x >= w {
        return;
    }
    let (mut a, mut b) = if y0 > y1 { (y1, y0) } else { (y0, y1) };
    if a >= h || b < 0 {
        return;
    }
    a = a.max(0);
    b = b.min(h - 1);
    for y in a..=b {
        let i = (x + y * w) as usize;
        buf[i] = buf[i].blend(col);
    }
}

/// 元の box*。太さ `thick` ぶん、1px の矩形を内側へずらして重ねる。
fn draw_box<P: Px>(buf: &mut [P], w: i64, h: i64, x0: i64, y0: i64, x1: i64, y1: i64, thick: i64, col: P) {
    let (mut xx0, mut xx1) = if x0 > x1 { (x1, x0) } else { (x0, x1) };
    let (mut yy0, mut yy1) = if y0 > y1 { (y1, y0) } else { (y0, y1) };
    let mut hh = thick;
    if (xx1 - xx0) < hh * 2 {
        hh = (xx1 - xx0) / 2;
    }
    if (yy1 - yy0) < hh * 2 {
        hh = (yy1 - yy0) / 2;
    }
    hh -= 1;
    if hh < 0 {
        return;
    }
    for _ in 0..=hh {
        xline(buf, w, h, xx0, xx1, yy0, col);
        yline(buf, w, h, xx0, yy0 + 1, yy1 - 1, col);
        xline(buf, w, h, xx0, xx1, yy1, col);
        yline(buf, w, h, xx1, yy0 + 1, yy1 - 1, col);
        xx0 += 1;
        yy0 += 1;
        xx1 -= 1;
        yy1 -= 1;
    }
}

/// 元の chkW*。不透明な画素の外接矩形の中の透明な画素を埋める。
fn check_holes<P: Px>(buf: &mut [P], w: i64, h: i64) {
    let opaque_in_row = |buf: &[P], y: i64| (0..w).any(|x| !buf[(x + y * w) as usize].is_clear());
    let opaque_in_col = |buf: &[P], x: i64| (0..h).any(|y| !buf[(x + y * w) as usize].is_clear());

    let Some(y0) = (0..h).find(|&y| opaque_in_row(buf, y)) else {
        return;
    };
    let Some(y1) = (0..h).rev().find(|&y| opaque_in_row(buf, y)) else {
        return;
    };
    let Some(x0) = (0..w).find(|&x| opaque_in_col(buf, x)) else {
        return;
    };
    let Some(x1) = (0..w).rev().find(|&x| opaque_in_col(buf, x)) else {
        return;
    };
    let col = P::check_colour();
    for y in y0..=y1 {
        for x in x0..=x1 {
            let i = (x + y * w) as usize;
            if buf[i].is_clear() {
                buf[i] = col;
            }
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Color,
    TopLeft,
    BottomRight,
    Check,
    Smooth,
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
        // 元: PF_ADD_COLOR("color", 0xff, 0x00, 0x00)。
        params.add(
            Params::Color,
            "color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0x00,
                    blue: 0x00,
                });
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT("TopLeft", 25, 25) / ("BottomRight", 75, 75)。
        // 既定値はレイヤーの大きさに対する % 指定。
        for (p, name, xy) in [
            (Params::TopLeft, "TopLeft", (25.0, 25.0)),
            (Params::BottomRight, "BottomRight", (75.0, 75.0)),
        ] {
            params.add(
                p,
                name,
                ae::PointDef::setup(|f| {
                    f.set_default(xy);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_CHECKBOX("Check", "on", FALSE)。
        params.add(
            Params::Check,
            "Check",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("Smooth", "on", TRUE)。元でもどこからも
        // 読まれていないが、構成を合わせるために残してある。
        params.add(
            Params::Smooth,
            "Smooth",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
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
                out_data.set_return_msg("fs-rs guideframe");
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
        let colour = params.get(Params::Color)?.as_color()?.value();
        let pt = |p: Params, params: &mut ae::Parameters<Params>| -> Result<(i64, i64), Error> {
            let (x, y) = params.get(p)?.as_point()?.value();
            // 元は PF_Fixed を >>16 して整数にする。
            Ok((((x * 65536.0) as i64) >> 16, ((y * 65536.0) as i64) >> 16))
        };
        let (x0, y0) = pt(Params::TopLeft, params)?;
        let (bx, by) = pt(Params::BottomRight, params)?;
        // 元: 右下だけ 1 引く。
        let (x1, y1) = (bx - 1, by - 1);
        let check = params.get(Params::Check)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をそのままコピー。
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

        macro_rules! run {
            ($rd:ident, $wr:ident) => {{
                let mut buf = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        buf.push(*out_layer.$rd(x, y));
                    }
                }
                if check {
                    check_holes(&mut buf, w as i64, h as i64);
                }
                // 元: ae->out->lineHeiht = 2。
                let col = Px::from8(colour);
                draw_box(&mut buf, w as i64, h as i64, x0, y0, x1, y1, 2, col);
                for y in 0..h {
                    for x in 0..w {
                        *out_layer.$wr(x, y) = buf[x + y * w];
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

    fn px(a: u8, r: u8, g: u8, b: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: a,
            red: r,
            green: g,
            blue: b,
        }
    }

    /// PF_Pixel は PartialEq を持たないので自前で見る。
    fn eq(a: ae::Pixel8, b: ae::Pixel8) -> bool {
        a.alpha == b.alpha && a.red == b.red && a.green == b.green && a.blue == b.blue
    }

    const CLEAR: ae::Pixel8 = ae::Pixel8 {
        alpha: 0,
        red: 0,
        green: 0,
        blue: 0,
    };

    fn red() -> ae::Pixel8 {
        px(255, 255, 0, 0)
    }

    /// 枠は太さ 2px で、中は空のまま。
    #[test]
    fn the_frame_is_two_pixels_thick() {
        let (w, h) = (12i64, 12i64);
        let mut buf = vec![CLEAR; (w * h) as usize];
        draw_box(&mut buf, w, h, 2, 2, 9, 9, 2, red());
        let at = |x: i64, y: i64| buf[(x + y * w) as usize];
        assert!(eq(at(2, 2), red()), "外側の角");
        assert!(eq(at(3, 3), red()), "内側の角");
        assert!(eq(at(4, 4), CLEAR), "中は空");
        assert!(eq(at(2, 1), CLEAR), "枠の外");
        assert!(eq(at(9, 9), red()));
        assert!(eq(at(8, 8), red()));
        assert!(eq(at(7, 7), CLEAR));
    }

    /// 細い長方形では太さが縮む。
    #[test]
    fn a_thin_box_loses_its_thickness() {
        let (w, h) = (12i64, 12i64);
        let mut buf = vec![CLEAR; (w * h) as usize];
        // 高さ 2 (y 5..6) なら (y1-y0)/2 = 0 で、太さは 0 になり何も出ない。
        draw_box(&mut buf, w, h, 1, 5, 10, 6, 2, red());
        assert!(buf.iter().all(|p| eq(*p, CLEAR)));
        // 高さ 4 なら 1px ぶん出る。
        draw_box(&mut buf, w, h, 1, 4, 10, 7, 2, red());
        assert!(eq(buf[(1 + 4 * w) as usize], red()));
        assert!(eq(buf[(2 + 5 * w) as usize], CLEAR));
    }

    /// 画面の外へ出る枠は縁で切られる (落ちない)。
    #[test]
    fn the_frame_is_clipped_at_the_edges() {
        let (w, h) = (8i64, 8i64);
        let mut buf = vec![CLEAR; (w * h) as usize];
        draw_box(&mut buf, w, h, -20, -20, 30, 30, 2, red());
        // 全部外なので何も描かれない (元の XLine/YLine と同じ)。
        assert!(buf.iter().all(|p| eq(*p, CLEAR)));
        draw_box(&mut buf, w, h, -3, -3, 4, 4, 2, red());
        assert!(eq(buf[(4 + 0 * w) as usize], red()), "右の縦線は見える");
    }

    /// 枠は下地に合成される。半透明なら下の色が残る。
    #[test]
    fn the_frame_blends_over_what_is_there() {
        let (w, h) = (6i64, 6i64);
        let white = px(255, 255, 255, 255);
        let mut buf = vec![white; (w * h) as usize];
        draw_box(&mut buf, w, h, 0, 0, 5, 5, 1, px(128, 0, 0, 0));
        let c = buf[0];
        assert_eq!(c.alpha, 255);
        assert!(c.red > 0 && c.red < 255, "red={}", c.red);
    }

    /// Check は外接矩形の中の穴だけを埋める。
    #[test]
    fn the_check_fills_only_the_holes_inside() {
        let (w, h) = (6i64, 6i64);
        let mut buf = vec![CLEAR; (w * h) as usize];
        // (1,1) と (4,4) に不透明な点 → 外接矩形は 1..4。
        buf[(1 + 1 * w) as usize] = red();
        buf[(4 + 4 * w) as usize] = red();
        check_holes(&mut buf, w, h);
        assert!(eq(buf[(2 + 2 * w) as usize], ae::Pixel8::check_colour()));
        assert!(eq(buf[(0 + 0 * w) as usize], CLEAR), "矩形の外は空のまま");
        assert!(eq(buf[(5 + 5 * w) as usize], CLEAR));
        assert!(eq(buf[(1 + 1 * w) as usize], red()), "元の画素はそのまま");
    }

    /// 全部透明なら Check は何もしない。
    #[test]
    fn the_check_does_nothing_on_an_empty_layer() {
        let (w, h) = (4i64, 4i64);
        let mut buf = vec![CLEAR; (w * h) as usize];
        check_holes(&mut buf, w, h);
        assert!(buf.iter().all(|p| eq(*p, CLEAR)));
    }

    /// 埋める色は深度を上げても同じ見え方 (アルファ 64/255・青だけ最大)。
    #[test]
    fn the_check_colour_rises_with_the_depth() {
        let c16 = <ae::Pixel16 as Px>::check_colour();
        assert_eq!(c16.blue, ae::MAX_CHANNEL16 as u16);
        assert_eq!(c16.red, 0);
        let c32 = <ae::PixelF32 as Px>::check_colour();
        assert_eq!(c32.blue, 1.0);
        assert!((c32.alpha - 64.0 / 255.0).abs() < 1e-6);
    }
}
