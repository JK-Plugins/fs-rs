//! F's Frame の移植。
//!
//! 画面の真ん中に指定した大きさの枠を描き、枠の外を半透明の色で覆う。
//! セーフフレームと中心の十字も引ける、構図合わせ用のガイド。
//! `FsSSFrame` は **#include 行以外まったく同じソース**の双子。
//!
//! 手順 (元の Exec):
//!
//! 1. 出力を透明で消し、`out_color` (アルファは `out_opacity`) で
//!    全面を塗り、枠の内側をもう一度透明で消す
//! 2. 枠の線を `line_color`、太さ `line_height` で引く
//! 3. `center_line` が入っていれば、画面いっぱいの横線と縦線を引く
//!    (位置は中心から太さの半分ぶん戻したところ)
//! 4. `safe_frame` が 0 より大きく 100 未満なら、その割合の大きさの
//!    四角も引く
//!
//! 枠の大きさは `size` のポップアップで選ぶ。「上の数値」なら `width` /
//! `height` のスライダーをそのまま使う。
//!
//! 元から写した細かいところ:
//!
//! - 枠の大きさ・線の太さには downsample を掛けて四捨五入する
//! - 中心は `幅 / 2`、`高さ / 2` の整数割り算。枠は
//!   `中心 ± 大きさ/2` で、右下は -1 する
//! - セーフフレームの大きさは `枠 * % / 100` の整数割り算
//! - 線は `PixelBlend8` で重ねる (重ねる色のアルファが最大なら上書き)
//! - 四角の角の処理も元のまま: 太さが辺の長さの半分を超えると
//!   細くなるよう押さえ、1画素の線を内側へずらしながら太さぶん重ねる
//! - `out_color` のアルファは `255 * 不透明度` を切り捨てたもの
//!
//! 元と違うところ:
//!
//! - 元は枠の内側を消すのに AE の `PF_FILL` を矩形つきで呼ぶ。枠が
//!   画面より大きいと矩形が画面の外に出て、**AE のコールバックが
//!   エラーを返す** (AEXCompat でも 516 で落ちる)。そのときは内側が
//!   消えないまま線だけが描かれる。移植では自前で塗って**画面の内側で
//!   切る**ので、枠が画面より大きくても線だけが見える絵になる
//!   (既定の 1280×720 はたいていの確認用の画より大きいので、ここが
//!   いちばん目につく違い)
//! - `PF_FILL` は 8bit 専用のコールバックなので、元は 16/32bit の
//!   出力を 8bit として塗ってしまう。移植では深度ごとに塗る
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Frame

use after_effects::{self as ae};

/// 1画素。深度に依らず f64 で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 画素の並び。
pub struct Buf {
    pub w: i64,
    pub h: i64,
    pub px: Vec<Px>,
    pub max: f64,
    pub float: bool,
    /// 元の CFsGraph::lineHeiht。
    pub line_height: i64,
}

impl Buf {
    /// 元の PixelBlend8。重ねる色のアルファが最大なら下地を見ずに上書き。
    fn blend(&self, s: Px, d: Px) -> Px {
        let max = self.max;
        if s.a == 0.0 || d.a == max {
            return d;
        }
        if d.a == 0.0 {
            return s;
        }
        // 元: ((MAX+1) - dd.alpha) * ss.alpha >> 8 (8bit)。
        let as2 = if self.float {
            (1.0 - d.a) * s.a
        } else if max == 255.0 {
            (((max + 1.0) - d.a) * s.a / 256.0).floor()
        } else {
            ((max - d.a) * s.a / 32768.0).floor()
        };
        let mut ad2 = d.a + as2;
        if ad2 > max {
            ad2 = max;
        }
        if ad2 <= 0.0 {
            return Px::default();
        }
        let f = |dc: f64, sc: f64| {
            let v = (dc * d.a + sc * as2) / ad2;
            let v = if self.float { v } else { v.trunc() };
            v.min(max)
        };
        Px {
            a: if self.float { ad2 } else { ad2.trunc() },
            r: f(d.r, s.r),
            g: f(d.g, s.g),
            b: f(d.b, s.b),
        }
    }

    fn set(&mut self, x: i64, y: i64, col: Px) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let i = (x + y * self.w) as usize;
        let s = self.px[i];
        self.px[i] = self.blend(s, col);
    }

    /// 元の PF_FILL。矩形を指定の色で塗りつぶす (画面の内側で切る)。
    fn fill(&mut self, rect: Option<(i64, i64, i64, i64)>, col: Px) {
        let (x0, y0, x1, y1) = match rect {
            Some(r) => (r.0.max(0), r.1.max(0), r.2.min(self.w - 1), r.3.min(self.h - 1)),
            None => (0, 0, self.w - 1, self.h - 1),
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                self.px[(x + y * self.w) as usize] = col;
            }
        }
    }

    /// 元の XLine8。太さは `line_height` で、下に伸びる。
    fn xline(&mut self, x0: i64, x1: i64, y: i64, col: Px) {
        let hh = self.line_height - 1;
        if hh < 0 {
            return;
        }
        if (y + hh) < 0 || (y - hh) >= self.h {
            return;
        }
        let (mut a, mut b) = (x0.min(x1), x0.max(x1));
        if a >= self.w || b < 0 {
            return;
        }
        a = a.max(0);
        b = b.min(self.w - 1);
        for j in 0..=hh {
            let yy = y + j;
            if yy >= 0 && yy < self.h {
                for x in a..=b {
                    self.set(x, yy, col);
                }
            }
        }
    }

    /// 元の YLine8。太さは右に伸びる。
    fn yline(&mut self, x: i64, y0: i64, y1: i64, col: Px) {
        let hh = self.line_height - 1;
        if hh < 0 {
            return;
        }
        if (x + hh) < 0 || (x - hh) >= self.w {
            return;
        }
        let (mut a, mut b) = (y0.min(y1), y0.max(y1));
        if a >= self.h || b < 0 {
            return;
        }
        a = a.max(0);
        b = b.min(self.h - 1);
        for j in 0..=hh {
            let xx = x + j;
            if xx >= 0 && xx < self.w {
                for y in a..=b {
                    self.set(xx, y, col);
                }
            }
        }
    }

    /// 元の box8。1画素の線を内側へずらしながら太さぶん重ねる。
    fn box_outline(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, col: Px) {
        let bak = self.line_height;
        self.line_height = 1;
        let mut hh = bak;
        let (mut xx0, mut xx1) = (x0.min(x1), x0.max(x1));
        let (mut yy0, mut yy1) = (y0.min(y1), y0.max(y1));
        if (xx1 - xx0) < hh * 2 {
            hh = (xx1 - xx0) / 2;
        }
        if (yy1 - yy0) < hh * 2 {
            hh = (yy1 - yy0) / 2;
        }
        hh -= 1;
        if hh >= 0 {
            for _ in 0..=hh {
                self.xline(xx0, xx1, yy0, col);
                self.yline(xx0, yy0 + 1, yy1 - 1, col);
                self.xline(xx0, xx1, yy1, col);
                self.yline(xx1, yy0 + 1, yy1 - 1, col);
                xx0 += 1;
                yy0 += 1;
                xx1 -= 1;
                yy1 -= 1;
            }
        }
        self.line_height = bak;
    }
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
pub struct Info {
    pub frame_width: i64,
    pub frame_height: i64,
    pub safe_frame: i64,
    pub center_line: bool,
    pub line_height: i64,
}

/// 元の Exec の描くところ。
pub fn draw(buf: &mut Buf, info: &Info, line_col: Px, out_col: Px) {
    let cx = buf.w / 2;
    let cy = buf.h / 2;
    let frm = (
        cx - info.frame_width / 2,
        cy - info.frame_height / 2,
        cx + info.frame_width / 2 - 1,
        cy + info.frame_height / 2 - 1,
    );
    let sw = info.frame_width * info.safe_frame / 100;
    let sh = info.frame_height * info.safe_frame / 100;
    let safe = (cx - sw / 2, cy - sh / 2, cx + sw / 2 - 1, cy + sh / 2 - 1);

    buf.fill(None, Px::default());
    buf.fill(None, out_col);
    buf.fill(Some((frm.0, frm.1, frm.2, frm.3)), Px::default());

    buf.line_height = info.line_height;
    buf.box_outline(frm.0, frm.1, frm.2, frm.3, line_col);
    if info.center_line {
        let p = buf.line_height / 2;
        buf.xline(0, buf.w, cy - p, line_col);
        buf.yline(cx - p, 0, buf.h, line_col);
    }
    if info.safe_frame > 0 && info.safe_frame < 100 {
        buf.box_outline(safe.0, safe.1, safe.2, safe.3, line_col);
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Width,
    Height,
    LineColor,
    SafeFrame,
    CenterLine,
    OutColor,
    OutOpacity,
    LineHeight,
    SizePop,
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
        // 元: PF_ADD_SLIDER("width", 50, 4800, 640, 1920, 1280)
        params.add(
            Params::Width,
            "width",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(50);
                f.set_valid_max(4800);
                f.set_slider_min(640);
                f.set_slider_max(1920);
                f.set_default(1280);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("height", 50, 4800, 480, 1080, 720)
        params.add(
            Params::Height,
            "height",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(50);
                f.set_valid_max(4800);
                f.set_slider_min(480);
                f.set_slider_max(1080);
                f.set_default(720);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("line_color", 0xFF, 0x00, 0x00)
        params.add(
            Params::LineColor,
            "line_color",
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
        // 元: PF_ADD_SLIDER("safe_frame", 0, 100, 50, 100, 80)
        params.add(
            Params::SafeFrame,
            "safe_frame",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(50);
                f.set_slider_max(100);
                f.set_default(80);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("center_line", "ON", TRUE)
        params.add(
            Params::CenterLine,
            "center_line",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("out_color", 0xFF, 0x00, 0xFF)
        params.add(
            Params::OutColor,
            "out_color",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("out_opacity", 0, 100, 0, 100, 50, 1, 0, 0)
        params.add(
            Params::OutOpacity,
            "out_opacity",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("line_height", 1, 10, 1, 5, 2)
        params.add(
            Params::LineHeight,
            "line_height",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(10);
                f.set_slider_min(1);
                f.set_slider_max(5);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP("size", 7, 1, "上の数値|720x540|...")。
        // 1つめの項目は元が非 ASCII なので英語にしてある。
        params.add(
            Params::SizePop,
            "size",
            ae::PopupDef::setup(|f| {
                f.set_options(&[
                    "Use sliders",
                    "720x540",
                    "1024x576",
                    "1024x768",
                    "1280x720",
                    "1440x810",
                    "1920x1080",
                ]);
                f.set_default(1);
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
                out_data.set_return_msg("fs-rs frame");
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
        _in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let ds = in_data.downsample_x();
        let down = |v: i64| -> i64 {
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        // 元の GetParams。
        let kind = params.get(Params::SizePop)?.as_popup()?.value() as i64;
        let (fw, fh) = match kind {
            2 => (720, 540),
            3 => (1024, 576),
            4 => (1024, 768),
            5 => (1280, 720),
            6 => (1440, 810),
            7 => (1920, 1080),
            _ => (
                params.get(Params::Width)?.as_slider()?.value() as i64,
                params.get(Params::Height)?.as_slider()?.value() as i64,
            ),
        };
        let line_col8 = params.get(Params::LineColor)?.as_color()?.value();
        let safe_frame = params.get(Params::SafeFrame)?.as_slider()?.value() as i64;
        let center_line = params.get(Params::CenterLine)?.as_checkbox()?.value();
        let mut out_col8 = params.get(Params::OutColor)?.as_color()?.value();
        let opacity =
            params.get(Params::OutOpacity)?.as_float_slider()?.value() as f64 / 100.0;
        // 元: RoundByteFpShort(255 * 不透明度) で切り捨て。
        out_col8.alpha = (255.0 * opacity).clamp(0.0, 255.0) as u8;
        let info = Info {
            frame_width: down(fw),
            frame_height: down(fh),
            safe_frame,
            center_line,
            line_height: down(params.get(Params::LineHeight)?.as_slider()?.value() as i64),
        };

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($wr:ident, $max:expr, $float:expr, $conv:expr, $put:expr) => {{
                let mut buf = Buf {
                    w,
                    h,
                    px: vec![Px::default(); (w * h) as usize],
                    max: $max,
                    float: $float,
                    line_height: 1,
                };
                draw(&mut buf, &info, $conv(&line_col8), $conv(&out_col8));
                for y in 0..h {
                    for x in 0..w {
                        $put(
                            out_layer.$wr(x as usize, y as usize),
                            &buf.px[(x + y * w) as usize],
                        );
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                255.0,
                false,
                |c: &ae::Pixel8| Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64
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
                |c: &ae::Pixel8| {
                    let p = libs::utils::conv_8_to_16(c);
                    Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64,
                    }
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
                |c: &ae::Pixel8| {
                    let p = libs::utils::conv_8_to_32(c);
                    Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64,
                    }
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

    fn buf(w: i64, h: i64) -> Buf {
        Buf {
            w,
            h,
            px: vec![Px::default(); (w * h) as usize],
            max: 255.0,
            float: false,
            line_height: 1,
        }
    }

    fn col(r: f64, g: f64, b: f64, a: f64) -> Px {
        Px { a, r, g, b }
    }

    fn info() -> Info {
        Info {
            frame_width: 100,
            frame_height: 60,
            safe_frame: 80,
            center_line: true,
            line_height: 2,
        }
    }

    /// 枠の外は out_color、内側は透明。
    #[test]
    fn the_outside_is_covered_and_the_inside_is_clear() {
        let mut b = buf(200, 120);
        let mut i = info();
        i.center_line = false;
        i.safe_frame = 0;
        i.line_height = 1;
        let out = col(255.0, 0.0, 255.0, 128.0);
        draw(&mut b, &i, col(255.0, 0.0, 0.0, 255.0), out);
        // 枠は (50,30)-(149,89)。左上の外。
        assert_eq!(b.px[(5 + 5 * 200) as usize], out);
        // 内側の真ん中は透明。
        assert_eq!(b.px[(100 + 60 * 200) as usize], Px::default());
    }

    /// 枠の線が引かれる。
    #[test]
    fn the_frame_outline_is_drawn() {
        let mut b = buf(200, 120);
        let mut i = info();
        i.center_line = false;
        i.safe_frame = 0;
        let line = col(255.0, 0.0, 0.0, 255.0);
        draw(&mut b, &i, line, col(0.0, 0.0, 0.0, 0.0));
        // 枠の左上 (50,30) と、太さ 2 ぶん内側 (51,31)。
        assert_eq!(b.px[(50 + 30 * 200) as usize], line);
        assert_eq!(b.px[(51 + 31 * 200) as usize], line);
        // 3画素内側は線でない。
        assert_eq!(b.px[(53 + 33 * 200) as usize], Px::default());
    }

    /// 中心の十字。
    #[test]
    fn the_centre_lines_cross_the_screen() {
        let mut b = buf(200, 120);
        let mut i = info();
        i.safe_frame = 0;
        let line = col(255.0, 0.0, 0.0, 255.0);
        draw(&mut b, &i, line, col(0.0, 0.0, 0.0, 0.0));
        // 中心は (100, 60)、太さ 2 なので 1 戻して y = 59,60。
        assert_eq!(b.px[(3 + 59 * 200) as usize], line, "左端まで横線がある");
        assert_eq!(b.px[(99 + 3 * 200) as usize], line, "上端まで縦線がある");
    }

    /// セーフフレームは枠の内側に入る。
    #[test]
    fn the_safe_frame_sits_inside() {
        let mut b = buf(200, 120);
        let mut i = info();
        i.center_line = false;
        let line = col(255.0, 0.0, 0.0, 255.0);
        draw(&mut b, &i, line, col(0.0, 0.0, 0.0, 0.0));
        // safe は 100*80/100 = 80 × 48 → (60,36)-(139,83)。
        assert_eq!(b.px[(60 + 36 * 200) as usize], line);
    }

    /// 0% と 100% のセーフフレームは引かれない。
    #[test]
    fn the_safe_frame_is_skipped_at_the_ends() {
        for sf in [0, 100] {
            let mut b = buf(200, 120);
            let mut i = info();
            i.center_line = false;
            i.safe_frame = sf;
            let line = col(255.0, 0.0, 0.0, 255.0);
            draw(&mut b, &i, line, col(0.0, 0.0, 0.0, 0.0));
            // 枠の線以外に赤が出ていないこと: 枠の内側 3 画素のところを見る。
            assert_eq!(b.px[(55 + 35 * 200) as usize], Px::default(), "sf = {sf}");
        }
    }

    /// 枠が画面より大きくても落ちず、線だけが見える。
    #[test]
    fn a_frame_bigger_than_the_screen_is_clipped() {
        let mut b = buf(64, 48);
        let mut i = info();
        i.frame_width = 1280;
        i.frame_height = 720;
        i.center_line = false;
        i.safe_frame = 0;
        let out = col(255.0, 0.0, 255.0, 128.0);
        draw(&mut b, &i, col(255.0, 0.0, 0.0, 255.0), out);
        // 枠の内側が画面を覆うので、全部透明になる。
        assert!(b.px.iter().all(|p| *p == Px::default()), "{:?}", b.px[0]);
    }
}
