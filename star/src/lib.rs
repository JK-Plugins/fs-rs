//! F's Star の移植。
//!
//! 絵の明るいところから4方向へ伸びる光条 (クロスフィルター) を描く。
//! 明るさの「山の縁」を拾って、そこから角度ちがいの線を4本ずつ引く。
//!
//! 手順 (元の Exec):
//!
//! 1. 出力を消す
//! 2. `TargetCopy`: 入力の RGB の最大値にアルファを掛けた値を、出力の
//!    RGBA 全部に入れる
//! 3. `MinMask`: アルファだけに半径 5 (downsample 済み) の最小値フィルタを
//!    横→縦に掛ける
//! 4. `SubtrackAlpha`: `アルファ = 赤 - アルファ`。2 で入れた明るさから
//!    3 で縮めたものを引くので、**明るい山の縁だけ**が残る
//! 5. RGB を決める。`Kind` が Screen なら入力の乗算済み RGB、そうでなければ
//!    `Color A` に 2 の明るさを掛けたもの
//! 6. `StarDraw`: アルファが 0 でない画素ごとに、4本の線を引く。線の長さと
//!    強さはその画素のアルファに比例する
//! 7. `AlphaMake`: アルファを RGB の最大値にする
//! 8. 乗算を戻す (AE の `premultiply(0)`)
//!
//! 線の引き方 (元の DrawLine / makeTable / calcColorTable):
//!
//! - 長さぶんの色の表をあらかじめ作る。`i` 歩目の濃さは
//!   `strong * (長さ - i) / 長さ` で、先が細くなる
//! - `Color Border(%)` までは `Color A` から `Color B` へ混ぜ、そこから先は
//!   `Color B` だけ (A Only / B Only / Screen のときは1色だけ)
//! - 角度から 1 歩ぶんのベクトルを固定小数で出し、中心から両方向へ
//!   表の順に置く。置くときは**小数位置の4画素へスクリーン合成**
//! - 画面の外へ出たら打ち切る
//!
//! 元から写した細かいところ:
//!
//! - `Length(%)` / `Strong(%)` / `Color Border(%)` は固定小数を
//!   `/6553600` して 0..1 にした **f32**
//! - 線の長さは `スライダー * downsample` を整数に落としてから
//!   `Length(%)` を掛けて整数に落とす。0..2500 で押さえる
//! - 線の角度は `各線の角度 + 全体の角度` を 0..360 に収め、
//!   **180 以上なら 180 引く** (線は両方向に伸びるので半周でよい)
//! - 最小値フィルタは「前の窓の最小値の位置」を覚える早回しで、
//!   0 が見つかったら打ち切る
//! - 色の表の濃さは `RoundByteFpLong` (切り捨て)
//!
//! 直っているように見えて**結果は変わらないところ**:
//!
//! - 小数位置へ置くときの判定が `(xd0 == 1) && (yd0 = 1)` になっている
//!   (`==` の打ち間違い)。`yd0` に 1 を代入したうえで常に真になるので、
//!   「x がちょうど画素に乗っていれば y が小数でも1画素にしか置かない」
//!   という振る舞いに見える。ただし **x が画素にぴったり乗るのは
//!   1歩ぶんのずれが画素の整数倍のときだけ**で、それは角度が 0 か 90 の
//!   ときに限られ (`dx = ベクトル / 長さ` の整数割り算なので、0 か 65536
//!   しか出ない)、そのときは `dy` も 0 か 65536 なので y も必ず画素に
//!   乗る。つまりこの枝に入るのは両方整数のときだけで、出る絵は同じ。
//!   ここでは `==` に直してあるが、AEXCompat で 9通り突き合わせても
//!   違いは出なかった
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Star

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 色の表の上限。元の TABLE_SIZE。
const TABLE_SIZE: usize = 2500;

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
    /// 元の RoundByteLong / RoundShort / RoundFpShortDouble。
    fn round(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 32.0)
        } else {
            v.trunc().clamp(0.0, self.max)
        }
    }
}

/// 1本ぶんの線。元の LineInfo。
#[derive(Debug, Clone, Copy, Default)]
struct Line {
    length: i64,
    strong: f32,
    /// 固定小数 (1/65536 度)。
    angle: i64,
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone, Copy)]
struct Info {
    kind: i64,
    col1: ae::Pixel8,
    col2: ae::Pixel8,
    border: f32,
    line: [Line; 4],
}

/// 元の skXxx。
const SK_A_TO_B: i64 = 1;
const SK_SCREEN: i64 = 5;

/// 画素の並び。
struct Buf {
    w: i64,
    h: i64,
    px: Vec<Px>,
}

/// 元の CRotCalc::SetRotLength (固定小数版)。角度と長さからベクトルを出す。
fn rot_vector_fixed(angle_fx: i64, length_fx: i64) -> (i64, i64) {
    if length_fx == 0 {
        return (0, 0);
    }
    let mut rr = angle_fx % (360 << 16);
    if rr < 0 {
        rr += 360 << 16;
    }
    let r2 = rr as f64 / 65536.0;
    let v2 = length_fx as f64 / 65536.0;
    let rad = |d: f64| d * std::f64::consts::PI / 180.0;
    let (x, y) = if rr < (90 << 16) {
        (rad(r2).sin() * v2, -rad(r2).cos() * v2)
    } else if rr < (180 << 16) {
        (rad(r2 - 90.0).cos() * v2, rad(r2 - 90.0).sin() * v2)
    } else if rr < (270 << 16) {
        (-rad(r2 - 180.0).sin() * v2, rad(r2 - 180.0).cos() * v2)
    } else {
        (-rad(r2 - 270.0).cos() * v2, -rad(r2 - 270.0).sin() * v2)
    };
    (libs::utils::flt2fix(x) as i64, libs::utils::flt2fix(y) as i64)
}

/// 元の MixPx。スクリーン合成で RGB だけを混ぜる (アルファは下地のまま)。
///
/// 重み `a` は元が **PF_FpShort (f32)** で、掛け算もそのまま f32 なので、
/// ここでも f32 で掛けてから落とす (double でやると丸め境界で 1 ずれる)。
fn mix_px(s: &Px, d: &Px, a: f32, ops: &Ops) -> Px {
    let f = |sv: f64, dv: f64| {
        // 元: a2 は深度のチャンネル型へ切り捨てキャスト。
        let prod = (dv as f32 * a) as f64;
        let a2 = if ops.float { prod } else { prod.trunc() };
        // 元の 8/16bit は整数式なので、**割り算のところだけ先に切り捨てる**。
        let sub = if ops.float {
            sv * a2
        } else {
            (sv * a2 / ops.max).trunc()
        };
        if ops.float {
            ops.round(sv + a2 - sub)
        } else {
            ops.round(sv + a2 - sub)
        }
    };
    Px {
        a: s.a,
        r: f(s.r, d.r),
        g: f(s.g, d.g),
        b: f(s.b, d.b),
    }
}

/// 元の pset。固定小数の位置へ、まわりの4画素にスクリーン合成で置く。
/// 1画素でも置けたら true。
fn pset(buf: &mut Buf, x_fx: i64, y_fx: i64, p: &Px, ops: &Ops) -> bool {
    let x0 = x_fx >> 16;
    let y0 = y_fx >> 16;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let (w, h) = (buf.w, buf.h);
    if x0 < -2 || y0 < -2 || x0 >= w || y0 >= h || x1 < 0 || y1 < 0 {
        return false;
    }
    // 元は PF_FpShort (f32)。
    let xd1 = (x_fx & 0xFFFF) as f32 / 65536.0;
    let yd1 = (y_fx & 0xFFFF) as f32 / 65536.0;
    let xd0 = 1.0 - xd1;
    let yd0 = 1.0 - yd1;
    let mut ret = false;
    let mut put = |bx: i64, by: i64, a: f32, buf: &mut Buf| {
        if bx < 0 || by < 0 || bx >= w || by >= h {
            return;
        }
        let i = (bx + by * w) as usize;
        let s = buf.px[i];
        buf.px[i] = mix_px(&s, p, a, ops);
        ret = true;
    };
    // 元は `(xd0 == 1) && (yd0 = 1)` という打ち間違いで、x が画素に
    // 乗っていれば y が小数でも1画素にしか置かない (直したところ 1)。
    let single = if FAITHFUL {
        xd0 == 1.0
    } else {
        xd0 == 1.0 && yd0 == 1.0
    };
    if single {
        put(x0, y0, 1.0, buf);
    } else {
        if x0 >= 0 && y0 >= 0 {
            put(x0, y0, xd0 * yd0, buf);
        }
        if x1 < w && y0 >= 0 {
            put(x1, y0, xd1 * yd0, buf);
        }
        if x1 < w && y1 < h {
            put(x1, y1, xd1 * yd1, buf);
        }
        if x0 >= 0 && y1 < h {
            put(x0, y1, xd0 * yd1, buf);
        }
    }
    ret
}

/// 元の calcColorTable / calcColorTableOnly。`i` 歩目の色。
fn calc_colour(
    i: i64,
    length: i64,
    strong: f32,
    col1: &Px,
    col2: &Px,
    border: f32,
    only: bool,
    ops: &Ops,
) -> Px {
    if length <= 0 || strong <= 0.0 || length <= i {
        return Px::default();
    }
    let op = strong as f64 * (length - i) as f64 / length as f64;
    if only {
        return Px {
            a: 0.0,
            r: ops.round(col1.r * op),
            g: ops.round(col1.g * op),
            b: ops.round(col1.b * op),
        };
    }
    // 元: A_long * PF_FpShort なので f32 で掛けてから整数へ。
    let bd = ((length as f32) * border) as i64;
    if bd <= 0 || i >= bd {
        Px {
            a: 0.0,
            r: ops.round(col2.r * op),
            g: ops.round(col2.g * op),
            b: ops.round(col2.b * op),
        }
    } else {
        let d2 = i as f64 / bd as f64;
        let d1 = 1.0 - d2;
        Px {
            a: 0.0,
            r: ops.round((col1.r * d1 + col2.r * d2) * op),
            g: ops.round((col1.g * d1 + col2.g * d2) * op),
            b: ops.round((col1.b * d1 + col2.b * d2) * op),
        }
    }
}

/// 元の DrawLine。中心から両方向へ、表の順に置く。
fn draw_line(buf: &mut Buf, x: i64, y: i64, angle: i64, length: i64, tbl: &[Px], ops: &Ops) {
    if length <= 0 {
        return;
    }
    let (cx, cy) = rot_vector_fixed(angle, length << 16);
    let dx = cx / length;
    let dy = cy / length;
    let wf = buf.w << 16;
    let hf = buf.h << 16;
    for sign in [1i64, -1] {
        let mut xf = x << 16;
        let mut yf = y << 16;
        for i in 0..length {
            if !pset(buf, xf, yf, &tbl[i as usize], ops) {
                break;
            }
            xf += dx * sign;
            yf += dy * sign;
            if sign > 0 {
                if xf >= wf || yf >= hf {
                    break;
                }
            } else if xf < -1 || yf < -1 {
                break;
            }
        }
    }
}

/// 元の MinMask の getMin。窓の最小値 (早回しつき)。
fn min_pass(line: &[f64], out: &mut [f64], value: i64) {
    let n = line.len() as i64;
    let mut max_index: i64 = -1;
    for t in 0..n {
        let start = (t - value).max(0);
        let end = (t + value).min(n - 1);
        if max_index >= start && max_index <= end {
            if line[end as usize] <= line[max_index as usize] {
                max_index = end;
                out[t as usize] = line[end as usize];
            } else {
                out[t as usize] = line[max_index as usize];
            }
            continue;
        }
        let mut idx = start;
        for i in start..=end {
            if line[i as usize] <= 0.0 {
                idx = i;
                break;
            }
            if line[i as usize] < line[idx as usize] {
                idx = i;
            }
        }
        max_index = idx;
        out[t as usize] = line[idx as usize];
    }
}

/// 元の MinMask。アルファだけに掛ける (横→縦)。
fn min_mask(buf: &mut Buf, value: i64) {
    let (w, h) = (buf.w, buf.h);
    let mut src: Vec<f64> = Vec::new();
    let mut dst: Vec<f64> = Vec::new();
    for y in 0..h {
        src.clear();
        for x in 0..w {
            src.push(buf.px[(x + y * w) as usize].a);
        }
        dst.clear();
        dst.resize(w as usize, 0.0);
        min_pass(&src, &mut dst, value);
        for x in 0..w {
            buf.px[(x + y * w) as usize].a = dst[x as usize];
        }
    }
    for x in 0..w {
        src.clear();
        for y in 0..h {
            src.push(buf.px[(x + y * w) as usize].a);
        }
        dst.clear();
        dst.resize(h as usize, 0.0);
        min_pass(&src, &mut dst, value);
        for y in 0..h {
            buf.px[(x + y * w) as usize].a = dst[y as usize];
        }
    }
}

/// 元の Exec の描くところをまとめたもの。
#[allow(clippy::too_many_arguments)]
fn exec(
    buf: &mut Buf,
    src: &[Px],
    info: &Info,
    col1: &Px,
    col2: &Px,
    min_value: i64,
    ops: &Ops,
) {
    let (w, h) = (buf.w, buf.h);
    // 2. TargetCopy。
    for (i, s) in src.iter().enumerate() {
        let mp = s.r.max(s.g).max(s.b);
        let v = ops.round(mp * s.a / ops.max);
        buf.px[i] = Px {
            a: v,
            r: v,
            g: v,
            b: v,
        };
    }
    // 3. MinMask (アルファだけ)。
    min_mask(buf, min_value);
    // 4. SubtrackAlpha。
    for p in buf.px.iter_mut() {
        p.a = ops.round(p.r - p.a);
    }
    // 5. RGB を決める。
    if info.kind == SK_SCREEN {
        for (i, s) in src.iter().enumerate() {
            let f = |v: f64| ops.round(v * s.a / ops.max);
            let a = buf.px[i].a;
            buf.px[i] = Px {
                a,
                r: f(s.r),
                g: f(s.g),
                b: f(s.b),
            };
        }
    } else {
        for p in buf.px.iter_mut() {
            // 元: 赤 (= TargetCopy の明るさ) を重みに使う。
            let t = p.r;
            let f = |v: f64| ops.round(v * t / ops.max);
            *p = Px {
                a: p.a,
                r: f(col1.r),
                g: f(col1.g),
                b: f(col1.b),
            };
        }
    }
    // 6. StarDraw。
    let only = info.kind != SK_A_TO_B && info.kind != 2;
    let mut tbl: Vec<Px> = vec![Px::default(); TABLE_SIZE];
    for y in 0..h {
        for x in 0..w {
            let base = buf.px[(x + y * w) as usize].a;
            if base <= 0.0 {
                continue;
            }
            // Screen のときは自分の色を両方に使う。
            let (c1, c2) = if info.kind == SK_SCREEN {
                let c = buf.px[(x + y * w) as usize];
                (c, c)
            } else {
                (*col1, *col2)
            };
            for l in info.line.iter() {
                let length = (l.length as f64 * base / ops.max) as i64;
                // 元: PF_FpShort の掛け算・割り算 (f32)。
                let strong = l.strong * (base as f32) / (ops.max as f32);
                if length <= 0 || strong <= 0.0 {
                    continue;
                }
                let length = length.min(TABLE_SIZE as i64);
                for i in 0..length {
                    tbl[i as usize] =
                        calc_colour(i, length, strong, &c1, &c2, info.border, only, ops);
                }
                draw_line(buf, x, y, l.angle, length, &tbl, ops);
            }
        }
    }
    // 7. AlphaMake。
    for p in buf.px.iter_mut() {
        p.a = p.r.max(p.g).max(p.b);
    }
    // 8. 乗算を戻す。
    for p in buf.px.iter_mut() {
        if p.a <= 0.0 {
            *p = Px::default();
        } else if p.a < ops.max {
            // AE の premultiply(0) は**四捨五入**する (ほかの段は切り捨て)。
            // 掛けてから割る (先に逆数を出すと合わない画素が増える)。
            let f = |x: f64| {
                if ops.float {
                    ops.round(x * ops.max / p.a)
                } else {
                    ops.round(x * ops.max / p.a + 0.5)
                }
            };
            let (r, g, b) = (f(p.r), f(p.g), f(p.b));
            p.r = r;
            p.g = g;
            p.b = b;
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    LengthPar,
    StrongPar,
    Rot,
    ColorTopic,
    ColorKind,
    ColorA,
    ColorB,
    ColorBorder,
    ColorTopicEnd,
    L1Topic,
    L1Length,
    L1Strong,
    L1Rot,
    L1TopicEnd,
    L2Topic,
    L2Length,
    L2Strong,
    L2Rot,
    L2TopicEnd,
    L3Topic,
    L3Length,
    L3Strong,
    L3Rot,
    L3TopicEnd,
    L4Topic,
    L4Length,
    L4Strong,
    L4Rot,
    L4TopicEnd,
}

/// 線1本ぶんのスロット (topic, length, strong, rot, topic_end)。
const LINES: [(Params, Params, Params, Params, Params); 4] = [
    (
        Params::L1Topic,
        Params::L1Length,
        Params::L1Strong,
        Params::L1Rot,
        Params::L1TopicEnd,
    ),
    (
        Params::L2Topic,
        Params::L2Length,
        Params::L2Strong,
        Params::L2Rot,
        Params::L2TopicEnd,
    ),
    (
        Params::L3Topic,
        Params::L3Length,
        Params::L3Strong,
        Params::L3Rot,
        Params::L3TopicEnd,
    ),
    (
        Params::L4Topic,
        Params::L4Length,
        Params::L4Strong,
        Params::L4Rot,
        Params::L4TopicEnd,
    ),
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
        // 元: PF_ADD_FIXED("Length(%)", 0, 500, 0, 100, 100)
        params.add(
            Params::LengthPar,
            "Length(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(500.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("Strong(%)", 0, 200, 0, 100, 50)
        params.add(
            Params::StrongPar,
            "Strong(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(200.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_ANGLE("Rot", 0)
        params.add(
            Params::Rot,
            "Rot",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        params.add_group(
            Params::ColorTopic,
            Params::ColorTopicEnd,
            "Color",
            true,
            |p| {
                // 元: PF_ADD_POPUP("Kind", 5, 1, "A To B|B To A|A Only|B Only|Screen")
                p.add(
                    Params::ColorKind,
                    "Kind",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["A To B", "B To A", "A Only", "B Only", "Screen"]);
                        f.set_default(1);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_COLOR("Color A", 0xF7, 0xFF, 0xD2)
                p.add(
                    Params::ColorA,
                    "Color A",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 0xFF,
                            red: 0xF7,
                            green: 0xFF,
                            blue: 0xD2,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_COLOR("Color B", 0xF0, 0xFF, 0x05)
                p.add(
                    Params::ColorB,
                    "Color B",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 0xFF,
                            red: 0xF0,
                            green: 0xFF,
                            blue: 0x05,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_FIXED("Color Border(%)", 0, 100, 0, 100, 30)
                p.add(
                    Params::ColorBorder,
                    "Color Border(%)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_precision(1);
                        f.set_curve_tolerance(0.05);
                        f.set_default(30.0);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            },
        )?;
        // 線4本。既定の長さは 64/40/64/40、角度は 0/45/90/135。
        let lengths = [64, 40, 64, 40];
        let angles = [0.0f32, 45.0, 90.0, 135.0];
        for (i, g) in LINES.iter().enumerate() {
            params.add_group(g.0, g.4, &format!("Line{}", i + 1), true, |p| {
                let len = lengths[i];
                p.add(
                    g.1,
                    &format!("Line{} Length", i + 1),
                    ae::SliderDef::setup(move |f| {
                        f.set_valid_min(0);
                        f.set_valid_max(500);
                        f.set_slider_min(0);
                        f.set_slider_max(100);
                        f.set_default(len);
                        f.set_value(f.default());
                    }),
                )?;
                p.add(
                    g.2,
                    &format!("Line{} Strong", i + 1),
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
                let ang = angles[i];
                p.add(
                    g.3,
                    &format!("Line{} Rot", i + 1),
                    ae::AngleDef::setup(move |f| {
                        f.set_default(ang);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            })?;
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
                out_data.set_return_msg("fs-rs star");
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
        // 元の GetFIXED2FpShort_P1: 固定小数を /6553600 して f32。
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f32, Error> {
            let fx = libs::utils::flt2fix(
                params.get(p)?.as_float_slider()?.value() as f64
            );
            // 元: (PF_FpShort)固定小数 / 6553600 で、**f32 の割り算**。
            Ok((fx as f32) / 6553600.0)
        };
        let ang = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(libs::utils::flt2fix(params.get(p)?.as_angle()?.value() as f64) as i64)
        };
        // 元の GetParams。
        let all_length = pct(Params::LengthPar, params)?;
        let all_strong = pct(Params::StrongPar, params)?;
        let all_angle = ang(Params::Rot, params)?;
        let kind = params.get(Params::ColorKind)?.as_popup()?.value() as i64;
        let a = params.get(Params::ColorA)?.as_color()?.value();
        let b = params.get(Params::ColorB)?.as_color()?.value();
        // 元: 種類で col1 / col2 の取り方が変わる。
        let (col1, col2) = match kind {
            2 => (b, a),
            3 => (a, a),
            4 => (b, b),
            _ => (a, b),
        };
        let border = pct(Params::ColorBorder, params)?;
        let mut line = [Line::default(); 4];
        for (i, g) in LINES.iter().enumerate() {
            let len_raw = params.get(g.1)?.as_slider()?.value() as i64;
            // 元: downScaleNoClip (整数の割り算) → Length(%) を掛けて整数へ。
            let len = len_raw * ds.num as i64 / ds.den as i64;
            let mut len = (len as f32 * all_length) as i64;
            if len < 0 {
                len = 0;
            } else if len > 2500 {
                len = 2500;
            }
            let strong = (pct(g.2, params)? * all_strong).max(0.0);
            let mut angle = ang(g.3, params)? + all_angle;
            angle %= 360 << 16;
            if angle < 0 {
                angle += 360 << 16;
            }
            if angle >= (180 << 16) {
                angle -= 180 << 16;
            }
            line[i] = Line {
                length: len,
                strong,
                angle,
            };
        }
        let info = Info {
            kind,
            col1,
            col2,
            border,
            line,
        };
        // 元: MinValue 5 に downScaleNoClip。
        let min_value = 5 * ds.num as i64 / ds.den as i64;

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();
        if w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $conv:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                let mut src: Vec<Px> = Vec::with_capacity((w * h) as usize);
                for y in 0..h {
                    for x in 0..w {
                        src.push($get(in_layer.$rd(x as usize, y as usize)));
                    }
                }
                let mut buf = Buf {
                    w,
                    h,
                    px: vec![Px::default(); (w * h) as usize],
                };
                exec(
                    &mut buf,
                    &src,
                    &info,
                    &$conv(&info.col1),
                    &$conv(&info.col2),
                    min_value,
                    &ops,
                );
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
                as_pixel8,
                as_pixel8_mut,
                Ops {
                    max: 255.0,
                    float: false
                },
                |c: &ae::Pixel8| Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64
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
                |c: &ae::Pixel8| {
                    let p = libs::utils::conv_8_to_16(c);
                    Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64,
                    }
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
                |c: &ae::Pixel8| {
                    let p = libs::utils::conv_8_to_32(c);
                    Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64,
                    }
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

    fn white() -> Px {
        Px {
            a: 255.0,
            r: 255.0,
            g: 255.0,
            b: 255.0,
        }
    }

    /// 角度 0 は上向き、90 は右向き (固定小数)。
    #[test]
    fn the_angle_points_the_right_way() {
        let (x, y) = rot_vector_fixed(0, 10 << 16);
        assert_eq!(x, 0);
        assert!((y + (10 << 16)).abs() < 2, "{y}");
        let (x, y) = rot_vector_fixed(90 << 16, 10 << 16);
        assert!((x - (10 << 16)).abs() < 2, "{x}");
        assert_eq!(y, 0);
    }

    /// 色の表は先へ行くほど薄くなる。
    #[test]
    fn the_table_fades_towards_the_tip() {
        let o = ops8();
        let c1 = white();
        let c2 = white();
        let v: Vec<f64> = (0..10)
            .map(|i| calc_colour(i, 10, 1.0, &c1, &c2, 0.3, true, &o).r)
            .collect();
        assert_eq!(v[0], 255.0);
        for i in 1..10 {
            assert!(v[i] <= v[i - 1], "i = {i}: {v:?}");
        }
        assert_eq!(v[9], 25.0);
    }

    /// 境目までは A から B へ混ざる。
    #[test]
    fn the_border_mixes_the_two_colours() {
        let o = ops8();
        let a = Px {
            a: 0.0,
            r: 255.0,
            g: 0.0,
            b: 0.0,
        };
        let b = Px {
            a: 0.0,
            r: 0.0,
            g: 255.0,
            b: 0.0,
        };
        // 長さ 100、境目 50%。i=0 は A、i=60 は B。
        let p0 = calc_colour(0, 100, 1.0, &a, &b, 0.5, false, &o);
        assert!(p0.r > p0.g);
        let p60 = calc_colour(60, 100, 1.0, &a, &b, 0.5, false, &o);
        assert!(p60.g > p60.r);
    }

    /// 最小値フィルタは明るい点を削る。
    #[test]
    fn the_min_pass_eats_a_dot() {
        let line: Vec<f64> = (0..11).map(|i| if i == 5 { 255.0 } else { 0.0 }).collect();
        let mut out = vec![0.0; 11];
        min_pass(&line, &mut out, 2);
        assert!(out.iter().all(|v| *v == 0.0));
    }

    /// 広い明るい帯は縁だけ削られる。
    #[test]
    fn the_min_pass_keeps_the_middle_of_a_band() {
        let line: Vec<f64> = (0..21)
            .map(|i| if (5..16).contains(&i) { 255.0 } else { 0.0 })
            .collect();
        let mut out = vec![0.0; 21];
        min_pass(&line, &mut out, 2);
        assert_eq!(out[10], 255.0);
        assert_eq!(out[5], 0.0);
        assert_eq!(out[7], 255.0);
    }

    /// 小数の位置は4画素に分かれて置かれる (直したところ 1)。
    #[test]
    fn a_fractional_position_spreads_over_four() {
        let o = ops8();
        let mut b = Buf {
            w: 4,
            h: 4,
            px: vec![Px::default(); 16],
        };
        // x はちょうど 1、y は 1.5。
        assert!(pset(&mut b, 1 << 16, (1 << 16) + 32768, &white(), &o));
        assert!(b.px[1 + 1 * 4].r > 0.0, "上の行に置かれていない");
        assert!(b.px[1 + 2 * 4].r > 0.0, "下の行に置かれていない");
    }

    /// 画面の外には置かない。
    #[test]
    fn a_position_outside_is_refused() {
        let o = ops8();
        let mut b = Buf {
            w: 4,
            h: 4,
            px: vec![Px::default(); 16],
        };
        assert!(!pset(&mut b, 10 << 16, 1 << 16, &white(), &o));
        assert!(!pset(&mut b, -10 << 16, 1 << 16, &white(), &o));
        assert!(b.px.iter().all(|p| *p == Px::default()));
    }

    /// 線は中心から両方向へ伸びる。
    #[test]
    fn the_line_runs_both_ways() {
        let o = ops8();
        let mut b = Buf {
            w: 32,
            h: 1,
            px: vec![Px::default(); 32],
        };
        let tbl: Vec<Px> = (0..10)
            .map(|i| calc_colour(i, 10, 1.0, &white(), &white(), 0.3, true, &o))
            .collect();
        // 角度 90 = 右向き。
        draw_line(&mut b, 16, 0, 90 << 16, 10, &tbl, &o);
        assert!(b.px[20].r > 0.0, "右へ伸びていない");
        assert!(b.px[12].r > 0.0, "左へ伸びていない");
        assert_eq!(b.px[0].r, 0.0);
    }

    /// 一通り通しても落ちない。
    #[test]
    fn the_whole_pass_runs() {
        let o = ops8();
        let mut src = vec![Px::default(); 64 * 48];
        // 真ん中に明るい四角。
        for y in 20..28 {
            for x in 28..36 {
                src[x + y * 64] = white();
            }
        }
        let mut b = Buf {
            w: 64,
            h: 48,
            px: vec![Px::default(); 64 * 48],
        };
        let info = Info {
            kind: SK_A_TO_B,
            col1: ae::Pixel8 {
                alpha: 255,
                red: 247,
                green: 255,
                blue: 210,
            },
            col2: ae::Pixel8 {
                alpha: 255,
                red: 240,
                green: 255,
                blue: 5,
            },
            border: 0.3,
            line: [
                Line {
                    length: 16,
                    strong: 0.5,
                    angle: 0,
                },
                Line {
                    length: 10,
                    strong: 0.5,
                    angle: 45 << 16,
                },
                Line {
                    length: 16,
                    strong: 0.5,
                    angle: 90 << 16,
                },
                Line {
                    length: 10,
                    strong: 0.5,
                    angle: 135 << 16,
                },
            ],
        };
        let c1 = Px {
            a: 255.0,
            r: 247.0,
            g: 255.0,
            b: 210.0,
        };
        let c2 = Px {
            a: 255.0,
            r: 240.0,
            g: 255.0,
            b: 5.0,
        };
        exec(&mut b, &src, &info, &c1, &c2, 5, &o);
        let drawn = b.px.iter().filter(|p| p.a > 0.0).count();
        assert!(drawn > 50, "drawn = {drawn}");
    }

    #[test]
    fn the_fix_is_on() {
        assert!(!FAITHFUL);
    }
}
