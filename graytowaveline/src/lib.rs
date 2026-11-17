//! F's grayToWaveLine の移植。
//!
//! 入力を灰色にして、縦横に粗くサンプリングし、その明るさを**折れ線グラフの
//! 高さ**に読み替えて描く。音の波形を何段も並べたような絵になる。入力の絵は
//! 残らない (キャンバスを消してから線だけを描く)。
//!
//! 手順 (元の exec8 / exec16):
//!
//! 1. 「波を描く」設定なら、まず入力を灰色にして出力へ書く
//!    (`red*0.29891 + green*0.58661 + blue*0.11448`、アルファが最大未満なら
//!    そのぶん暗くする)
//! 2. 縦に `height / SampleYCount` 行ずつ平均して上から `SampleYCount` 行に
//!    詰め、横に `width / SampleXCount` 画素ずつ平均して左から
//!    `SampleXCount` 画素に詰める。**行数・画素数が足りなければその段は
//!    何もしない** (生の画素がそのまま標本になる)
//! 3. 標本を `SampleXCount × SampleYCount` の 0..1 の表にする
//! 4. キャンバスを消す
//! 5. 「下敷きを描く」設定なら、段ごとに水平な線 (GraphTilt ぶん傾く) を
//!    BaseColor で引く
//! 6. 「波を描く」設定なら、段ごとに標本を結ぶ折れ線を GraphColor で引く。
//!    高さは `段の y - GraphYMax * 標本 + GraphTilt * (xc / SampleXCount)`
//!
//! 元から写した細かいところ:
//!
//! - `SampleXCount` は幅、`SampleYCount` は高さで上から押さえる
//! - 線は Bresenham で、`LineHeight` 行ぶん**下に**伸ばす。太らせるのは下
//!   だけで、上には伸びない
//! - 線は合成せずに置き換える (アルファごと上書き)
//! - 折れ線の y は `(A_long)(y + 0.5)` なので、負の側では 0 方向へ切り捨て
//! - 平均は `(A_long)(合計 / 個数 + 0.5)` で、上だけを最大値で押さえる
//! - 位置は `PF_ADD_POINT` の値を 16 ビット右シフトしたもの (切り下げ)
//! - 標本の表は `SampleXCount * SampleYCount * 2` 個ぶん確保されるが、
//!   使うのは前半だけ
//!
//! 元と違うところ:
//!
//! - **32bit (float) では元は入力をそのまま流すだけで、何も描かない**
//!   (`case PF_PixelFormat_ARGB128: ae->CopyInToOut()`)。PiPL は
//!   FloatColorAware を立てているので、これは落としただけと見て 8/16bit と
//!   同じ絵を描くようにした。`FAITHFUL` を立てると元どおり素通しになる
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/grayToWaveLine

use after_effects::{self as ae};

/// 32bit で元どおり「何も描かない」に戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    sx_count: i64,
    sy_count: i64,
    draw_x: i64,
    draw_y: i64,
    graph_x_scale: i64,
    graph_y_max: i64,
    graph_tilt: i64,
    draw_x_offset: i64,
    draw_y_offset: i64,
    line_height: i64,
    mode: i64,
}

/// 元の MODE_*。ポップアップは 1 から数える。
const MODE_ALL: i64 = 1;
const MODE_WAVE: i64 = 2;
const MODE_BASE: i64 = 3;

/// 深度ごとのチャンネルの振る舞い。元の PF_MAX_CHAN8 / PF_MAX_CHAN16 と、
/// 「double を整数に丸めて上だけ押さえる」ところをまとめたもの。
trait Chan: Copy {
    const CMAX: f64;
    fn to_f64(self) -> f64;
    /// 元: `v = (A_long)(d + 0.5); if (v > MAX) v = MAX;`
    /// 32bit には元に相当する処理がないので、丸めずに押さえるだけ。
    fn quant(d: f64) -> f64;
}

impl Chan for u8 {
    const CMAX: f64 = 255.0;
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn quant(d: f64) -> f64 {
        (((d + 0.5) as i64) as f64).min(Self::CMAX)
    }
}

impl Chan for u16 {
    const CMAX: f64 = 32768.0;
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn quant(d: f64) -> f64 {
        (((d + 0.5) as i64) as f64).min(Self::CMAX)
    }
}

impl Chan for f32 {
    const CMAX: f64 = 1.0;
    fn to_f64(self) -> f64 {
        self as f64
    }
    fn quant(d: f64) -> f64 {
        if d > Self::CMAX {
            Self::CMAX
        } else {
            d
        }
    }
}

/// 元の gray* + sampling*。入力の灰色化から標本の表までを一息でやる。
///
/// `gray` は `(red, green, blue, alpha)` を並べた入力の読み出し。元は灰色の
/// 絵を出力のバッファに書いて、そこを読みながら平均を詰めていくので、
/// ここでも幅 `w` ぶんの作業用バッファを1枚持って同じ順で潰していく。
fn gray_sample<C: Chan>(
    gray: &mut dyn FnMut(usize, usize) -> (f64, f64, f64, f64),
    w: i64,
    h: i64,
    sx_count: i64,
    sy_count: i64,
) -> Vec<f64> {
    // 元の gray*: 灰色にして出力へ。
    let mut buf = vec![0.0f64; (w * h) as usize];
    for y in 0..h {
        for x in 0..w {
            let (r, g, b, a) = gray(x as usize, y as usize);
            let mut v = r * 0.29891 + g * 0.58661 + b * 0.11448;
            if a < C::CMAX {
                v *= a / C::CMAX;
            }
            buf[(x + y * w) as usize] = C::quant(v);
        }
    }

    // 元の sampling*: 縦に平均して上に詰める。行が足りなければ何もしない。
    let sy = h / sy_count;
    if h > sy_count {
        for yy in 0..sy_count {
            for x in 0..w {
                let mut v = 0.0f64;
                for k in 0..sy {
                    v += buf[(x + (yy * sy + k) * w) as usize];
                }
                buf[(x + yy * w) as usize] = C::quant(v / sy as f64);
            }
        }
    }
    // 横に平均して左に詰める。読む先は書く先より必ず右なので壊れない。
    let sx = w / sx_count;
    if w > sx_count {
        for yy in 0..sy_count {
            let mut src = yy * w;
            for x in 0..sx_count {
                let mut v = 0.0f64;
                for _ in 0..sx {
                    v += buf[src as usize];
                    src += 1;
                }
                buf[(x + yy * w) as usize] = C::quant(v / sx as f64);
            }
        }
    }

    let mut out = vec![0.0f64; (sx_count * sy_count) as usize];
    for y in 0..sy_count {
        for x in 0..sx_count {
            out[(x + y * sx_count) as usize] = buf[(x + y * w) as usize] / C::CMAX;
        }
    }
    out
}

/// 引く線1本。元の LineInfo の位置だけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Seg {
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
    /// 下敷きの色で引くか (そうでなければグラフの色)
    base: bool,
}

/// 元の drawGraph*。引く線を順に並べる。
fn plan(info: &Info, samples: &[f64]) -> Vec<Seg> {
    let mut out = Vec::new();
    if info.mode == MODE_ALL || info.mode == MODE_BASE {
        for yc in 0..info.sy_count {
            let x_pos = info.draw_x + yc * info.draw_x_offset;
            let y_pos = info.draw_y + yc * info.draw_y_offset;
            out.push(Seg {
                x0: x_pos,
                y0: y_pos,
                x1: x_pos + info.sx_count * info.graph_x_scale,
                y1: y_pos + info.graph_tilt,
                base: true,
            });
        }
    }
    if info.mode == MODE_ALL || info.mode == MODE_WAVE {
        for yc in 0..info.sy_count {
            let x_pos = info.draw_x + yc * info.draw_x_offset;
            let y_pos = info.draw_y + yc * info.draw_y_offset;
            let target = info.sx_count * yc;
            for xc in 1..info.sx_count {
                let at = |i: i64| -> i64 {
                    let mut y = y_pos as f64 - info.graph_y_max as f64 * samples[(target + i) as usize];
                    y += info.graph_tilt as f64 * i as f64 / info.sx_count as f64;
                    // 元: (A_long)(y + 0.5) なので負の側は 0 方向へ切り捨て。
                    (y + 0.5) as i64
                };
                out.push(Seg {
                    x0: x_pos + (xc - 1) * info.graph_x_scale,
                    y0: at(xc - 1),
                    x1: x_pos + xc * info.graph_x_scale,
                    y1: at(xc),
                    base: false,
                });
            }
        }
    }
    out
}

/// 元の Line8/Line16。Bresenham で、`line_height` 行ぶん下に伸ばす。
fn raster(seg: &Seg, w: i64, h: i64, line_height: i64, mut put: impl FnMut(i64, i64)) {
    let sx = if seg.x1 > seg.x0 { 1 } else { -1 };
    let dx = (seg.x1 - seg.x0).abs();
    let sy = if seg.y1 > seg.y0 { 1 } else { -1 };
    let dy = (seg.y1 - seg.y0).abs();
    let (mut x, mut y) = (seg.x0, seg.y0);
    let mut e;
    let mut dot = |x: i64, y: i64| {
        for lh in 0..line_height {
            let y2 = y + lh;
            if x >= 0 && x < w && y2 >= 0 && y2 < h {
                put(x, y2);
            }
        }
    };
    if dx >= dy {
        e = -dx;
        for _ in 0..=dx {
            dot(x, y);
            x += sx;
            e += 2 * dy;
            if e >= 0 {
                y += sy;
                e -= 2 * dx;
            }
        }
    } else {
        e = -dy;
        for _ in 0..=dy {
            dot(x, y);
            y += sy;
            e += 2 * dx;
            if e >= 0 {
                x += sx;
                e -= 2 * dy;
            }
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    SxCount,
    SyCount,
    DrawPos,
    GraphXScale,
    GraphYMax,
    Tilt,
    XOffset,
    YOffset,
    LineHeight,
    GraphColor,
    BaseColor,
    Mode,
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
        // 元: PF_ADD_SLIDER の並びそのまま。
        for (p, name, vmin, vmax, smin, smax, dflt) in [
            (Params::SxCount, "SampleXCount", 50, 2000, 100, 500, 300),
            (Params::SyCount, "SampleYCount", 1, 1000, 1, 50, 20),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(move |f| {
                    f.set_valid_min(vmin);
                    f.set_valid_max(vmax);
                    f.set_slider_min(smin);
                    f.set_slider_max(smax);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_POINT("DrawPos", 25, 15)
        params.add(
            Params::DrawPos,
            "DrawPos",
            ae::PointDef::setup(|f| {
                f.set_default((25.0, 15.0));
                f.set_value(f.default());
            }),
        )?;
        for (p, name, vmin, vmax, smin, smax, dflt) in [
            (Params::GraphXScale, "GraphXScale", 1, 100, 2, 50, 5),
            (Params::GraphYMax, "GraphYMax", -1000, 1000, -200, 200, 100),
            (Params::Tilt, "GraphTilt", -1000, 1000, -200, 200, 0),
            (Params::XOffset, "GraphXOffset", -1000, 1000, -500, 500, 0),
            (Params::YOffset, "GraphYOffset", -1000, 1000, -500, 500, 0),
            (Params::LineHeight, "LineHeight", 1, 20, 1, 6, 3),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(move |f| {
                    f.set_valid_min(vmin);
                    f.set_valid_max(vmax);
                    f.set_slider_min(smin);
                    f.set_slider_max(smax);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_COLOR("GraphColor", 0xFF, 0xFF, 0xFF)
        params.add(
            Params::GraphColor,
            "GraphColor",
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
        // 元: PF_ADD_COLOR("BaseColor", 0x80, 0x80, 0x80)
        params.add(
            Params::BaseColor,
            "BaseColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0x80,
                    green: 0x80,
                    blue: 0x80,
                });
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP("mode", 3, 1, "Wave+Base|WaveOnly|BaseOnly")
        params.add(
            Params::Mode,
            "mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Wave+Base", "WaveOnly", "BaseOnly"]);
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
                out_data.set_return_msg("fs-rs graytowaveline");
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
        let sl = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(params.get(p)?.as_slider()?.value() as i64)
        };
        // 元の GetParams の順。
        let mut sx_count = sl(Params::SxCount, params)?;
        if sx_count <= 0 {
            sx_count = 1;
        }
        let mut sy_count = sl(Params::SyCount, params)?;
        if sy_count <= 0 {
            sy_count = 1;
        }
        // 元: pnt.x >> 16 (切り下げ)。
        let (px, py) = params.get(Params::DrawPos)?.as_point()?.value();
        let draw_x = (libs::utils::flt2fix(px as f64) as i64) >> 16;
        let draw_y = (libs::utils::flt2fix(py as f64) as i64) >> 16;
        let mut info = Info {
            sx_count,
            sy_count,
            draw_x,
            draw_y,
            graph_x_scale: sl(Params::GraphXScale, params)?,
            graph_y_max: sl(Params::GraphYMax, params)?,
            graph_tilt: sl(Params::Tilt, params)?,
            draw_x_offset: sl(Params::XOffset, params)?,
            draw_y_offset: sl(Params::YOffset, params)?,
            line_height: sl(Params::LineHeight, params)?,
            mode: params.get(Params::Mode)?.as_popup()?.value() as i64,
        };
        let graph_colour = params.get(Params::GraphColor)?.as_color()?.value();
        let base_colour = params.get(Params::BaseColor)?.as_color()?.value();

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }
        // 元の Exec: 標本の数は画面の大きさで押さえる。
        if info.sx_count > w {
            info.sx_count = w;
        }
        if info.sy_count > h {
            info.sy_count = h;
        }

        if FAITHFUL && depth == 32 {
            // 元: 32bit は入力をそのまま流すだけ。
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

        macro_rules! run {
            ($chan:ty, $rd:ident, $wr:ident, $zero:expr, $graph:expr, $base:expr) => {{
                // 波を描くときだけ標本を採る。元は BaseOnly では灰色化も
                // しない (標本の表は使われないまま捨てられる)。
                let samples = if info.mode == MODE_ALL || info.mode == MODE_WAVE {
                    let mut rd = |x: usize, y: usize| {
                        let p = in_layer.$rd(x, y);
                        (
                            Chan::to_f64(p.red),
                            Chan::to_f64(p.green),
                            Chan::to_f64(p.blue),
                            Chan::to_f64(p.alpha),
                        )
                    };
                    gray_sample::<$chan>(&mut rd, w, h, info.sx_count, info.sy_count)
                } else {
                    vec![0.0; (info.sx_count * info.sy_count) as usize]
                };
                // 元の out->clear()。
                for y in 0..h {
                    for x in 0..w {
                        *out_layer.$wr(x as usize, y as usize) = $zero;
                    }
                }
                for seg in &plan(&info, &samples) {
                    let col = if seg.base { $base } else { $graph };
                    raster(seg, w, h, info.line_height, |x, y| {
                        *out_layer.$wr(x as usize, y as usize) = col;
                    });
                }
            }};
        }
        match depth {
            8 => run!(
                u8,
                as_pixel8,
                as_pixel8_mut,
                ae::Pixel8 {
                    alpha: 0,
                    red: 0,
                    green: 0,
                    blue: 0
                },
                graph_colour,
                base_colour
            ),
            16 => run!(
                u16,
                as_pixel16,
                as_pixel16_mut,
                ae::Pixel16 {
                    alpha: 0,
                    red: 0,
                    green: 0,
                    blue: 0
                },
                libs::utils::conv_8_to_16(&graph_colour),
                libs::utils::conv_8_to_16(&base_colour)
            ),
            _ => run!(
                f32,
                as_pixel32,
                as_pixel32_mut,
                ae::PixelF32 {
                    alpha: 0.0,
                    red: 0.0,
                    green: 0.0,
                    blue: 0.0
                },
                libs::utils::conv_8_to_32(&graph_colour),
                libs::utils::conv_8_to_32(&base_colour)
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info {
            sx_count: 8,
            sy_count: 2,
            draw_x: 4,
            draw_y: 20,
            graph_x_scale: 5,
            graph_y_max: 100,
            graph_tilt: 0,
            draw_x_offset: 0,
            draw_y_offset: 30,
            line_height: 3,
            mode: MODE_ALL,
        }
    }

    /// 真っ白な入力の標本は 1、真っ黒なら 0。
    #[test]
    fn a_flat_input_gives_a_flat_sample() {
        let mut white = |_x: usize, _y: usize| (255.0, 255.0, 255.0, 255.0);
        let s = gray_sample::<u8>(&mut white, 64, 16, 8, 2);
        assert_eq!(s.len(), 16);
        assert!(s.iter().all(|v| *v == 1.0), "{s:?}");
        let mut black = |_x: usize, _y: usize| (0.0, 0.0, 0.0, 255.0);
        let s = gray_sample::<u8>(&mut black, 64, 16, 8, 2);
        assert!(s.iter().all(|v| *v == 0.0), "{s:?}");
    }

    /// 透明なところは暗くなる。アルファ半分の白はおよそ 0.5。
    #[test]
    fn alpha_dims_the_sample() {
        let mut half = |_x: usize, _y: usize| (255.0, 255.0, 255.0, 128.0);
        let s = gray_sample::<u8>(&mut half, 64, 16, 8, 2);
        assert!(s.iter().all(|v| (*v - 128.0 / 255.0).abs() < 0.01), "{s:?}");
    }

    /// 標本の数が画素より多ければ縦横の平均は飛ばされ、生の画素が標本になる。
    #[test]
    fn too_many_samples_read_the_raw_pixels() {
        // 左半分が白、右半分が黒。幅 8・標本 8 なので横の平均は走らない。
        let mut f = |x: usize, _y: usize| {
            let v = if x < 4 { 255.0 } else { 0.0 };
            (v, v, v, 255.0)
        };
        let s = gray_sample::<u8>(&mut f, 8, 2, 8, 2);
        assert_eq!(s[0], 1.0);
        assert_eq!(s[4], 0.0);
    }

    /// 下敷きだけの指定なら段ごとに1本ずつ、波だけなら標本の数 -1 本ずつ。
    #[test]
    fn the_mode_picks_which_lines_are_drawn() {
        let s = vec![0.5; 16];
        let mut i = info();
        i.mode = MODE_BASE;
        let p = plan(&i, &s);
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|g| g.base));
        i.mode = MODE_WAVE;
        let p = plan(&i, &s);
        assert_eq!(p.len(), 2 * 7);
        assert!(p.iter().all(|g| !g.base));
        i.mode = MODE_ALL;
        assert_eq!(plan(&i, &s).len(), 2 + 2 * 7);
    }

    /// 標本が大きいほど線は上に行く (y は下向き)。
    #[test]
    fn a_brighter_sample_lifts_the_line() {
        let i = {
            let mut i = info();
            i.mode = MODE_WAVE;
            i
        };
        let dark = plan(&i, &vec![0.0; 16]);
        let bright = plan(&i, &vec![1.0; 16]);
        assert_eq!(dark[0].y0, 20);
        // 20 - 100 = -80 に +0.5 して 0 方向へ切り捨てるので -79。
        // 元の (A_long)(y + 0.5) がそうなっている。
        assert_eq!(bright[0].y0, -79);
    }

    /// 段ごとに X/Y のずらしが効く。
    #[test]
    fn each_row_is_offset() {
        let mut i = info();
        i.mode = MODE_BASE;
        i.draw_x_offset = 7;
        let p = plan(&i, &vec![0.0; 16]);
        assert_eq!(p[0].x0, 4);
        assert_eq!(p[1].x0, 11);
        assert_eq!(p[1].y0, 50);
    }

    /// 線は LineHeight 行ぶん下に伸びる。上には伸びない。
    #[test]
    fn the_line_only_thickens_downwards() {
        let seg = Seg {
            x0: 0,
            y0: 5,
            x1: 3,
            y1: 5,
            base: false,
        };
        let mut hit = Vec::new();
        raster(&seg, 16, 16, 3, |x, y| hit.push((x, y)));
        assert_eq!(hit.len(), 4 * 3);
        assert_eq!(hit.iter().map(|p| p.1).min(), Some(5));
        assert_eq!(hit.iter().map(|p| p.1).max(), Some(7));
    }

    /// 画面の外は捨てられる。
    #[test]
    fn the_line_is_clipped() {
        let seg = Seg {
            x0: -20,
            y0: -20,
            x1: 4,
            y1: 4,
            base: false,
        };
        let mut hit = Vec::new();
        raster(&seg, 8, 8, 1, |x, y| hit.push((x, y)));
        assert!(hit.iter().all(|(x, y)| (0..8).contains(x) && (0..8).contains(y)));
        assert_eq!(hit.len(), 5);
    }

    /// 傾きが 1 より大きいときも端から端まで引く。
    #[test]
    fn a_steep_line_runs_end_to_end() {
        let seg = Seg {
            x0: 2,
            y0: 0,
            x1: 3,
            y1: 10,
            base: false,
        };
        let mut hit = Vec::new();
        raster(&seg, 16, 16, 1, |x, y| hit.push((x, y)));
        assert_eq!(hit.len(), 11);
        assert_eq!(hit[0], (2, 0));
        assert_eq!(hit[10].1, 10);
    }
}
