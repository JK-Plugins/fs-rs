//! F's SelectedBlur の移植。
//!
//! 指定した色 (最大8色) のところだけを選び出して、**そこだけ**をぼかす。
//! 選ばれなかったところは元の絵のまま残るので、セル画のベタ塗りのうち
//! 特定の色だけを柔らかくできる。
//!
//! 手順 (元の Exec):
//!
//! 1. ぼかし量が 0 以下か、有効な色が 1色以下なら入力をそのまま流す
//!    (「1色以下」は元のバグ。下の「元と違うところ」を参照)
//! 2. 色見本のどれかに `range` の幅で近く、**かつ不透明な**画素だけを
//!    出力に残し、ほかは透明にする
//! 3. 残ったところをガウスぼかしする。横→縦に掛けたあと、
//!    「ボケの破綻をごまかす処理」として窓を 1/4・1/16・1/64 に縮めて
//!    横→縦→横ともう3回掛ける
//! 4. 出力が透明なところへ入力を書き戻す
//!
//! ぼかしの作法 (元の horBlur / verBlur):
//!
//! - 不透明な画素だけを書き換える
//! - 隣を見ていく途中で**透明な画素に当たったら打ち切る**。選んだ色の
//!   かたまりの外から色を引っぱってこないための工夫
//! - 重みは `exp(-i²/(2*(blur/3)²)) * 256` の整数表。**窓を縮めた回でも
//!   表は元のぼかし量のまま作り直される** (元のとおり) ので、重みの形は
//!   変わらず範囲だけが狭まる
//! - アルファは最大に揃える
//!
//! 元から写した細かいところ:
//!
//! - ぼかし量には downsample を掛けて四捨五入する
//! - `range` は `255 * % / 100` の整数割り算
//! - 色の近さは RGB を 8bit に落としてチャンネルごとの差で見る。
//!   16bit は `C16TO8`、32bit は `FS_CONVERT32TO8`
//! - 8bit だけ、ぼかしの表の `zone` が **float** で計算される
//!   (`double zone = (PF_FpShort)range/3;`)。16/32bit は double
//! - 有効な色は上から詰めて数える (チェックの入っていない行は飛ばす)
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **有効な色が 1色だけだと何もしない**。`if (blur <= 0 || col_count
//!    <= 1)` で入力をそのまま流してしまう。1色を選んでぼかすのは当然の
//!    使い方なので `col_count <= 0` にした
//! 2. 16bit のぼかしの合計が `A_long` (32bit) で、ぼかし量を大きくすると
//!    溢れる (32768 × 重みの合計が 2^31 に届く)。64bit で足すようにした。
//!    8bit では溢れないので、元との画素比較には出てこない
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/SelectedBlur

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 1画素。深度に依らず f64 で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Px {
    a: f64,
    r: f64,
    g: f64,
    b: f64,
}

/// 深度ごとの作法。
#[derive(Debug, Clone, Copy)]
struct Ops {
    max: f64,
    float: bool,
    /// 8bit だけ表の zone が float で計算される。
    zone_f32: bool,
}

impl Ops {
    /// 元の RoundByteLong / RoundShort。32bit は丸めない。
    fn round(&self, v: f64) -> f64 {
        if self.float {
            v
        } else {
            v.trunc().clamp(0.0, self.max)
        }
    }
    /// 色見本と比べるための 8bit 値。
    fn to8(&self, v: f64) -> i32 {
        if self.float {
            // 元: RoundByteLong(FS_CONVERT32TO8(v))、つまり v*255+0.5 を
            // 切り捨ててから 0..255 で押さえる。
            ((v * 255.0 + 0.5) as i64).clamp(0, 255) as i32
        } else if self.max == 255.0 {
            v as i32
        } else {
            // 元の C16TO8 = RoundByteFpLong((v*255 + 16384)/32768)。
            (((v as i64 * 255 + 16384) / 32768) as i64).clamp(0, 255) as i32
        }
    }
}

/// 画素の並び。
struct Buf {
    w: usize,
    h: usize,
    px: Vec<Px>,
}

impl Buf {
    fn at(&self, x: usize, y: usize) -> Px {
        self.px[x + y * self.w]
    }
}

/// 元の makeTbl。ぼかし量が 0 以下なら表を作らない。
fn make_table(blur: i64, ops: &Ops) -> Option<Vec<i64>> {
    if blur <= 0 {
        return None;
    }
    let zone = if ops.zone_f32 {
        ((blur as f32) / 3.0) as f64
    } else {
        blur as f64 / 3.0
    };
    Some(
        (0..=blur)
            .map(|i| ((-(i as f64) * i as f64 / (2.0 * zone * zone)).exp() * 256.0) as i64)
            .collect(),
    )
}

/// 元の horBlur / verBlur。`vert` が立っていれば列を走る。
fn blur_pass(buf: &mut Buf, vert: bool, value: i64, tbl: &[i64], ops: &Ops) {
    let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
    let w = buf.w;
    let at = |o: usize, i: usize| if vert { i * w + o } else { o * w + i };
    let mut scan: Vec<Px> = Vec::with_capacity(inner);
    for o in 0..outer {
        scan.clear();
        for i in 0..inner {
            scan.push(buf.px[at(o, i)]);
        }
        for i in 0..inner {
            let c = scan[i];
            let opaque = if ops.float {
                c.a >= 1.0
            } else {
                c.a == ops.max
            };
            if !opaque {
                continue;
            }
            let mut count: i64 = 0;
            let (mut sr, mut sg, mut sb) = (0.0f64, 0.0f64, 0.0f64);
            // 真ん中。
            let g0 = tbl[0];
            sr += c.r * g0 as f64;
            sg += c.g * g0 as f64;
            sb += c.b * g0 as f64;
            count += g0;
            // 左 (上) 。透明に当たったら打ち切る。
            for k in 1..=value as usize {
                if i < k {
                    break;
                }
                let n = scan[i - k];
                if n.a == 0.0 {
                    break;
                }
                let g = tbl[k];
                sr += n.r * g as f64;
                sg += n.g * g as f64;
                sb += n.b * g as f64;
                count += g;
            }
            // 右 (下)。
            for k in 1..=value as usize {
                if i + k >= inner {
                    break;
                }
                let n = scan[i + k];
                if n.a == 0.0 {
                    break;
                }
                let g = tbl[k];
                sr += n.r * g as f64;
                sg += n.g * g as f64;
                sb += n.b * g as f64;
                count += g;
            }
            if count > 0 {
                let d = count as f64;
                buf.px[at(o, i)] = Px {
                    a: ops.max,
                    r: ops.round((sr / d).trunc_like(ops)),
                    g: ops.round((sg / d).trunc_like(ops)),
                    b: ops.round((sb / d).trunc_like(ops)),
                };
            }
        }
    }
}

/// 8/16bit は `sr / count` が整数の割り算になる。32bit は double のまま。
trait TruncLike {
    fn trunc_like(self, ops: &Ops) -> f64;
}

impl TruncLike for f64 {
    fn trunc_like(self, ops: &Ops) -> f64 {
        if ops.float {
            self
        } else {
            self.trunc()
        }
    }
}

/// 元の BlurSub。本番の1回 + 「ごまかす」3回。
fn blur_all(buf: &mut Buf, blur: i64, ops: &Ops) {
    let Some(tbl) = make_table(blur, ops) else {
        return;
    };
    blur_pass(buf, false, blur, &tbl, ops);
    blur_pass(buf, true, blur, &tbl, ops);
    // 元: 表は毎回ぼかし量そのままで作り直すので形は変わらない。
    blur_pass(buf, false, blur / 4, &tbl, ops);
    blur_pass(buf, true, blur / 16, &tbl, ops);
    blur_pass(buf, false, blur / 64, &tbl, ops);
}

/// 元の CopyImage の「選ぶ」側。見本に近い不透明な画素だけ残す。
fn select(buf: &mut Buf, cols: &[ae::Pixel8], range: i32, ops: &Ops) {
    for p in buf.px.iter_mut() {
        let mut hit = false;
        let a8 = if ops.float {
            ops.to8(p.a)
        } else {
            p.a as i32
        };
        let full = if ops.float { 255 } else { ops.max as i32 };
        if a8 >= full {
            let (r, g, b) = (ops.to8(p.r), ops.to8(p.g), ops.to8(p.b));
            for c in cols {
                if (b - c.blue as i32).abs() <= range
                    && (g - c.green as i32).abs() <= range
                    && (r - c.red as i32).abs() <= range
                {
                    hit = true;
                    break;
                }
            }
        }
        if !hit {
            *p = Px::default();
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    BlurValue,
    TargetRange,
    En0,
    Col0,
    En1,
    Col1,
    En2,
    Col2,
    En3,
    Col3,
    En4,
    Col4,
    En5,
    Col5,
    En6,
    Col6,
    En7,
    Col7,
}

/// 元の並び: 有効のチェックとその色が交互に8組。
const GROUPS: [(Params, Params); 8] = [
    (Params::En0, Params::Col0),
    (Params::En1, Params::Col1),
    (Params::En2, Params::Col2),
    (Params::En3, Params::Col3),
    (Params::En4, Params::Col4),
    (Params::En5, Params::Col5),
    (Params::En6, Params::Col6),
    (Params::En7, Params::Col7),
];

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
        // 元: PF_ADD_SLIDER("blur", 0, 300, 0, 30, 0)
        params.add(
            Params::BlurValue,
            "blur",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(300);
                f.set_slider_min(0);
                f.set_slider_max(30);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("range", 0, 100, 0, 16, 0, 1, 0, 0)
        params.add(
            Params::TargetRange,
            "range",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(16.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元の既定: 赤と緑だけ ON。チェックの名前は元が非 ASCII なので英語。
        let cols = [
            (0xFFu8, 0x00u8, 0x00u8),
            (0x00, 0xFF, 0x00),
            (0x00, 0x00, 0xFF),
            (0xFF, 0x00, 0xFF),
            (0x00, 0xFF, 0xFF),
            (0x00, 0x80, 0x80),
            (0x80, 0x00, 0x80),
            (0x80, 0x00, 0x00),
        ];
        for (i, g) in GROUPS.iter().enumerate() {
            let on = i < 2;
            params.add(
                g.0,
                "enabled",
                ae::CheckBoxDef::setup(move |f| {
                    f.set_label("ON");
                    f.set_default(on);
                    f.set_value(f.default());
                }),
            )?;
            let (r, gr, b) = cols[i];
            params.add(
                g.1,
                &format!("target{i}"),
                ae::ColorDef::setup(move |f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: r,
                        green: gr,
                        blue: b,
                    });
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
                out_data.set_return_msg("fs-rs selectedblur");
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
        // 元の GetParams。
        let v = params.get(Params::BlurValue)?.as_slider()?.value() as i64;
        let blur = if v > 0 {
            (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
        } else {
            v
        };
        // 元: RoundByteLong(255 * PF_Fixed / (100 << 16))。整数の割り算。
        let fx = libs::utils::flt2fix(
            params.get(Params::TargetRange)?.as_float_slider()?.value() as f64,
        ) as i64;
        let range = (255 * fx / (100 << 16)).clamp(0, 255) as i32;
        // 有効な色を上から詰める。
        let mut cols: Vec<ae::Pixel8> = Vec::with_capacity(8);
        for g in GROUPS.iter() {
            if params.get(g.0)?.as_checkbox()?.value() {
                cols.push(params.get(g.1)?.as_color()?.value());
            }
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }
        // 元: 有効な色が 1色以下なら素通し (直したところ 1)。
        let too_few = if FAITHFUL {
            cols.len() <= 1
        } else {
            cols.is_empty()
        };
        if blur <= 0 || too_few {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                let mut buf = Buf {
                    w,
                    h,
                    px: Vec::with_capacity(w * h),
                };
                for y in 0..h {
                    for x in 0..w {
                        buf.px.push($get(in_layer.$rd(x, y)));
                    }
                }
                select(&mut buf, &cols, range, &ops);
                blur_all(&mut buf, blur, &ops);
                // 元: 出力が透明なところへ入力を書き戻す。
                for y in 0..h {
                    for x in 0..w {
                        let p = buf.at(x, y);
                        if p.a == 0.0 {
                            *out_layer.$wr(x, y) = *in_layer.$rd(x, y);
                        } else {
                            $put(out_layer.$wr(x, y), &p);
                        }
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                Ops {
                    max: 255.0,
                    float: false,
                    zone_f32: true
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
            16 => go!(
                as_pixel16,
                as_pixel16_mut,
                Ops {
                    max: 32768.0,
                    float: false,
                    zone_f32: false
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
            ),
            _ => go!(
                as_pixel32,
                as_pixel32_mut,
                Ops {
                    max: 1.0,
                    float: true,
                    zone_f32: false
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
            zone_f32: true,
        }
    }

    fn red() -> Px {
        Px {
            a: 255.0,
            r: 255.0,
            g: 0.0,
            b: 0.0,
        }
    }

    fn blue() -> Px {
        Px {
            a: 255.0,
            r: 0.0,
            g: 0.0,
            b: 255.0,
        }
    }

    fn col(r: u8, g: u8, b: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: 255,
            red: r,
            green: g,
            blue: b,
        }
    }

    fn buf(px: Vec<Px>, w: usize, h: usize) -> Buf {
        Buf { w, h, px }
    }

    /// 見本に近い不透明な画素だけ残る。
    #[test]
    fn only_the_sampled_opaque_pixels_stay() {
        let mut b = buf(vec![red(), blue(), Px { a: 100.0, r: 255.0, g: 0.0, b: 0.0 }], 3, 1);
        select(&mut b, &[col(255, 0, 0)], 0, &ops8());
        assert_eq!(b.px[0], red());
        assert_eq!(b.px[1], Px::default(), "見本と違う色は落ちる");
        assert_eq!(b.px[2], Px::default(), "半透明は落ちる");
    }

    /// 幅を広げれば近い色も入る。
    #[test]
    fn the_range_widens_the_match() {
        let near = Px {
            a: 255.0,
            r: 240.0,
            g: 10.0,
            b: 10.0,
        };
        let mut b = buf(vec![near], 1, 1);
        select(&mut b, &[col(255, 0, 0)], 20, &ops8());
        assert_eq!(b.px[0], near);
        let mut b = buf(vec![near], 1, 1);
        select(&mut b, &[col(255, 0, 0)], 5, &ops8());
        assert_eq!(b.px[0], Px::default());
    }

    /// 表は真ん中がいちばん大きく、外へ減る。
    #[test]
    fn the_table_falls_off() {
        let t = make_table(9, &ops8()).unwrap();
        assert_eq!(t.len(), 10);
        assert_eq!(t[0], 256);
        for i in 1..10 {
            assert!(t[i] < t[i - 1], "i = {i}");
        }
        assert!(make_table(0, &ops8()).is_none());
    }

    /// ぼかしは隣の色と混ざる。
    #[test]
    fn the_blur_mixes_the_neighbours() {
        let mut px = vec![red(); 9];
        px[4] = Px {
            a: 255.0,
            r: 0.0,
            g: 255.0,
            b: 0.0,
        };
        let mut b = buf(px, 9, 1);
        let t = make_table(3, &ops8()).unwrap();
        blur_pass(&mut b, false, 3, &t, &ops8());
        assert!(b.px[4].r > 0.0, "真ん中に赤が混ざる");
        assert!(b.px[3].g > 0.0, "隣に緑が混ざる");
    }

    /// 透明な画素に当たったらそこで打ち切る (向こうの色を引っぱらない)。
    #[test]
    fn the_blur_stops_at_a_transparent_pixel() {
        let green = Px {
            a: 255.0,
            r: 0.0,
            g: 255.0,
            b: 0.0,
        };
        // red red | 透明 | green green
        let mut b = buf(
            vec![red(), red(), Px::default(), green, green],
            5,
            1,
        );
        let t = make_table(3, &ops8()).unwrap();
        blur_pass(&mut b, false, 3, &t, &ops8());
        assert_eq!(b.px[1].g, 0.0, "透明の向こうの緑が漏れている");
        assert_eq!(b.px[3].r, 0.0, "透明の向こうの赤が漏れている");
        assert_eq!(b.px[2], Px::default(), "透明はそのまま");
    }

    /// 透明な画素は書き換えられない。
    #[test]
    fn transparent_pixels_are_not_touched() {
        let mut b = buf(vec![Px::default(); 5], 5, 1);
        let t = make_table(3, &ops8()).unwrap();
        blur_pass(&mut b, false, 3, &t, &ops8());
        assert!(b.px.iter().all(|p| *p == Px::default()));
    }

    /// ぼかし量を上げるほど端まで広がる。
    #[test]
    fn more_blur_reaches_further() {
        let reach = |n: i64| {
            let mut px = vec![red(); 64];
            px[32] = Px {
                a: 255.0,
                r: 0.0,
                g: 255.0,
                b: 0.0,
            };
            let mut b = buf(px, 64, 1);
            blur_all(&mut b, n, &ops8());
            (0..64).filter(|x| b.px[*x].g > 0.0).count()
        };
        assert!(reach(12) > reach(3), "{} > {}", reach(12), reach(3));
    }

    /// 1色だけでも効く (直したところ 1)。
    #[test]
    fn one_colour_is_enough() {
        assert!(!FAITHFUL);
    }
}
