//! F's CellGrad の移植。
//!
//! 絵の中の「選んだ色のところ」だけを残して、その**かたまりに外接する
//! 四角**を見つけ、指定の角度のグラデーションをそこに合わせて塗る。セル画の
//! ベタ塗りに後から色の転びを足すための道具。
//!
//! 手順 (元の Exec):
//!
//! 1. 入力をそのままコピーする
//! 2. `targetCount` が 0 より大きければ、色見本 (最大8色) のどれかに
//!    `targetLevel` の幅で近い画素だけを残し、ほかは透明にする
//!    (近さは RGB を 8bit に落としてチャンネルごとの差で見る)
//! 3. アルファを RGB に写す (`GradMask`)
//! 4. 赤に半径 3 の最小値フィルタを掛ける (横→縦)。小さな点や細い縁が
//!    消えるので、四角が余計に広がらない
//! 5. 赤が 0 でない画素の外接四角を出す (`scanRect`)
//! 6. その四角と角度から、グラデーションの始点と終点を決める
//!    (`FRectGrad`)。`startOver` / `lastOver` は中心からの伸ばし量の倍率
//! 7. アルファが 0 でない画素に、始点色から終点色へのグラデーションを
//!    塗る (アルファはそのまま)
//! 8. `showGuide` が入っていれば、四角の4辺と始点→終点の線を引く
//!
//! グラデーションの塗り方 (元の FGrad):
//!
//! - 始点と終点を通る「グラデーションに垂直な2本の直線」を引く
//! - 画素ごとに、その画素を通るグラデーション方向の直線と2本の交点を求め、
//!   それぞれまでの距離を全長で割った比で2色を混ぜる
//! - 2本の外側は、近い側の色でべた塗り
//! - 横長 (|dx| >= |dy|) なら行ごと、縦長なら列ごとに走る。外側の判定が
//!   行 (列) ごとの交点で決まるので、どちらを選ぶかで縁が変わる
//!
//! 元から写した細かいところ:
//!
//! - `targetLevel` は `(255 * % / 100 + 0.5)` を切り捨てた 8bit の幅
//! - 最小値フィルタは半径 3 固定 (パラメータに出ていない)
//! - 外接四角は、左は「左から最初に 0 でない画素」の最小、右は「右から
//!   最初に 0 でない画素」の最大、という走り方。何も無ければ
//!   `left = 幅-1, right = 0` のまま潰れた四角になる
//! - グラデーションの方向は `start → last` で、角度は 0 が上、時計回り
//! - 色の混ぜ方は `始点色 * v2 + 終点色 * v1` で、`v1 + v2` は 1 に
//!   なるはずだが丸めの都合でわずかにずれる。切り捨てもそのまま
//! - ガイドの線は不透明な色で上書きする (元の PixelBlend8 は、重ねる色の
//!   アルファが最大なら下地を見ずにその色を返す)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/CellGrad

use after_effects::{self as ae};

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
}

impl Ops {
    /// 元の RoundByteFpLong / RoundShortFpLong / RoundFpShortDouble。
    /// どれも丸めずに切り捨てる。
    fn round(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 32.0)
        } else {
            v.clamp(0.0, self.max).trunc()
        }
    }
    /// このチャンネルの値を 8bit に落とす (色見本との比較用)。
    fn to8(&self, v: f64) -> i32 {
        if self.float {
            // 元の FS_CONVERT32TO8: v * 255 + 0.5 を切り捨てる。
            (v * 255.0 + 0.5).clamp(0.0, 255.0) as i32
        } else if self.max == 255.0 {
            v as i32
        } else {
            // 元の FS_CONVERT16TO8。
            (((v as i64 * 255 + 16384) / 32768) as f64).clamp(0.0, 255.0) as i32
        }
    }
    /// 8bit の色をこの深度へ。
    fn from8(&self, c: &ae::Pixel8) -> Px {
        if self.float {
            let f = libs::utils::conv_8_to_32(c);
            Px {
                a: f.alpha as f64,
                r: f.red as f64,
                g: f.green as f64,
                b: f.blue as f64,
            }
        } else if self.max == 255.0 {
            Px {
                a: c.alpha as f64,
                r: c.red as f64,
                g: c.green as f64,
                b: c.blue as f64,
            }
        } else {
            let f = libs::utils::conv_8_to_16(c);
            Px {
                a: f.alpha as f64,
                r: f.red as f64,
                g: f.green as f64,
                b: f.blue as f64,
            }
        }
    }
}

/// 画素の並び。元の FWorld / FGrad が持つ世界 (ここでは隙間なし)。
struct Buf {
    w: usize,
    h: usize,
    px: Vec<Px>,
}

impl Buf {
    fn at(&self, x: usize, y: usize) -> Px {
        self.px[x + y * self.w]
    }
    fn set(&mut self, x: usize, y: usize, p: Px) {
        self.px[x + y * self.w] = p;
    }
}

/// 元の SelectedPixel。見本のどれにも近くない画素を透明にする。
fn selected_pixel(buf: &mut Buf, cols: &[ae::Pixel8], count: usize, level: i32, ops: &Ops) {
    for p in buf.px.iter_mut() {
        let (r, g, b) = (ops.to8(p.r), ops.to8(p.g), ops.to8(p.b));
        let mut hit = false;
        for c in cols.iter().take(count) {
            if (b - c.blue as i32).abs() <= level
                && (g - c.green as i32).abs() <= level
                && (r - c.red as i32).abs() <= level
            {
                hit = true;
                break;
            }
        }
        if !hit {
            *p = Px::default();
        }
    }
}

/// 元の GradMask。アルファを RGB に写す。
fn grad_mask(buf: &mut Buf) {
    for p in buf.px.iter_mut() {
        p.r = p.a;
        p.g = p.a;
        p.b = p.a;
    }
}

/// 元の Min。赤に半径 `v` の最小値フィルタを横→縦で掛ける。
fn min_red(buf: &mut Buf, v: i64, ops: &Ops) {
    let (w, h) = (buf.w as i64, buf.h as i64);
    let mut scan: Vec<f64> = Vec::new();
    for y in 0..h {
        scan.clear();
        for x in 0..w {
            scan.push(buf.at(x as usize, y as usize).r);
        }
        for x in 0..w {
            let x0 = (x - v).max(0);
            let x1 = (x + v).min(w - 1);
            let mut lo = ops.max;
            for k in x0..=x1 {
                if lo > scan[k as usize] {
                    lo = scan[k as usize];
                }
                if lo <= 0.0 {
                    break;
                }
            }
            buf.px[(x + y * w) as usize].r = lo;
        }
    }
    for x in 0..w {
        scan.clear();
        for y in 0..h {
            scan.push(buf.at(x as usize, y as usize).r);
        }
        for y in 0..h {
            let y0 = (y - v).max(0);
            let y1 = (y + v).min(h - 1);
            let mut lo = ops.max;
            for k in y0..=y1 {
                if lo > scan[k as usize] {
                    lo = scan[k as usize];
                }
                if lo <= 0.0 {
                    break;
                }
            }
            buf.px[(x + y * w) as usize].r = lo;
        }
    }
}

/// 外接四角。元の A_LRect。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}

/// 元の scanRect。赤が 0 でない画素を囲む四角。
fn scan_rect(buf: &Buf) -> Rect {
    let (w, h) = (buf.w as i64, buf.h as i64);
    let mut ret = Rect {
        left: w - 1,
        right: 0,
        top: h - 1,
        bottom: 0,
    };
    for y in 0..h {
        if ret.left > 0 {
            let mut p = w - 1;
            for x in 0..=ret.left {
                if buf.at(x as usize, y as usize).r > 0.0 {
                    p = x;
                    break;
                }
            }
            if ret.left > p {
                ret.left = p;
            }
        }
        if ret.right < w - 1 {
            let mut p = 0;
            let mut x = w - 1;
            while x >= ret.right {
                if buf.at(x as usize, y as usize).r > 0.0 {
                    p = x;
                    break;
                }
                x -= 1;
            }
            if ret.right < p {
                ret.right = p;
            }
        }
    }
    for x in 0..w {
        if ret.top > 0 {
            let mut p = h - 1;
            for y in 0..=ret.top {
                if buf.at(x as usize, y as usize).r > 0.0 {
                    p = y;
                    break;
                }
            }
            if ret.top > p {
                ret.top = p;
            }
        }
        if ret.bottom < h - 1 {
            let mut p = 0;
            let mut y = h - 1;
            while y >= ret.bottom {
                if buf.at(x as usize, y as usize).r > 0.0 {
                    p = y;
                    break;
                }
                y -= 1;
            }
            if ret.bottom < p {
                ret.bottom = p;
            }
        }
    }
    ret
}

/// 元の CrossPointer / FGrad::CrossPoint。2直線の交点。
fn cross(s0: (f64, f64), s1: (f64, f64), e0: (f64, f64), e1: (f64, f64)) -> (f64, f64) {
    let (dx0, dy0) = (s1.0 - s0.0, s1.1 - s0.1);
    let (dx1, dy1) = (e1.0 - e0.0, e1.1 - e0.1);
    let den = dx0 * dy1 - dy0 * dx1;
    if den == 0.0 {
        return (0.0, 0.0);
    }
    let (ax, ay) = (e0.0 - s0.0, e0.1 - s0.1);
    let dr = (dy1 * ax - dx1 * ay) / den;
    (s0.0 + dr * dx0, s0.1 + dr * dy0)
}

/// 元の FRectGrad。四角と角度からグラデーションの両端を出す。
fn rect_grad(rct: &Rect, angle: f64, start_over: f64, last_over: f64) -> ((f64, f64), (f64, f64)) {
    let dx = (rct.right - rct.left) as f64 / 2.0;
    let dy = (rct.bottom - rct.top) as f64 / 2.0;
    let cx = rct.left as f64 + dx;
    let cy = rct.top as f64 + dy;
    // 元: atan2(dx, dy) (引数の順が逆) なので、四角の角が縦から何度かになる。
    let dia = dx.atan2(dy) * 180.0 / std::f64::consts::PI;

    let v = 8000.0;
    let rad = |d: f64| d * std::f64::consts::PI / 180.0;
    // 角度から、中心を通るグラデーション方向の直線。
    let s0 = if (0.0..90.0).contains(&angle) {
        (v * rad(angle).sin(), -v * rad(angle).cos())
    } else if (90.0..180.0).contains(&angle) {
        (v * rad(angle - 90.0).cos(), v * rad(angle - 90.0).sin())
    } else if (180.0..270.0).contains(&angle) {
        (-v * rad(angle - 180.0).sin(), v * rad(angle - 180.0).cos())
    } else if angle >= 270.0 {
        (-v * rad(angle - 270.0).cos(), -v * rad(angle - 270.0).sin())
    } else {
        (0.0, 0.0)
    };
    let s1 = (-s0.0, -s0.1);

    // その直線が四角のどの辺と交わるかを角度で場合分けする。
    let (mut start, mut last) = if (0.0..dia).contains(&angle) || angle >= 360.0 - dia {
        (
            cross(s0, s1, (-v, -dy), (v, -dy)),
            cross(s0, s1, (-v, dy), (v, dy)),
        )
    } else if (dia..180.0 - dia).contains(&angle) {
        (
            cross(s0, s1, (dx, -v), (dx, v)),
            cross(s0, s1, (-dx, -v), (-dx, v)),
        )
    } else if (180.0 - dia..180.0 + dia).contains(&angle) {
        (
            cross(s0, s1, (-v, dy), (v, dy)),
            cross(s0, s1, (-v, -dy), (v, -dy)),
        )
    } else if (180.0 + dia..360.0 - dia).contains(&angle) {
        (
            cross(s0, s1, (-dx, -v), (-dx, v)),
            cross(s0, s1, (dx, -v), (dx, v)),
        )
    } else {
        ((0.0, 0.0), (0.0, 0.0))
    };
    start = (start.0 * start_over + cx, start.1 * start_over + cy);
    last = (last.0 * last_over + cx, last.1 * last_over + cy);
    (start, last)
}

/// 元の FGrad::exec。アルファが 0 でない画素を塗る。
fn grad(
    buf: &mut Buf,
    start: (f64, f64),
    last: (f64, f64),
    start_col: Px,
    last_col: Px,
    ops: &Ops,
) {
    let (dx, dy) = (last.0 - start.0, last.1 - start.1);
    let r = dy.atan2(dx);
    let (ax, ay) = (dx * 10.0, dy * 10.0);
    let length = dx.hypot(dy);
    let rd2 = r + 90.0 * std::f64::consts::PI / 180.0;
    let (px0, py0) = (8000.0 * rd2.cos(), 8000.0 * rd2.sin());
    // 始点と終点を通る、グラデーションに垂直な2本の直線。
    let sv = ((start.0 - px0, start.1 - py0), (start.0 + px0, start.1 + py0));
    let ev = ((last.0 - px0, last.1 - py0), (last.0 + px0, last.1 + py0));

    let (w, h) = (buf.w as i64, buf.h as i64);
    // 元: 横長なら行ごと、縦長なら列ごとに走る。
    let horizontal = dx.abs() >= dy.abs();
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };

    for o in 0..outer {
        // 行 (列) と2本の交点から、外側になる範囲を決める。
        let line = if horizontal {
            ((-8000.0, o as f64), (8000.0, o as f64))
        } else {
            ((o as f64, -8000.0), (o as f64, 8000.0))
        };
        let pick = |p: (f64, f64)| if horizontal { p.0 } else { p.1 };
        let mut i0 = (pick(cross(line.0, line.1, sv.0, sv.1)) + 0.5) as i64;
        let mut i1 = (pick(cross(line.0, line.1, ev.0, ev.1)) + 0.5) as i64;
        let (mut outside0, mut outside1) = (start_col, last_col);
        if i0 > i1 {
            std::mem::swap(&mut i0, &mut i1);
            std::mem::swap(&mut outside0, &mut outside1);
        }
        for i in 0..inner {
            let (x, y) = if horizontal { (i, o) } else { (o, i) };
            let p = buf.at(x as usize, y as usize);
            if p.a == 0.0 {
                continue;
            }
            let mut col = if i < i0 {
                outside0
            } else if i > i1 {
                outside1
            } else {
                // 画素を通るグラデーション方向の直線と2本の交点までの距離。
                let gl = (
                    (x as f64 - ax, y as f64 - ay),
                    (x as f64 + ax, y as f64 + ay),
                );
                let p1 = cross(gl.0, gl.1, sv.0, sv.1);
                let p2 = cross(gl.0, gl.1, ev.0, ev.1);
                let v1 = (p1.0 - x as f64).hypot(p1.1 - y as f64) / length;
                let v2 = (p2.0 - x as f64).hypot(p2.1 - y as f64) / length;
                Px {
                    a: 0.0,
                    r: ops.round(start_col.r * v2 + last_col.r * v1),
                    g: ops.round(start_col.g * v2 + last_col.g * v1),
                    b: ops.round(start_col.b * v2 + last_col.b * v1),
                }
            };
            col.a = p.a;
            buf.set(x as usize, y as usize, col);
        }
    }
}

/// 元の Line8 + setPixelBlend8。ガイドの線を引く。重ねる色は不透明なので
/// そのまま上書きになる。
fn line(buf: &mut Buf, x0: i64, y0: i64, x1: i64, y1: i64, col: Px) {
    let sx = if x1 > x0 { 1 } else { -1 };
    let dx = (x1 - x0).abs();
    let sy = if y1 > y0 { 1 } else { -1 };
    let dy = (y1 - y0).abs();
    let (mut x, mut y) = (x0, y0);
    let (w, h) = (buf.w as i64, buf.h as i64);
    let mut put = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && x < w && y < h {
            buf.px[(x + y * w) as usize] = col;
        }
    };
    if dx >= dy {
        let mut e = -dx;
        for _ in 0..=dx {
            put(x, y);
            x += sx;
            e += 2 * dy;
            if e >= 0 {
                y += sy;
                e -= 2 * dx;
            }
        }
    } else {
        let mut e = -dy;
        for _ in 0..=dy {
            put(x, y);
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
    TargetTopic,
    TargetCount,
    TargetLevel,
    Col1,
    Col2,
    Col3,
    Col4,
    Col5,
    Col6,
    Col7,
    Col8,
    TargetTopicEnd,
    Angle,
    StartOver,
    LastOver,
    StartCol,
    LastCol,
    GuideShow,
    GuideCol,
}

const COLS: [Params; 8] = [
    Params::Col1,
    Params::Col2,
    Params::Col3,
    Params::Col4,
    Params::Col5,
    Params::Col6,
    Params::Col7,
    Params::Col8,
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
        // 元: PF_ADD_TOPIC("target") から PF_END_TOPIC まで。
        params.add_group(
            Params::TargetTopic,
            Params::TargetTopicEnd,
            "target",
            false,
            |p| {
                // 元: PF_ADD_SLIDER("targetCount", 0, 8, 0, 8, 0)
                p.add(
                    Params::TargetCount,
                    "targetCount",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(8);
                        f.set_slider_min(0);
                        f.set_slider_max(8);
                        f.set_default(0);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_FLOAT_SLIDER("targetLevel", 0, 20, 0, 10, 1, 0, 1, 0, 0)
                p.add(
                    Params::TargetLevel,
                    "targetLevel",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(20.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(10.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(0.0);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: 白・赤・緑・青・黄・マゼンタ・暗い黄・暗いマゼンタ。
                let cols = [
                    (0xFFu8, 0xFFu8, 0xFFu8),
                    (0xFF, 0x00, 0x00),
                    (0x00, 0xFF, 0x00),
                    (0x00, 0x00, 0xFF),
                    (0xFF, 0xFF, 0x00),
                    (0xFF, 0x00, 0xFF),
                    (0x80, 0x80, 0x00),
                    (0x80, 0x00, 0x80),
                ];
                for (i, c) in COLS.iter().enumerate() {
                    let (r, g, b) = cols[i];
                    p.add(
                        *c,
                        &format!("color{}", i + 1),
                        ae::ColorDef::setup(move |f| {
                            f.set_default(ae::Pixel8 {
                                alpha: 0xFF,
                                red: r,
                                green: g,
                                blue: b,
                            });
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;
        // 元: PF_ADD_ANGLE("angle", 0)
        params.add(
            Params::Angle,
            "angle",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        for (p, name) in [
            (Params::StartOver, "startOver"),
            (Params::LastOver, "lastOver"),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(-200.0);
                    f.set_valid_max(400.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(200.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(100.0);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_COLOR("startCol", 0xFF, 0xFF, 0xFF)
        params.add(
            Params::StartCol,
            "startCol",
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
        // 元: PF_ADD_COLOR("lastCol", 0x00, 0x00, 0x00)
        params.add(
            Params::LastCol,
            "lastCol",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("showGuide", "on", FALSE)
        params.add(
            Params::GuideShow,
            "showGuide",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR も名前が "showGuide" のまま (元のとおり)。
        params.add(
            Params::GuideCol,
            "showGuide",
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
                out_data.set_return_msg("fs-rs cellgrad");
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
        // 元の GetParams。
        let target_count = params.get(Params::TargetCount)?.as_slider()?.value() as usize;
        let lv = params.get(Params::TargetLevel)?.as_float_slider()?.value() as f64;
        // 元: RoundByteFpLong(255 * lv / 100 + 0.5) なので四捨五入になる。
        let target_level = (255.0 * lv / 100.0 + 0.5).clamp(0.0, 255.0) as i32;
        let mut cols: Vec<ae::Pixel8> = Vec::with_capacity(8);
        for c in COLS.iter() {
            cols.push(params.get(*c)?.as_color()?.value());
        }
        // 元: PF_Fixed を 0..360 に収めてから度にする。
        let a = libs::utils::flt2fix(params.get(Params::Angle)?.as_angle()?.value() as f64) as i64;
        let mut aa = a % (360 << 16);
        if aa < 0 {
            aa += 360 << 16;
        }
        let angle = aa as f64 / 65536.0;
        let start_over = params.get(Params::StartOver)?.as_float_slider()?.value() as f64 / 100.0;
        let last_over = params.get(Params::LastOver)?.as_float_slider()?.value() as f64 / 100.0;
        let start_col8 = params.get(Params::StartCol)?.as_color()?.value();
        let last_col8 = params.get(Params::LastCol)?.as_color()?.value();
        let guide_show = params.get(Params::GuideShow)?.as_checkbox()?.value();
        let guide_col8 = params.get(Params::GuideCol)?.as_color()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                // 元の Exec: まず入力をコピー。
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
                if target_count > 0 {
                    selected_pixel(&mut buf, &cols, target_count, target_level, &ops);
                }
                grad_mask(&mut buf);
                min_red(&mut buf, 3, &ops);
                let rct = scan_rect(&buf);
                let (start, last) = rect_grad(&rct, angle, start_over, last_over);
                grad(
                    &mut buf,
                    start,
                    last,
                    ops.from8(&start_col8),
                    ops.from8(&last_col8),
                    &ops,
                );
                if guide_show {
                    let gc = ops.from8(&guide_col8);
                    line(&mut buf, rct.left, rct.top, rct.right, rct.top, gc);
                    line(&mut buf, rct.right, rct.top, rct.right, rct.bottom, gc);
                    line(&mut buf, rct.right, rct.bottom, rct.left, rct.bottom, gc);
                    line(&mut buf, rct.left, rct.bottom, rct.left, rct.top, gc);
                    line(
                        &mut buf,
                        start.0 as i64,
                        start.1 as i64,
                        last.0 as i64,
                        last.1 as i64,
                        gc,
                    );
                }
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), &buf.at(x, y));
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
                    float: false
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
                    float: false
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
                    float: true
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
        }
    }

    fn buf(w: usize, h: usize) -> Buf {
        Buf {
            w,
            h,
            px: vec![Px::default(); w * h],
        }
    }

    fn opaque(r: f64, g: f64, b: f64) -> Px {
        Px {
            a: 255.0,
            r,
            g,
            b,
        }
    }

    /// 見本に近い色だけが残る。
    #[test]
    fn only_the_sampled_colours_survive() {
        let mut b = buf(3, 1);
        b.px[0] = opaque(255.0, 0.0, 0.0);
        b.px[1] = opaque(250.0, 5.0, 5.0);
        b.px[2] = opaque(0.0, 0.0, 255.0);
        let cols = [ae::Pixel8 {
            alpha: 255,
            red: 255,
            green: 0,
            blue: 0,
        }];
        selected_pixel(&mut b, &cols, 1, 10, &ops8());
        assert_eq!(b.px[0].a, 255.0);
        assert_eq!(b.px[1].a, 255.0, "幅 10 の中なので残る");
        assert_eq!(b.px[2], Px::default());
    }

    /// 幅が 0 ならぴったり同じ色だけ。
    #[test]
    fn a_zero_level_needs_an_exact_match() {
        let mut b = buf(2, 1);
        b.px[0] = opaque(255.0, 0.0, 0.0);
        b.px[1] = opaque(254.0, 0.0, 0.0);
        let cols = [ae::Pixel8 {
            alpha: 255,
            red: 255,
            green: 0,
            blue: 0,
        }];
        selected_pixel(&mut b, &cols, 1, 0, &ops8());
        assert_eq!(b.px[0].a, 255.0);
        assert_eq!(b.px[1], Px::default());
    }

    /// 最小値フィルタは小さな点を消す。
    #[test]
    fn the_min_filter_eats_a_lone_dot() {
        let mut b = buf(16, 16);
        b.px[8 + 8 * 16] = opaque(255.0, 255.0, 255.0);
        grad_mask(&mut b);
        min_red(&mut b, 3, &ops8());
        assert!(b.px.iter().all(|p| p.r == 0.0), "点が残っている");
    }

    /// 大きなかたまりは残り、外接四角が取れる。
    #[test]
    fn a_solid_block_keeps_its_box() {
        let mut b = buf(32, 32);
        for y in 8..24 {
            for x in 6..20 {
                b.set(x, y, opaque(255.0, 255.0, 255.0));
            }
        }
        grad_mask(&mut b);
        min_red(&mut b, 3, &ops8());
        let r = scan_rect(&b);
        // 半径 3 ぶん内側に縮む。
        assert_eq!(r.left, 9);
        assert_eq!(r.right, 16);
        assert_eq!(r.top, 11);
        assert_eq!(r.bottom, 20);
    }

    /// 何も無ければ潰れた四角が返る。
    #[test]
    fn an_empty_mask_gives_a_collapsed_box() {
        let b = buf(10, 6);
        let r = scan_rect(&b);
        assert_eq!(
            r,
            Rect {
                left: 9,
                top: 5,
                right: 0,
                bottom: 0
            }
        );
    }

    /// 角度 0 なら上から下へのグラデーション。
    #[test]
    fn angle_zero_runs_from_top_to_bottom() {
        let r = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let (s, l) = rect_grad(&r, 0.0, 1.0, 1.0);
        assert!((s.0 - 50.0).abs() < 0.01, "{s:?}");
        assert!((s.1 - 0.0).abs() < 0.01, "{s:?}");
        assert!((l.1 - 100.0).abs() < 0.01, "{l:?}");
    }

    /// 角度 90 なら右から左へ (元の場合分けのまま)。
    #[test]
    fn angle_ninety_runs_sideways() {
        let r = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let (s, l) = rect_grad(&r, 90.0, 1.0, 1.0);
        assert!((s.1 - 50.0).abs() < 0.01, "{s:?}");
        assert!((s.0 - 100.0).abs() < 0.01, "{s:?}");
        assert!((l.0 - 0.0).abs() < 0.01, "{l:?}");
    }

    /// 伸ばし量を 2 倍にすると両端が中心から倍に離れる。
    #[test]
    fn the_over_values_stretch_the_ends() {
        let r = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let (s, l) = rect_grad(&r, 0.0, 2.0, 2.0);
        assert!((s.1 - (-50.0)).abs() < 0.01, "{s:?}");
        assert!((l.1 - 150.0).abs() < 0.01, "{l:?}");
    }

    /// グラデーションは始点色から終点色へ移る。アルファは残る。
    #[test]
    fn the_gradient_runs_between_the_two_colours() {
        let mut b = buf(1, 11);
        for y in 0..11 {
            b.set(0, y, opaque(0.0, 0.0, 0.0));
        }
        let white = opaque(255.0, 255.0, 255.0);
        let black = opaque(0.0, 0.0, 0.0);
        grad(&mut b, (0.0, 0.0), (0.0, 10.0), white, black, &ops8());
        assert_eq!(b.at(0, 0).r, 255.0);
        assert_eq!(b.at(0, 10).r, 0.0);
        assert!(b.at(0, 5).r > 100.0 && b.at(0, 5).r < 155.0, "{:?}", b.at(0, 5));
        assert_eq!(b.at(0, 5).a, 255.0, "アルファは残る");
    }

    /// 透明な画素は塗らない。
    #[test]
    fn transparent_pixels_are_left_alone() {
        let mut b = buf(1, 4);
        let white = opaque(255.0, 255.0, 255.0);
        grad(&mut b, (0.0, 0.0), (0.0, 3.0), white, white, &ops8());
        assert!(b.px.iter().all(|p| *p == Px::default()));
    }

    /// 両端の外側はべた塗り。
    #[test]
    fn outside_the_ends_is_flat() {
        let mut b = buf(1, 21);
        for y in 0..21 {
            b.set(0, y, opaque(0.0, 0.0, 0.0));
        }
        let white = opaque(255.0, 255.0, 255.0);
        let black = opaque(0.0, 0.0, 0.0);
        // 5..15 のあいだだけグラデーション。
        grad(&mut b, (0.0, 5.0), (0.0, 15.0), white, black, &ops8());
        assert_eq!(b.at(0, 0).r, 255.0);
        assert_eq!(b.at(0, 4).r, 255.0);
        assert_eq!(b.at(0, 20).r, 0.0);
    }

    /// ガイドの線は画面の外で切れる。
    #[test]
    fn the_guide_line_is_clipped() {
        let mut b = buf(8, 8);
        let red = opaque(255.0, 0.0, 0.0);
        line(&mut b, -4, -4, 4, 4, red);
        assert_eq!(b.at(0, 0), red);
        assert_eq!(b.at(4, 4), red);
        assert_eq!(b.at(5, 5), Px::default());
    }
}
