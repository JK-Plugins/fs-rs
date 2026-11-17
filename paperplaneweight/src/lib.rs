//! F's PaperPlaneGetWeightInfo の移植。
//!
//! 双子の `paperplanegetweight` が RGB に埋めた「紙の重さ」のデータを
//! 読み戻して、**重さの合計と重心**を絵の上に出す道具。絵を作るための
//! ものではなく、紙飛行機の設計を確かめるための計りに近い。
//!
//! 手順 (元の Exec):
//!
//! 1. 入力をそのままコピーする
//! 2. `chkWeightTable`: 列ごとに、アルファがいっぱいの画素だけを見て
//!    重さを足す。1画素ぶんは
//!    - `blue` の下4ビットが枚数 `c`、上4ビットが `c2`
//!    - `green + red` が重さのもと
//!    - `Glue` が入っていて `c > 1` なら、のりの重さを足す
//!      (`c == c2` なら `Glue × (c-1)`、違えば倍)
//!    - 36 で割って、A1 の紙 (788×1091mm を 300dpi にした画素数) ぶんの
//!      グラム数に直す
//! 3. `drawBalance`: 合計を `weight:0.00g` の形で (30, 300) に白い字で
//!    書き、`DrawGraph` が入っていれば列ごとの重さを高さ
//!    `重さ × 100000` の赤い棒グラフにして重ね、重心のところに
//!    5画素幅の線 (`{255,255,0,255}` なので実際は赤紫) を引く
//!
//! 元から写した細かいところ:
//!
//! - 字は元の `AEDebugAngel.cpp` にある 12×18 のビットマップフォント
//!   (ASCII 32〜126)。`font.bin` にそのまま落としてある。1ドットを
//!   2×2 画素に広げて、フォントの値をアルファにして重ねる
//! - 空白 (32) は**書かない** (元の `c <= STR_START` の比較がそうなっている)
//! - 字は画面の幅に入る文字数で打ち切る
//! - 重心は `Σ(x × 重さ) / Σ重さ + 0.5` の切り捨て。**重さが 0 のときは
//!   0 で割る**ので、元では線が出ない (`(A_long)NaN` が大きな負の数に
//!   なって、線を引くところで範囲の外として捨てられる)。ここも線を
//!   引かないようにしてある
//! - 棒グラフと線は下から上へ積む (アルファ合成)
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **16/32bit では何もしていなかった** (`Exec` の分岐が空で、入力の
//!    コピーだけ)。データは 8bit の目盛りで埋まっているので、読むときに
//!    8bit へ直してから同じ道を通すようにした
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/PaperPlaneGetWeightInfo

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 元の AEDebugAngel のフォント (ASCII 32..126、12×18)。
static FONT: &[u8] = include_bytes!("font.bin");
const FONT_W: i64 = 12;
const FONT_H: i64 = 18;
const FONT_FIRST: u8 = 32;
const FONT_LAST: u8 = 126;
/// 元の DRAWSCALE。
const SCALE: i64 = 2;

/// 1画素。深さに依らず生の目盛りで持つ。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深さごとの目盛り。
#[derive(Clone, Copy, Debug)]
pub struct Ops {
    pub max: f64,
    pub float: bool,
}

impl Ops {
    /// 8bit の目盛りに直す (埋まっているデータを読むため)。
    fn to8(&self, p: &Px) -> (i64, i64, i64, i64) {
        if self.float {
            let q = libs::utils::conv_32_to_8(&ae::sys::PF_PixelFloat {
                alpha: p.a as f32,
                red: p.r as f32,
                green: p.g as f32,
                blue: p.b as f32,
            });
            (q.alpha as i64, q.red as i64, q.green as i64, q.blue as i64)
        } else if self.max > 255.0 {
            let q = libs::utils::conv_16_to_8(&ae::sys::PF_Pixel16 {
                alpha: p.a as u16,
                red: p.r as u16,
                green: p.g as u16,
                blue: p.b as u16,
            });
            (q.alpha as i64, q.red as i64, q.green as i64, q.blue as i64)
        } else {
            (p.a as i64, p.r as i64, p.g as i64, p.b as i64)
        }
    }

    /// 8bit で書かれた色をその深さに直す。
    fn from8(&self, a: u8, r: u8, g: u8, b: u8) -> Px {
        let c = ae::Pixel8 {
            alpha: a,
            red: r,
            green: g,
            blue: b,
        };
        if self.float {
            let q = libs::utils::conv_8_to_32(&c);
            Px {
                a: q.alpha as f64,
                r: q.red as f64,
                g: q.green as f64,
                b: q.blue as f64,
            }
        } else if self.max > 255.0 {
            let q = libs::utils::conv_8_to_16(&c);
            Px {
                a: q.alpha as f64,
                r: q.red as f64,
                g: q.green as f64,
                b: q.blue as f64,
            }
        } else {
            Px {
                a: a as f64,
                r: r as f64,
                g: g as f64,
                b: b as f64,
            }
        }
    }
}

/// 画素の並び。
pub struct Buf {
    pub w: i64,
    pub h: i64,
    pub px: Vec<Px>,
}

impl Buf {
    pub fn new(w: i64, h: i64) -> Self {
        Buf {
            w,
            h,
            px: vec![Px::default(); (w * h).max(0) as usize],
        }
    }

    pub fn at(&self, x: i64, y: i64) -> Px {
        self.px[(x + y * self.w) as usize]
    }

    pub fn set(&mut self, x: i64, y: i64, p: Px) {
        self.px[(x + y * self.w) as usize] = p;
    }
}

/// 元の PixelBlend8。`col` を下地の上に重ねる。
fn pixel_blend(src: &Px, dst: &Px, ops: &Ops) -> Px {
    let m = ops.max;
    if src.a == 0.0 || dst.a >= m {
        return *dst;
    }
    if dst.a == 0.0 {
        return *src;
    }
    if ops.float {
        let as2 = (1.0 - dst.a) * src.a;
        let ad2 = (dst.a + as2).min(1.0);
        if ad2 <= 0.0 {
            return Px::default();
        }
        let mix = |d: f64, s: f64| ((d * dst.a + s * as2) / ad2).min(1.0);
        Px {
            a: ad2,
            r: mix(dst.r, src.r),
            g: mix(dst.g, src.g),
            b: mix(dst.b, src.b),
        }
    } else {
        let mm = m as i64;
        let shift = if mm > 255 { 15 } else { 8 };
        let top = if mm > 255 { mm } else { mm + 1 };
        let as2 = ((top - dst.a as i64) * src.a as i64) >> shift;
        let ad2 = (dst.a as i64 + as2).min(mm);
        if ad2 <= 0 {
            return Px::default();
        }
        let mix =
            |d: f64, s: f64| (((d as i64 * dst.a as i64 + s as i64 * as2) / ad2).min(mm)) as f64;
        Px {
            a: ad2 as f64,
            r: mix(dst.r, src.r),
            g: mix(dst.g, src.g),
            b: mix(dst.b, src.b),
        }
    }
}

/// 元の setPixelBlend8 (範囲の外は書かない)。
fn blend_px(buf: &mut Buf, x: i64, y: i64, col: &Px, ops: &Ops) {
    if x < 0 || y < 0 || x >= buf.w || y >= buf.h {
        return;
    }
    let s = buf.at(x, y);
    buf.set(x, y, pixel_blend(&s, col, ops));
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub draw_graph: bool,
    pub glue: i64,
}

/// 元の chkWeightTable。列ごとの重さと、合計・重心を出す。
pub fn weight_table(inp: &Buf, info: &Info, ops: &Ops) -> (Vec<f64>, f64, Option<i64>) {
    // 元: A1 の紙 (788 × 1091 mm) を 300dpi にした画素数。
    let area = (788.0 * 300.0_f64 / 25.4) * (1091.0 * 300.0 / 25.4);
    let mut table = vec![0.0f64; inp.w as usize];
    for x in 0..inp.w {
        let mut ww = 0.0f64;
        for y in 0..inp.h {
            let p = inp.at(x, y);
            let (a, r, g, b) = ops.to8(&p);
            if a != 255 {
                continue;
            }
            let c = b & 0xF;
            let c2 = (b >> 4) & 0xF;
            let mut v = g as f64 + r as f64;
            if info.glue > 0 && c > 1 {
                v += if c == c2 {
                    (info.glue * (c - 1)) as f64
                } else {
                    (info.glue * (c - 1) * 2) as f64
                };
            }
            v /= 36.0;
            v = v * 180.0 / area;
            ww += v;
        }
        table[x as usize] = ww;
    }
    let mut mm = 0.0f64;
    let mut all = 0.0f64;
    for x in 0..inp.w {
        mm += x as f64 * table[x as usize];
        all += table[x as usize];
    }
    // 元は 0 で割って `(A_long)NaN` になり、線を引くところで捨てられる。
    let centre = if all == 0.0 {
        None
    } else {
        Some((mm / all + 0.5) as i64)
    };
    (table, all, centre)
}

/// 元の DispAngelStr8。字1つを 2×2 に広げて重ねる。
fn draw_char(out: &mut Buf, c: u8, x: i64, y: i64, col: &Px, ops: &Ops) {
    // 元は空白 (32) も書かない (`c <= STR_START`)。
    if c <= FONT_FIRST || c > FONT_LAST {
        return;
    }
    let base = ((c - FONT_FIRST) as i64 * FONT_W * FONT_H) as usize;
    for j in 0..FONT_H {
        for i in 0..FONT_W {
            let v = FONT[base + (i + j * FONT_W) as usize];
            if v == 0 {
                continue;
            }
            let mut p = *col;
            p.a = ops.from8(v, 0, 0, 0).a;
            let (xx, yy) = (x + i * SCALE, y + j * SCALE);
            blend_px(out, xx, yy, &p, ops);
            blend_px(out, xx + 1, yy, &p, ops);
            blend_px(out, xx, yy + 1, &p, ops);
            blend_px(out, xx + 1, yy + 1, &p, ops);
        }
    }
}

/// 元の DispAngel。文字列を書く (画面の幅に入るぶんだけ)。
fn draw_text(out: &mut Buf, msg: &str, x: i64, y: i64, col: &Px, ops: &Ops) {
    let bytes = msg.as_bytes();
    let max = (out.w / FONT_W) as usize;
    let n = bytes.len().min(max);
    for (i, c) in bytes[..n].iter().enumerate() {
        draw_char(out, *c, x + i as i64 * FONT_W * SCALE, y, col, ops);
    }
}

/// 元の YLine8 (太さ1、縦1本)。
fn y_line(out: &mut Buf, x: i64, y0: i64, y1: i64, col: &Px, ops: &Ops) {
    if x < 0 || x >= out.w {
        return;
    }
    let (mut a, mut b) = (y0, y1);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    }
    if a >= out.h || b < 0 {
        return;
    }
    a = a.max(0);
    b = b.min(out.h - 1);
    for y in a..=b {
        let s = out.at(x, y);
        out.set(x, y, pixel_blend(&s, col, ops));
    }
}

/// 元の drawBalance。
pub fn draw_balance(out: &mut Buf, info: &Info, table: &[f64], weight: f64, centre: Option<i64>, ops: &Ops) {
    // 元: 小数2桁に丸めてから "weight:%4.2fg"。
    let ww = (weight * 100.0 + 0.5) as i64;
    let ww2 = ww as f64 / 100.0;
    let msg = format!("weight:{ww2:4.2}g");
    let white = ops.from8(255, 255, 255, 255);
    draw_text(out, &msg, 30, 300, &white, ops);

    if info.draw_graph {
        // 元: {128,255,0,0} = アルファ 128 の赤。
        let red = ops.from8(128, 255, 0, 0);
        for x in 0..out.w {
            let g = (table[x as usize] * 100000.0 + 0.5) as i64;
            for y in 0..g.min(out.h) {
                let s = out.at(x, y);
                out.set(x, y, pixel_blend(&s, &red, ops));
            }
        }
    }
    // 元: {255,255,0,255} なので赤紫。重心のところに5画素ぶん。
    if let Some(c) = centre {
        let line = ops.from8(255, 255, 0, 255);
        for d in -2..=2 {
            y_line(out, c + d, 0, out.h, &line, ops);
        }
    }
}

/// 元の Exec。
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    out.px.copy_from_slice(&inp.px);
    let (table, weight, centre) = weight_table(inp, info, ops);
    draw_balance(out, info, &table, weight, centre, ops);
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    DrawGraph,
    Glue,
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
        // 元: PF_ADD_CHECKBOX("DrawGraph", "on", TRUE) + CANNOT_TIME_VARY
        params.add_with_flags(
            Params::DrawGraph,
            "DrawGraph",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
            ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::NONE,
        )?;
        // 元: PF_ADD_SLIDER("Glue", 0, 25, 0, 15, 7) + CANNOT_TIME_VARY
        params.add_with_flags(
            Params::Glue,
            "Glue",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(25);
                f.set_slider_min(0);
                f.set_slider_max(15);
                f.set_default(7);
                f.set_value(f.default());
            }),
            ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::NONE,
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
                out_data.set_return_msg("fs-rs paperplaneweight");
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
        let info = Info {
            draw_graph: params.get(Params::DrawGraph)?.as_checkbox()?.value(),
            glue: params.get(Params::Glue)?.as_slider()?.value() as i64,
        };
        let depth = out_layer.bit_depth();
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };

        // 元の Exec: まず入力をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if FAITHFUL && depth > 8 {
            // 元は 8bit 以外では何もしない。
            return Ok(());
        }
        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        if w == 0 || h == 0 {
            return Ok(());
        }
        let mut inp = Buf::new(w, h);
        let mut out = Buf::new(w, h);
        macro_rules! go {
            ($wr:ident, $get:expr, $put:expr) => {{
                let src = &in_layer;
                for y in 0..h {
                    for x in 0..w {
                        inp.set(x, y, $get(src.$wr(x as usize, y as usize)));
                    }
                }
                exec(&inp, &mut out, &info, &ops);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x as usize, y as usize), &out.at(x, y));
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
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

    fn ops8() -> Ops {
        Ops {
            max: 255.0,
            float: false,
        }
    }

    fn info() -> Info {
        Info {
            draw_graph: true,
            glue: 7,
        }
    }

    /// データの入った画素だけが重さに入る。
    #[test]
    fn only_the_opaque_data_counts() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(4, 1);
        // アルファいっぱい、枚数 1、重さ 128+127。
        inp.px[0] = Px {
            a: 255.0,
            r: 127.0,
            g: 128.0,
            b: 0x11 as f64,
        };
        // アルファが足りないので数えない。
        inp.px[1] = Px {
            a: 128.0,
            r: 127.0,
            g: 128.0,
            b: 0x11 as f64,
        };
        let (t, all, centre) = weight_table(&inp, &i, &o);
        assert!(t[0] > 0.0);
        assert_eq!(t[1], 0.0);
        assert_eq!(all, t[0]);
        assert_eq!(centre, Some(0));
    }

    /// `Glue` は枚数が2枚以上のところだけ重くする。
    #[test]
    fn the_glue_only_adds_from_two_sheets() {
        let o = ops8();
        let mut i = info();
        let mut inp = Buf::new(2, 1);
        inp.px[0] = Px {
            a: 255.0,
            r: 10.0,
            g: 10.0,
            b: 0x11 as f64,
        };
        inp.px[1] = Px {
            a: 255.0,
            r: 10.0,
            g: 10.0,
            b: 0x33 as f64,
        };
        let (t0, _, _) = weight_table(&inp, &i, &o);
        i.glue = 0;
        let (t1, _, _) = weight_table(&inp, &i, &o);
        assert_eq!(t0[0], t1[0], "1枚のところは変わらない");
        assert!(t0[1] > t1[1], "3枚のところは重くなる");
    }

    /// 枚数の上下4ビットが違えば、のりは倍になる。
    #[test]
    fn a_mismatched_count_doubles_the_glue() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(2, 1);
        inp.px[0] = Px {
            a: 255.0,
            r: 0.0,
            g: 0.0,
            b: 0x22 as f64,
        };
        inp.px[1] = Px {
            a: 255.0,
            r: 0.0,
            g: 0.0,
            b: 0x12 as f64,
        };
        let (t, _, _) = weight_table(&inp, &i, &o);
        assert!((t[1] - t[0] * 2.0).abs() < 1e-12, "{} {}", t[0], t[1]);
    }

    /// 重心は重さの偏りに寄る。
    #[test]
    fn the_centre_leans_to_the_weight() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(11, 1);
        for x in 8..11 {
            inp.set(
                x,
                0,
                Px {
                    a: 255.0,
                    r: 100.0,
                    g: 100.0,
                    b: 0x11 as f64,
                },
            );
        }
        let (_, _, centre) = weight_table(&inp, &i, &o);
        assert_eq!(centre, Some(9));
    }

    /// データが無ければ線を引かない。
    #[test]
    fn nothing_to_weigh_draws_no_line() {
        let o = ops8();
        let i = info();
        let inp = Buf::new(8, 8);
        let mut out = Buf::new(8, 8);
        exec(&inp, &mut out, &i, &o);
        assert!(out.px.iter().all(|p| *p == Px::default()));
    }

    /// グラフと重心の線が出る。
    #[test]
    fn the_graph_and_the_line_show_up() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(64, 64);
        for x in 20..40 {
            for y in 20..40 {
                inp.set(
                    x,
                    y,
                    Px {
                        a: 255.0,
                        r: 200.0,
                        g: 200.0,
                        b: 0x22 as f64,
                    },
                );
            }
        }
        let mut out = Buf::new(64, 64);
        exec(&inp, &mut out, &i, &o);
        // 重心は塗りの真ん中あたり。
        let (_, _, centre) = weight_table(&inp, &i, &o);
        let c = centre.unwrap();
        assert!((29..=30).contains(&c), "{c}");
        assert!(out.at(c, 0).b > 0.0, "線が引かれる (赤紫)");
        assert!(out.at(25, 0).r > 0.0, "棒グラフが下から立つ");
        assert_eq!(out.at(5, 0), Px::default(), "データの無い列は何も無い");
    }

    /// 字は 2×2 に広げて書かれる (空白は書かない)。
    #[test]
    fn the_text_is_drawn_double_size() {
        let o = ops8();
        let mut out = Buf::new(64, 64);
        let white = o.from8(255, 255, 255, 255);
        draw_text(&mut out, "A", 0, 0, &white, &o);
        let n = out.px.iter().filter(|p| p.a > 0.0).count();
        assert!(n > 0, "何か書かれる");
        // 空白は書かない。
        let mut out2 = Buf::new(64, 64);
        draw_text(&mut out2, " ", 0, 0, &white, &o);
        assert!(out2.px.iter().all(|p| p.a == 0.0));
    }
}
