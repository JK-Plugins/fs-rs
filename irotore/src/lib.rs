//! F's IroTore の移植。
//!
//! セル画の**主線を消して、まわりの塗りで埋める**。そのうえで主線だけを
//! 取り出し、太らせ・ぼかし・色変えをしてから戻す。手描きの主線を
//! 「色トレス線」に仕立てるための道具。
//!
//! 手順 (元の irotoreExec):
//!
//! 1. 入力をそのままコピーする。主線の色が1つも無ければ終わり
//! 2. 入力の色を見てアルファに印をつける: 主線色なら `MAX`、`NegColor`
//!    なら 0、透明か真っ白なら 0、それ以外 (塗り) は `HALF`
//! 3. 主線の画素が無くなるまで埋める
//!    - `targetRemove`: 上下左右の塗りのうち**いちばん暗いもの**で埋める
//!    - 1画素も埋まらなかったら斜め4方向で同じことをする
//!    - それでも埋まらなかったら残りを `GiveUpColor` で塗る
//! 4. ぼかし・色変え・`Min/Max` のどれかが入っていれば、入力の色から
//!    主線をもう一度拾い直す (印を `MAX` に戻す)
//! 5. `Min/Max1`・`Min/Max2`: 主線のところだけ、主線が続くあいだを見て
//!    いちばん明るい (正) か暗い (負) 画素で置き換える。横→縦の2回
//! 6. `LineBlur`: 主線のところだけ平均する。横→縦→横 (3回目は半分の幅)
//! 7. `LineH`/`LineS`/`LineL`/`LineRed`/`LineGreen`/`LineBlue`: 主線の
//!    色を HLS と RGB で動かす
//! 8. `LineOnly` が入っていれば主線だけ残す。でなければアルファを
//!    入力のものに戻す
//!
//! 元から写した細かいところ:
//!
//! - 色の近さは RGB のチャンネルごとの差を `level` (= `255 × % / 100`
//!   を四捨五入した整数) と比べる
//! - 明るさは `0.29891R + 0.58661G + 0.11448B + 0.5` を切り捨てた整数
//! - 埋めるとき・太らせるとき・ぼかすときは、どれも「1行 (1列) ぶんの
//!   写し」から読むので、同じ回の書き込みは見えない。縁は押さえる
//! - 埋める向きの順は上・右・下・左。同じ明るさなら先に見たほうが残る
//!   (`v > v2` で厳しく比べる)
//! - `GiveUpColor` のアルファは色見本のアルファ (ふつう 255) なので、
//!   塗ったあとも印は `MAX` のまま残る。数は減るのでそこで止まる
//! - ぼかしは `(合計 / 個数) + 0.5` を切り捨てる。3回目の幅は
//!   `LineBlur / 2` で、0 になったら掛けない
//! - 色変えは 8bit → 32bit (小数4桁に量子化) → HLS → 32bit → 8bit。
//!   足すぶんはいちど **f32 に落として**から足す (元の `(PF_FpShort)`)
//! - 色相は正のときだけ効く (角度を 0..1 に収めるので負にならない)
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **32bit では何もしていなかった**。`Exec` の分岐が
//!    `case PF_PixelFormat_ARGB128: break;` で、`IroToreSub32.cpp` は
//!    書かれているのに一度も呼ばれていない。8/16bit と同じ道を通すように
//!    した (IroToreAdjacent と同じ直し)
//! 2. **パラメータ名「あきらめ色」が非 ASCII** で、AE 上では空の名前に
//!    なる。`GiveUpColor` にした
//!
//! 直っているように見えて**結果は変わらないところ**:
//!
//! - `Color5`/`Color6` だけ重複を省く `FindColor` を通らない。主線色の
//!   表は全件を順に比べるだけなので、重複が入っても絵は変わらない
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/IroTore

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 1画素。深さに依らず生の目盛りで持つ。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深さごとの上限と丸め。
#[derive(Clone, Copy, Debug)]
pub struct Ops {
    pub max: f64,
    pub float: bool,
}

impl Ops {
    fn half(&self) -> f64 {
        if self.float {
            0.5
        } else if self.max > 255.0 {
            16384.0
        } else {
            128.0
        }
    }

    /// 元の RoundByteFpLong 相当 (整数の深さは切り捨て)。
    fn clip(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 1.0)
        } else {
            v.clamp(0.0, self.max).trunc()
        }
    }

    /// 画素を 8bit の目盛りに直す (色見本との比較に使う)。
    fn to8(&self, p: &Px) -> (i64, i64, i64) {
        if self.float {
            let q = libs::utils::conv_32_to_8(&ae::sys::PF_PixelFloat {
                alpha: p.a as f32,
                red: p.r as f32,
                green: p.g as f32,
                blue: p.b as f32,
            });
            (q.red as i64, q.green as i64, q.blue as i64)
        } else if self.max > 255.0 {
            let q = libs::utils::conv_16_to_8(&ae::sys::PF_Pixel16 {
                alpha: p.a as u16,
                red: p.r as u16,
                green: p.g as u16,
                blue: p.b as u16,
            });
            (q.red as i64, q.green as i64, q.blue as i64)
        } else {
            (p.r as i64, p.g as i64, p.b as i64)
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

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Clone, Debug)]
pub struct Info {
    pub line_only: bool,
    /// 主線の色 (8bit の目盛り)。
    pub colour: Vec<(i64, i64, i64)>,
    /// 主線にしない色。
    pub ng_colour: Vec<(i64, i64, i64)>,
    pub level: i64,
    /// 埋めきれなかったところを塗る色 (その深さの目盛り)。
    pub ig_colour: Px,
    pub blur: i64,
    pub minmax1: i64,
    pub minmax2: i64,
    pub hue: f64,
    pub sat: f64,
    pub light: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

/// 元の pixelValue。明るさ (整数)。
fn pixel_value(p: &Px) -> i64 {
    (0.29891 * p.r + 0.58661 * p.g + 0.11448 * p.b + 0.5) as i64
}

/// 元の isTarget / isNeg。色見本のどれかに近いか。
fn matches(list: &[(i64, i64, i64)], p: &Px, level: i64, ops: &Ops) -> bool {
    if list.is_empty() {
        return false;
    }
    let (r, g, b) = ops.to8(p);
    list.iter()
        .any(|c| (c.0 - r).abs() <= level && (c.1 - g).abs() <= level && (c.2 - b).abs() <= level)
}

/// 元の targetSelect。印をアルファに入れる。
pub fn target_select(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) -> i64 {
    let mut count = 0;
    for i in 0..out.px.len() {
        let p = inp.px[i];
        out.px[i].a = if matches(&info.colour, &p, info.level, ops) {
            count += 1;
            ops.max
        } else if matches(&info.ng_colour, &p, info.level, ops) {
            0.0
        } else if p.a == 0.0 || (p.r >= ops.max && p.g >= ops.max && p.b >= ops.max) {
            0.0
        } else {
            ops.half()
        };
    }
    count
}

/// 元の targetLineOnly。主線だけ印を立てる。
pub fn line_only(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    for i in 0..out.px.len() {
        out.px[i].a = if matches(&info.colour, &inp.px[i], info.level, ops) {
            ops.max
        } else {
            0.0
        };
    }
}

/// 元の targetRemove / targetRemove2nd。塗りで主線を埋める。
/// `diag` が立っていれば斜め4方向を見る。残っている主線の数を返す。
pub fn target_remove(out: &mut Buf, count: i64, diag: bool, ops: &Ops) -> i64 {
    let hf = ops.half();
    // 元は3行ぶんの写しから読むので、この回の書き込みは見えない。
    let src: Vec<Px> = out.px.clone();
    let (bw, bh) = (out.w, out.h);
    let get = |x: i64, y: i64| -> Px {
        let xx = x.clamp(0, bw - 1);
        let yy = y.clamp(0, bh - 1);
        src[(xx + yy * bw) as usize]
    };
    let dirs: [(i64, i64); 4] = if diag {
        [(-1, -1), (1, -1), (-1, 1), (1, 1)]
    } else {
        [(0, -1), (1, 0), (0, 1), (-1, 0)]
    };
    let mut left = count;
    for y in 0..out.h {
        for x in 0..out.w {
            if out.at(x, y).a != ops.max {
                continue;
            }
            let mut v = if ops.float { 1.0 } else { ops.max } as i64;
            if ops.float {
                v = 1;
            }
            let mut best: Option<Px> = None;
            // 元の CHK_PX は v が 0 より大きいあいだだけ見る。
            for (dx, dy) in dirs {
                if v <= 0 {
                    break;
                }
                let s = get(x + dx, y + dy);
                if s.a != hf {
                    continue;
                }
                let v2 = pixel_value(&s);
                if v > v2 {
                    v = v2;
                    best = Some(s);
                }
            }
            if let Some(mut p) = best {
                p.a = hf;
                out.set(x, y, p);
                left -= 1;
                if left <= 0 {
                    return left;
                }
            }
        }
    }
    left
}

/// 元の targetRemoveMax。残りを `GiveUpColor` で塗る。
pub fn target_remove_max(out: &mut Buf, count: i64, info: &Info, ops: &Ops) -> i64 {
    let mut left = count;
    for y in 0..out.h {
        for x in 0..out.w {
            if out.at(x, y).a != ops.max {
                continue;
            }
            out.set(x, y, info.ig_colour);
            left -= 1;
            if left <= 0 {
                return left;
            }
        }
    }
    left
}

/// 元の targetMinMax。主線のところだけ、続いているあいだの最大 (正) か
/// 最小 (負) で置き換える。横のあと縦。
pub fn min_max(out: &mut Buf, value: i64, ops: &Ops) {
    if value == 0 {
        return;
    }
    let max_f = value > 0;
    let n = value.abs();
    // 横。
    for y in 0..out.h {
        let row: Vec<Px> = (0..out.w).map(|x| out.at(x, y)).collect();
        for x in 0..out.w {
            if out.at(x, y).a != ops.max {
                continue;
            }
            let mut target = out.at(x, y);
            let mut v = pixel_value(&target);
            let mut ok = false;
            for side in [-1i64, 1] {
                for i in 1..=n {
                    let xx = x + side * i;
                    if xx < 0 || xx >= out.w {
                        break;
                    }
                    let p = row[xx as usize];
                    if p.a != ops.max {
                        break;
                    }
                    let v2 = pixel_value(&p);
                    if (max_f && v < v2) || (!max_f && v > v2) {
                        v = v2;
                        target = p;
                        ok = true;
                    }
                }
            }
            if ok {
                out.set(x, y, target);
            }
        }
    }
    // 縦。
    for x in 0..out.w {
        let col: Vec<Px> = (0..out.h).map(|y| out.at(x, y)).collect();
        for y in 0..out.h {
            if out.at(x, y).a != ops.max {
                continue;
            }
            let mut target = out.at(x, y);
            let mut v = pixel_value(&target);
            let mut ok = false;
            for side in [-1i64, 1] {
                for i in 1..=n {
                    let yy = y + side * i;
                    if yy < 0 || yy >= out.h {
                        break;
                    }
                    let p = col[yy as usize];
                    if p.a != ops.max {
                        break;
                    }
                    let v2 = pixel_value(&p);
                    if (max_f && v < v2) || (!max_f && v > v2) {
                        v = v2;
                        target = p;
                        ok = true;
                    }
                }
            }
            if ok {
                out.set(x, y, target);
            }
        }
    }
}

/// 元の targetBlur の1回ぶん。主線が続くあいだだけ平均する。
fn blur_pass(out: &mut Buf, v: i64, vert: bool, ops: &Ops) {
    let outer = if vert { out.w } else { out.h };
    let inner = if vert { out.h } else { out.w };
    for o in 0..outer {
        let line: Vec<Px> = (0..inner)
            .map(|i| if vert { out.at(o, i) } else { out.at(i, o) })
            .collect();
        for i in 0..inner {
            let (x, y) = if vert { (o, i) } else { (i, o) };
            if out.at(x, y).a != ops.max {
                continue;
            }
            let c = line[i as usize];
            let mut r = c.r;
            let mut g = c.g;
            let mut b = c.b;
            let mut cnt = 1i64;
            for side in [-1i64, 1] {
                for k in 1..=v {
                    let j = i + side * k;
                    if j < 0 || j >= inner {
                        break;
                    }
                    let p = line[j as usize];
                    if p.a != ops.max {
                        break;
                    }
                    r += p.r;
                    g += p.g;
                    b += p.b;
                    cnt += 1;
                }
            }
            if cnt > 1 {
                let f = cnt as f64;
                out.set(
                    x,
                    y,
                    Px {
                        a: ops.max,
                        r: ops.clip(r / f + 0.5),
                        g: ops.clip(g / f + 0.5),
                        b: ops.clip(b / f + 0.5),
                    },
                );
            }
        }
    }
}

/// 元の targetBlur。横→縦→横 (3回目は半分の幅)。
pub fn blur(out: &mut Buf, v: i64, ops: &Ops) {
    if v <= 0 {
        return;
    }
    blur_pass(out, v, false, ops);
    blur_pass(out, v, true, ops);
    let v2 = v / 2;
    if v2 > 0 {
        blur_pass(out, v2, false, ops);
    }
}

/// 元の targetColor。主線の色を動かす。
pub fn target_colour(out: &mut Buf, info: &Info, ops: &Ops) {
    for i in 0..out.px.len() {
        if out.px[i].a != ops.max {
            continue;
        }
        // 元は 8bit に落としてから 32bit に上げる。
        let p8 = match (ops.float, ops.max > 255.0) {
            (true, _) => libs::utils::conv_32_to_8(&ae::sys::PF_PixelFloat {
                alpha: out.px[i].a as f32,
                red: out.px[i].r as f32,
                green: out.px[i].g as f32,
                blue: out.px[i].b as f32,
            }),
            (false, true) => libs::utils::conv_16_to_8(&ae::sys::PF_Pixel16 {
                alpha: out.px[i].a as u16,
                red: out.px[i].r as u16,
                green: out.px[i].g as u16,
                blue: out.px[i].b as u16,
            }),
            (false, false) => ae::sys::PF_Pixel {
                alpha: out.px[i].a as u8,
                red: out.px[i].r as u8,
                green: out.px[i].g as u8,
                blue: out.px[i].b as u8,
            },
        };
        let f = libs::utils::conv_8_to_32(&p8);
        let mut hls = libs::hls::HLSA::from_rgb(
            f.red as f64,
            f.green as f64,
            f.blue as f64,
            f.alpha as f64,
        );
        if info.hue > 0.0 {
            hls.hue += info.hue;
            if hls.hue > 1.0 {
                hls.hue -= 1.0;
            } else if hls.hue < 0.0 {
                hls.hue += 1.0;
            }
        }
        if info.sat > 0.0 {
            hls.saturation += (1.0 - hls.saturation) * info.sat;
        } else if info.sat < 0.0 {
            hls.saturation += hls.saturation * info.sat;
        }
        // 元は足すぶんだけ f32 に落とす。
        if info.light > 0.0 {
            hls.lightness += ((1.0 - hls.lightness) * info.light) as f32 as f64;
        } else if info.light < 0.0 {
            hls.lightness += (hls.lightness * info.light) as f32 as f64;
        }
        let (r, g, b, _) = hls.to_rgb();
        // 元の PF_PixelFloat は f32。
        let mut r = r as f32;
        let mut g = g as f32;
        let mut b = b as f32;
        for (v, k) in [
            (&mut r, info.red),
            (&mut g, info.green),
            (&mut b, info.blue),
        ] {
            if k > 0.0 {
                *v += ((1.0 - *v as f64) * k) as f32;
            } else if k < 0.0 {
                *v += (*v as f64 * k) as f32;
            }
        }
        let q = libs::utils::conv_32_to_8(&ae::sys::PF_PixelFloat {
            alpha: 1.0,
            red: r,
            green: g,
            blue: b,
        });
        // その深さへ戻す。
        out.px[i] = match (ops.float, ops.max > 255.0) {
            (true, _) => {
                let c = libs::utils::conv_8_to_32(&q);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
            (false, true) => {
                let c = libs::utils::conv_8_to_16(&q);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
            (false, false) => Px {
                a: q.alpha as f64,
                r: q.red as f64,
                g: q.green as f64,
                b: q.blue as f64,
            },
        };
    }
}

/// 元の irotoreExec。手順ぜんぶ。
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    out.px.copy_from_slice(&inp.px);
    if info.colour.is_empty() {
        return;
    }
    let mut count = target_select(inp, out, info, ops);
    if count <= 0 {
        if info.line_only {
            out.px.iter_mut().for_each(|p| *p = Px::default());
        } else {
            out.px.copy_from_slice(&inp.px);
        }
        return;
    }
    // 主線が無くなるまで埋める。
    while count > 0 {
        let before = count;
        count = target_remove(out, count, false, ops);
        if count == before {
            count = target_remove(out, count, true, ops);
            if count == before {
                count = target_remove_max(out, count, info, ops);
            }
        }
    }
    // 主線を拾い直す。
    let mut lo_flag = true;
    if info.blur > 0
        || info.hue != 0.0
        || info.sat != 0.0
        || info.light != 0.0
        || info.red != 0.0
        || info.green != 0.0
        || info.blue != 0.0
        || info.minmax1 != 0
        || info.minmax2 != 0
    {
        line_only(inp, out, info, ops);
        lo_flag = false;
    }
    if info.minmax1 != 0 {
        min_max(out, info.minmax1, ops);
    }
    if info.minmax2 != 0 {
        min_max(out, info.minmax2, ops);
    }
    if info.blur > 0 {
        blur(out, info.blur, ops);
    }
    if info.hue != 0.0
        || info.sat != 0.0
        || info.light != 0.0
        || info.red != 0.0
        || info.green != 0.0
        || info.blue != 0.0
    {
        target_colour(out, info, ops);
    }
    if info.line_only {
        if lo_flag {
            line_only(inp, out, info, ops);
        }
    } else {
        for i in 0..out.px.len() {
            out.px[i].a = inp.px[i].a;
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    LineOnly,
    Color1On,
    Color1,
    Color2On,
    Color2,
    Color3On,
    Color3,
    Color4On,
    Color4,
    Color5On,
    Color5,
    Color6On,
    Color6,
    Level,
    IgColor,
    MinMax1,
    MinMax2,
    LineBlur,
    LineH,
    LineS,
    LineL,
    LineRed,
    LineGreen,
    LineBlue,
    Neg1On,
    Neg1,
    Neg2On,
    Neg2,
    Neg3On,
    Neg3,
    Neg4On,
    Neg4,
}

/// 主線の色6つ (チェックと色)。
const COLOURS: [(Params, Params); 6] = [
    (Params::Color1On, Params::Color1),
    (Params::Color2On, Params::Color2),
    (Params::Color3On, Params::Color3),
    (Params::Color4On, Params::Color4),
    (Params::Color5On, Params::Color5),
    (Params::Color6On, Params::Color6),
];

/// 主線にしない色4つ。
const NEGS: [(Params, Params); 4] = [
    (Params::Neg1On, Params::Neg1),
    (Params::Neg2On, Params::Neg2),
    (Params::Neg3On, Params::Neg3),
    (Params::Neg4On, Params::Neg4),
];

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// 元の PF_ADD_FLOAT_SLIDER (精度1桁)。
fn fslider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    vmin: f32,
    vmax: f32,
    smin: f32,
    smax: f32,
    dflt: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(vmin);
            f.set_valid_max(vmax);
            f.set_slider_min(smin);
            f.set_slider_max(smax);
            f.set_precision(1);
            f.set_curve_tolerance(0.05);
            f.set_default(dflt);
            f.set_value(f.default());
        }),
    )
}

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
        // 元: PF_ADD_CHECKBOX("LineOnly", "on", FALSE)
        params.add(
            Params::LineOnly,
            "LineOnly",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        let on = [true, true, true, false, false, false];
        let cols = [0x0Fu8, 0x0A, 0x05, 0x00, 0x00, 0x00];
        for (i, (on_id, col_id)) in COLOURS.iter().enumerate() {
            let o = on[i];
            params.add(
                *on_id,
                &format!("EnabledColor{}", i + 1),
                ae::CheckBoxDef::setup(move |f| {
                    f.set_label("on");
                    f.set_default(o);
                    f.set_value(f.default());
                }),
            )?;
            let c = cols[i];
            params.add(
                *col_id,
                &format!("Color{}", i + 1),
                ae::ColorDef::setup(move |f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: c,
                        green: c,
                        blue: c,
                    });
                    f.set_value(f.default());
                }),
            )?;
        }
        fslider(params, Params::Level, "level", 0.0, 100.0, 0.0, 10.0, 0.0)?;
        // 元のパラメータ名は「あきらめ色」(非 ASCII)。英語にしてある。
        params.add(
            Params::IgColor,
            "GiveUpColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0x10,
                    green: 0x10,
                    blue: 0x10,
                });
                f.set_value(f.default());
            }),
        )?;
        for (id, name, dflt) in [
            (Params::MinMax1, "Min/Max1", -2),
            (Params::MinMax2, "Min/Max2", 0),
        ] {
            params.add(
                id,
                name,
                ae::SliderDef::setup(move |f| {
                    f.set_valid_min(-50);
                    f.set_valid_max(50);
                    f.set_slider_min(-10);
                    f.set_slider_max(10);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_SLIDER("LineBlur", 0, 50, 0, 10, 4)
        params.add(
            Params::LineBlur,
            "LineBlur",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(50);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(4);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_ANGLE("LineH", 0)
        params.add(
            Params::LineH,
            "LineH",
            ae::AngleDef::setup(|f| {
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        for (id, name, dflt) in [
            (Params::LineS, "LineS", 0.0f64),
            (Params::LineL, "LineL", -50.0),
            (Params::LineRed, "LineRed", 0.0),
            (Params::LineGreen, "LineGreen", 0.0),
            (Params::LineBlue, "LineBlue", 0.0),
        ] {
            fslider(params, id, name, -100.0, 100.0, -100.0, 100.0, dflt)?;
        }
        let negs = [0xFEu8, 0xFD, 0xFC, 0xFC];
        for (i, (on_id, col_id)) in NEGS.iter().enumerate() {
            params.add(
                *on_id,
                &format!("EnabledN{}", i + 1),
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(false);
                    f.set_value(f.default());
                }),
            )?;
            let c = negs[i];
            params.add(
                *col_id,
                &format!("NegColor{}", i + 1),
                ae::ColorDef::setup(move |f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: c,
                        green: c,
                        blue: c,
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
                out_data.set_return_msg("fs-rs irotore");
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
        _in_data: &ae::InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let depth = out_layer.bit_depth();
        if FAITHFUL && depth > 16 {
            // 元は 32bit で何もしない。
            return Ok(());
        }
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };

        // 元の GetParams。色見本は 8bit のまま持つ。
        let line_only = params.get(Params::LineOnly)?.as_checkbox()?.value();
        let mut colour: Vec<(i64, i64, i64)> = Vec::new();
        for (i, (on_id, col_id)) in COLOURS.iter().enumerate() {
            if !params.get(*on_id)?.as_checkbox()?.value() {
                continue;
            }
            let c = params.get(*col_id)?.as_color()?.value();
            let t = (c.red as i64, c.green as i64, c.blue as i64);
            // 元は Color5/Color6 だけ重複を省かない。
            if i < 4 && colour.contains(&t) {
                continue;
            }
            colour.push(t);
        }
        let mut ng_colour: Vec<(i64, i64, i64)> = Vec::new();
        for (i, (on_id, col_id)) in NEGS.iter().enumerate() {
            if !params.get(*on_id)?.as_checkbox()?.value() {
                continue;
            }
            let c = params.get(*col_id)?.as_color()?.value();
            let t = (c.red as i64, c.green as i64, c.blue as i64);
            if colour.contains(&t) {
                continue;
            }
            // 元は1つめだけ、ほかの NegColor との重複を見ない。
            if i > 0 && ng_colour.contains(&t) {
                continue;
            }
            ng_colour.push(t);
        }
        let level = {
            let lv = params.get(Params::Level)?.as_float_slider()?.value() as f64;
            (255.0 * lv / 100.0 + 0.5) as i64
        };
        let ig8 = params.get(Params::IgColor)?.as_color()?.value();
        let ig_colour = match depth {
            8 => Px {
                a: ig8.alpha as f64,
                r: ig8.red as f64,
                g: ig8.green as f64,
                b: ig8.blue as f64,
            },
            16 => {
                let c = libs::utils::conv_8_to_16(&ig8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
            _ => {
                let c = libs::utils::conv_8_to_32(&ig8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
        };
        let minmax1 = params.get(Params::MinMax1)?.as_slider()?.value() as i64;
        let minmax2 = params.get(Params::MinMax2)?.as_slider()?.value() as i64;
        let blur_v = params.get(Params::LineBlur)?.as_slider()?.value() as i64;
        // 元: 角度を 0..360 に収めて 360 で割る。
        let hue = {
            let mut r =
                libs::utils::flt2fix(params.get(Params::LineH)?.as_angle()?.value() as f64) as i64
                    % (360 << 16);
            if r < 0 {
                r += 360 << 16;
            }
            (r as f64 / 65536.0) / 360.0
        };
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() as f64 / 100.0)
        };
        let info = Info {
            line_only,
            colour,
            ng_colour,
            level,
            ig_colour,
            blur: blur_v,
            minmax1,
            minmax2,
            hue,
            sat: pct(Params::LineS, params)?,
            light: pct(Params::LineL, params)?,
            red: pct(Params::LineRed, params)?,
            green: pct(Params::LineGreen, params)?,
            blue: pct(Params::LineBlue, params)?,
        };

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
            line_only: false,
            colour: vec![(0, 0, 0)],
            ng_colour: vec![],
            level: 16,
            ig_colour: Px {
                a: 255.0,
                r: 16.0,
                g: 16.0,
                b: 16.0,
            },
            blur: 0,
            minmax1: 0,
            minmax2: 0,
            hue: 0.0,
            sat: 0.0,
            light: 0.0,
            red: 0.0,
            green: 0.0,
            blue: 0.0,
        }
    }

    fn px(a: f64, v: f64) -> Px {
        Px {
            a,
            r: v,
            g: v,
            b: v,
        }
    }

    /// 主線は `MAX`、塗りは `HALF`、真っ白と透明は 0。
    #[test]
    fn the_marks_tell_line_from_paint() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(4, 1);
        inp.px[0] = px(255.0, 0.0);
        inp.px[1] = px(255.0, 100.0);
        inp.px[2] = px(255.0, 255.0);
        inp.px[3] = px(0.0, 50.0);
        let mut out = Buf::new(4, 1);
        let n = target_select(&inp, &mut out, &i, &o);
        assert_eq!(n, 1);
        assert_eq!(out.px[0].a, 255.0, "主線");
        assert_eq!(out.px[1].a, 128.0, "塗り");
        assert_eq!(out.px[2].a, 0.0, "真っ白");
        assert_eq!(out.px[3].a, 0.0, "透明");
    }

    /// 主線は隣の塗りのいちばん暗い色で埋まる。
    #[test]
    fn the_line_fills_with_the_darkest_paint() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(3, 1);
        inp.px[0] = px(255.0, 200.0);
        inp.px[1] = px(255.0, 0.0);
        inp.px[2] = px(255.0, 100.0);
        let mut out = Buf::new(3, 1);
        exec(&inp, &mut out, &i, &o);
        assert_eq!(out.px[1].r, 100.0, "右の暗い塗りで埋まる");
        assert_eq!(out.px[1].a, 255.0, "アルファは入力のまま");
    }

    /// まわりに塗りが無ければ `GiveUpColor` になる。
    #[test]
    fn a_line_with_no_paint_gives_up() {
        let o = ops8();
        let i = info();
        let mut inp = Buf::new(2, 1);
        inp.px[0] = px(255.0, 0.0);
        inp.px[1] = px(255.0, 0.0);
        let mut out = Buf::new(2, 1);
        exec(&inp, &mut out, &i, &o);
        assert_eq!(out.px[0].r, 16.0);
        assert_eq!(out.px[1].r, 16.0);
    }

    /// `LineOnly` は主線だけ残す。
    #[test]
    fn the_line_only_switch_keeps_the_line() {
        let o = ops8();
        let mut i = info();
        i.line_only = true;
        let mut inp = Buf::new(3, 1);
        inp.px[0] = px(255.0, 200.0);
        inp.px[1] = px(255.0, 0.0);
        inp.px[2] = px(255.0, 100.0);
        let mut out = Buf::new(3, 1);
        exec(&inp, &mut out, &i, &o);
        assert_eq!(out.px[1].a, 255.0, "主線は残る");
        assert_eq!(out.px[0].a, 0.0, "塗りは消える");
    }

    /// `NegColor` に入れた色は主線にならない。
    #[test]
    fn the_neg_colour_is_never_a_line() {
        let o = ops8();
        let mut i = info();
        i.ng_colour = vec![(0, 0, 0)];
        let mut inp = Buf::new(2, 1);
        inp.px[0] = px(255.0, 0.0);
        inp.px[1] = px(255.0, 100.0);
        let mut out = Buf::new(2, 1);
        let n = target_select(&inp, &mut out, &i, &o);
        assert_eq!(n, 1, "主線色の判定が先なのでそちらが勝つ");
        i.colour = vec![(255, 255, 255)];
        let n = target_select(&inp, &mut out, &i, &o);
        assert_eq!(n, 0);
        assert_eq!(out.px[0].a, 0.0);
    }

    /// 主線のぼかしは主線の中だけで平均する。
    #[test]
    fn the_blur_stays_inside_the_line() {
        let o = ops8();
        let mut out = Buf::new(4, 1);
        out.px[0] = px(255.0, 0.0);
        out.px[1] = px(255.0, 100.0);
        out.px[2] = px(128.0, 200.0);
        out.px[3] = px(128.0, 200.0);
        blur(&mut out, 1, &o);
        assert_eq!(out.px[0].r, 50.0, "(0+100)/2");
        assert_eq!(out.px[2].r, 200.0, "塗りは動かない");
    }

    /// `Min/Max` の正は明るいほう、負は暗いほうへ寄せる。
    #[test]
    fn the_minmax_picks_the_bright_or_dark_side() {
        let o = ops8();
        let mut out = Buf::new(3, 1);
        out.px[0] = px(255.0, 0.0);
        out.px[1] = px(255.0, 100.0);
        out.px[2] = px(255.0, 200.0);
        let mut up = Buf::new(3, 1);
        up.px.copy_from_slice(&out.px);
        min_max(&mut up, 1, &o);
        assert_eq!(up.px[0].r, 100.0);
        assert_eq!(up.px[1].r, 200.0);
        let mut down = Buf::new(3, 1);
        down.px.copy_from_slice(&out.px);
        min_max(&mut down, -1, &o);
        assert_eq!(down.px[1].r, 0.0);
        assert_eq!(down.px[2].r, 100.0);
    }

    /// 色変えは主線だけに効く。
    #[test]
    fn the_colour_change_only_touches_the_line() {
        let o = ops8();
        let mut i = info();
        i.light = 0.5;
        let mut out = Buf::new(2, 1);
        out.px[0] = px(255.0, 100.0);
        out.px[1] = px(128.0, 100.0);
        target_colour(&mut out, &i, &o);
        assert!(out.px[0].r > 100.0, "明るくなる");
        assert_eq!(out.px[1].r, 100.0, "塗りはそのまま");
    }
}
