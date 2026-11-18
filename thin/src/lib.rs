//! F's Thin の移植。
//!
//! セル画の**主線を細くする**。線の画素のまわり8つを見て、形 (パターン)
//! ごとに「消してよいか」を決め、消すときはまわりの色で塗りつぶす。
//! `ThinValue` の回数くりかえすと、そのぶん細くなる。
//!
//! 手順 (元の thinExec):
//!
//! `ThinValue` 回、次の4つの走査をこの順でかける。
//!
//! 1. `EdgeFilter` が入っていれば `getPatEdge`: 線が斜めに折れている
//!    ところだけを、両隣と同じ色なら隣の色で埋める (角を落とす)
//! 2. `getPat`: まわり8つのうち線の数 (`patCount`) で場合分けし、
//!    決まったパターンのときだけ上下左右のどれかの色で埋める。
//!    `patCount` が 8 なら線の内側なので何もしない、0 なら孤立点なので
//!    上下左右のいちばん暗い色で埋める
//! 3. `getPatDot`: 「かど3つだけが線」の4パターンを埋める
//! 4. `getPatDot2nd`: 線の**隣**の画素を書き替える4パターン
//!    (`dir` の向きの隣に色を置く)
//!
//! 元から写した細かいところ:
//!
//! - まわりを読むのは「3行ぶんの写し」からで、同じ走査での書き込みは
//!   見えない。縁は押さえる (`getScanLine`)
//! - ただし `getPat` の中の `getData` だけは**書き込み済みの画面**を
//!   直に読む。走査の順 (上から下、左から右) で結果が変わるのも元のまま
//! - 色が主線かどうかは 8bit に落としてチャンネルごとの差を `level`
//!   (= `255 × % / 100` を四捨五入した整数) と比べる
//! - 埋める色を選ぶ `minColor` は 8bit の R+G+B が小さいほう
//! - `NoWhite` が入っていれば真っ白 (アルファも 255) で埋めない、
//!   `NoAlphaZero` が入っていればアルファ 0 で埋めない
//! - `Color2`〜`Color4` は同じ色の重複を省く (`Color1` は素通し)
//! - パターンの番号は、まわり8つを左上から右下へ (真ん中を除く) 並べて
//!   1ビットずつ詰めたもの。上位が左上
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **`EdgeFilter` の走査だけマルチスレッドで、行の継ぎ目が崩れる**。
//!    スレッドごとに持つ3行の写しを、担当の最初の行で「1行ずらす」から
//!    始めるので、その行とその上の行が**0 で埋まったまま**になる
//!    (`std::vector` の初期値)。0 は黒なので、既定の主線色だと「線が
//!    ある」と見なされて、継ぎ目の行だけ違う形に見える。しかも分ける
//!    数が `std::thread::hardware_concurrency()` なので、**出る絵が
//!    走らせた機械のコア数で変わる**。1本の走査に直した
//!
//! 崩れかたは機械のコア数で変わるので、`FAITHFUL` で元に戻す口は作って
//! いない。突き合わせでは、`EdgeFilter` を切った回はぜんぶ画素まで一致し、
//! 入れた回だけ**継ぎ目の行に数画素の差**が出る (y = 48 や y = 89 など、
//! スレッドの担当の切れ目)。差がそこだけに出ることが、この直しの裏づけに
//! なっている。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Thin

use after_effects::{self as ae};

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
    /// 8bit の目盛りに直す (元の ConvertTo8bit)。
    pub fn to8(&self, p: &Px) -> (i64, i64, i64, i64) {
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
    pub value: i64,
    /// 主線の色 (8bit の目盛り)。
    pub colour: Vec<(i64, i64, i64)>,
    pub level: i64,
    pub white: bool,
    pub alpha_zero: bool,
    pub edge: bool,
}

/// 元の scanlineCopy。3行ぶんの写し。
struct Scan {
    w: i64,
    h: i64,
    rows: [Vec<Px>; 3],
}

impl Scan {
    fn new(buf: &Buf) -> Self {
        Scan {
            w: buf.w,
            h: buf.h,
            rows: [
                vec![Px::default(); buf.w as usize],
                vec![Px::default(); buf.w as usize],
                vec![Px::default(); buf.w as usize],
            ],
        }
    }

    /// 元の scanlineCopy(y)。y=0 は上の行を2回ぶん、いちばん下の行では
    /// 3行目を読み直さない (結果としてどちらも縁を押さえた形になる)。
    ///
    /// 高さが1しか無いときは下の行が存在しない。元は y=0 のとき下の行を
    /// 無条件に読むので、そこでワールドの外を触る (C では次の行の中身が
    /// そのまま入るだけで落ちない)。ここでは縁を押さえて同じ行を使う。
    fn copy(&mut self, buf: &Buf, y: i64) {
        if y == 0 {
            for i in 0..self.w as usize {
                self.rows[0][i] = buf.px[i];
                self.rows[1][i] = buf.px[i];
                let below = if buf.h > 1 { i + buf.w as usize } else { i };
                self.rows[2][i] = buf.px[below];
            }
        } else {
            self.rows.swap(0, 1);
            self.rows.swap(1, 2);
            if y < self.h - 1 {
                let v = ((y + 1) * buf.w) as usize;
                self.rows[2].copy_from_slice(&buf.px[v..v + self.w as usize]);
            }
        }
    }

    /// 元の getScanLine。
    fn get(&self, now_x: i64, dx: i64, dy: i64) -> Px {
        let xx = (now_x + dx).clamp(0, self.w - 1);
        let yy = (1 + dy).clamp(0, 2);
        self.rows[yy as usize][xx as usize]
    }
}

/// 元の CompPx。色見本のどれかに近いか。
pub fn comp_px(info: &Info, p: &Px, ops: &Ops) -> bool {
    let (_, r, g, b) = ops.to8(p);
    info.colour.iter().any(|c| {
        (c.0 - r).abs() <= info.level
            && (c.1 - g).abs() <= info.level
            && (c.2 - b).abs() <= info.level
    })
}

/// 元の CompPxs2。2つの色が近いか。
fn comp_pxs2(info: &Info, p1: &Px, p2: &Px, ops: &Ops) -> bool {
    let (_, r1, g1, b1) = ops.to8(p1);
    let (_, r2, g2, b2) = ops.to8(p2);
    (r1 - r2).abs() <= info.level
        && (g1 - g2).abs() <= info.level
        && (b1 - b2).abs() <= info.level
}

/// 元の CompPxs。3つの色が (1つめを軸に) 近いか。
fn comp_pxs(info: &Info, p1: &Px, p2: &Px, p3: &Px, ops: &Ops) -> bool {
    comp_pxs2(info, p1, p2, ops) && comp_pxs2(info, p1, p3, ops)
}

/// 元の minColor。8bit の R+G+B が小さいほう。
fn min_colour(s: Px, d: Px, ops: &Ops) -> Px {
    let (_, sr, sg, sb) = ops.to8(&s);
    let (_, dr, dg, db) = ops.to8(&d);
    if sr + sg + sb < dr + dg + db {
        s
    } else {
        d
    }
}

/// 元の shouldSkipPixel。
fn should_skip(info: &Info, p: &Px, ops: &Ops) -> bool {
    if !info.white && !info.alpha_zero {
        return false;
    }
    let (a, r, g, b) = ops.to8(p);
    if info.white && r == 255 && g == 255 && b == 255 && a == 255 {
        return true;
    }
    if info.alpha_zero && a == 0 {
        return true;
    }
    false
}

/// 元の getData。**書き込み済みの画面**を直に読む。
fn get_data(buf: &Buf, info: &Info, now_x: i64, now_y: i64, dx: i64, dy: i64, ops: &Ops) -> bool {
    let xx = (now_x + dx).clamp(0, buf.w - 1);
    let yy = (now_y + dy).clamp(0, buf.h - 1);
    comp_px(info, &buf.at(xx, yy), ops)
}

/// まわり8つを左上から右下へ (真ん中を抜いて) 並べたもの。
fn table(scan: &Scan, x: i64) -> [Px; 8] {
    [
        scan.get(x, -1, -1),
        scan.get(x, 0, -1),
        scan.get(x, 1, -1),
        scan.get(x, -1, 0),
        scan.get(x, 1, 0),
        scan.get(x, -1, 1),
        scan.get(x, 0, 1),
        scan.get(x, 1, 1),
    ]
}

/// パターン番号と、まわりの線の数。
fn pattern(info: &Info, t: &[Px; 8], ops: &Ops) -> (i64, i64) {
    let mut pat = 0i64;
    let mut count = 0i64;
    for p in t.iter() {
        pat <<= 1;
        if comp_px(info, p, ops) {
            pat |= 1;
            count += 1;
        }
    }
    (pat, count)
}

/// 元の getPatEdge。角を落とす。
fn get_pat_edge(info: &Info, t: &[Px; 8], ops: &Ops) -> Option<Px> {
    let (pat, _) = pattern(info, t, ops);
    // (パターン, 3つ比べる組 (2つなら None), 置く色)
    let three: [(&[i64], [usize; 3], usize); 8] = [
        (&[0x8F, 0x8E], [1, 2, 3], 1),
        (&[0x6D, 0x4D], [0, 3, 6], 0),
        (&[0xF1, 0x71], [4, 5, 6], 4),
        (&[0x86, 0xB2], [1, 4, 7], 1),
        (&[0x35, 0x33], [0, 1, 4], 0),
        (&[0xD5, 0x55], [2, 4, 6], 2),
        (&[0xEC, 0xCC], [3, 6, 7], 3),
        (&[0xAB, 0xAA], [1, 3, 5], 1),
    ];
    for (pats, idx, col) in three {
        if pats.contains(&pat) {
            return if comp_pxs(info, &t[idx[0]], &t[idx[1]], &t[idx[2]], ops) {
                Some(t[col])
            } else {
                None
            };
        }
    }
    let two: [(&[i64], [usize; 2], usize); 4] = [
        (&[0xED, 0xCD], [3, 6], 3),
        (&[0xAF, 0xAE], [1, 3], 1),
        (&[0xB7, 0xB3], [1, 4], 1),
        (&[0xF5, 0x75], [4, 6], 4),
    ];
    for (pats, idx, col) in two {
        if pats.contains(&pat) {
            return if comp_pxs2(info, &t[idx[0]], &t[idx[1]], ops) {
                Some(t[col])
            } else {
                None
            };
        }
    }
    None
}

/// 元の getPatDot。かど3つだけが線のとき。
fn get_pat_dot(info: &Info, t: &[Px; 8], ops: &Ops) -> Option<Px> {
    let (pat, _) = pattern(info, t, ops);
    let m = |a: usize, b: usize, c: usize| {
        Some(min_colour(t[a], min_colour(t[b], t[c], ops), ops))
    };
    match pat {
        0xE0 => m(3, 4, 6),
        0x29 => m(1, 3, 6),
        0x07 => m(1, 3, 4),
        0x94 => m(1, 4, 6),
        _ => None,
    }
}

/// 元の getPatDot2nd。隣の画素を書き替えるパターン。返すのは (色, 向き)。
/// 向きは 0:上 1:右 2:下 3:左。
fn get_pat_dot2nd(info: &Info, t: &[Px; 8], ops: &Ops) -> Option<(Px, i64)> {
    let (pat, _) = pattern(info, t, ops);
    if pat & 0x1F == 0x1D {
        Some((t[3], 2))
    } else if pat & 0xD6 == 0xC6 {
        Some((t[0], 3))
    } else if pat & 0xF8 == 0xB8 {
        Some((t[0], 0))
    } else if pat & 0x6B == 0x63 {
        Some((t[1], 1))
    } else {
        None
    }
}

/// 元の getPat。細める本体。
fn get_pat(
    buf: &Buf,
    info: &Info,
    t: &[Px; 8],
    now_x: i64,
    now_y: i64,
    ops: &Ops,
) -> Option<Px> {
    let (pat, count) = pattern(info, t, ops);
    let gd = |dx: i64, dy: i64| get_data(buf, info, now_x, now_y, dx, dy, ops);
    let mn = |a: usize, b: usize| min_colour(t[a], t[b], ops);
    if count > 7 {
        return None;
    }
    if count <= 0 {
        // 孤立点。上下左右のいちばん暗い色。
        let c = min_colour(t[1], t[3], ops);
        let c = min_colour(c, t[4], ops);
        return Some(min_colour(c, t[6], ops));
    }
    match count {
        7 => match pat {
            0xBF => gd(0, 1).then(|| t[1]),
            0xEF => gd(1, 0).then(|| t[3]),
            0xFD => gd(0, -1).then(|| t[6]),
            0x7F => gd(-1, 0).then(|| t[4]),
            _ => None,
        },
        6 => match pat {
            0x9F | 0x3F => gd(0, -1).then(|| t[1]),
            0x6F | 0xEB => gd(1, 0).then(|| t[3]),
            0xF9 | 0xFC => gd(0, -1).then(|| t[6]),
            0xF6 | 0xD7 => gd(-1, 0).then(|| t[4]),
            _ => None,
        },
        5 => match pat {
            0xF8 => gd(0, -1).then(|| t[6]),
            0x1F => gd(0, 1).then(|| t[1]),
            0x6B => gd(1, 0).then(|| t[3]),
            0xD6 => gd(-1, 0).then(|| t[4]),
            0x97 => Some(mn(1, 4)),
            0x2F => Some(mn(1, 3)),
            0xE9 => Some(mn(3, 6)),
            0xF4 => Some(mn(4, 6)),
            0x5D => gd(0, -1).then(|| t[6]),
            0x73 => gd(-1, 0).then(|| t[4]),
            0xBA => gd(0, -1).then(|| t[1]),
            0xCE => gd(1, 0).then(|| t[3]),
            _ => None,
        },
        4 => match pat {
            0x0F | 0x2B => Some(mn(1, 3)),
            0x69 | 0xE8 => Some(mn(3, 6)),
            0xF0 | 0xD4 => Some(mn(4, 6)),
            0x96 | 0x17 => Some(mn(1, 4)),
            0xC9 => (gd(0, -1) && gd(1, 0)).then(|| mn(3, 6)),
            0x74 => (gd(0, -1) && gd(-1, 0)).then(|| mn(4, 6)),
            0x93 => (gd(-1, 0) && gd(0, 1)).then(|| mn(1, 4)),
            0x2E => (gd(1, 0) && gd(0, 1)).then(|| mn(1, 3)),
            _ => None,
        },
        3 => match pat {
            0xE0 => gd(0, -1).then(|| min_colour(mn(3, 4), t[6], ops)),
            0x94 => gd(-1, 0).then(|| min_colour(mn(1, 4), t[6], ops)),
            0x07 => gd(0, 1).then(|| min_colour(mn(1, 3), t[4], ops)),
            0x29 => gd(1, 0).then(|| min_colour(mn(1, 3), t[6], ops)),
            0x70 => gd(0, -1).then(|| mn(4, 6)),
            0x13 => gd(-1, 0).then(|| mn(4, 1)),
            0x2A => gd(1, 0).then(|| mn(1, 3)),
            0x92 => gd(-1, 0).then(|| mn(1, 4)),
            0xC8 => gd(0, -1).then(|| mn(3, 6)),
            0x0E => gd(1, 0).then(|| mn(1, 3)),
            0xD0 => (gd(0, -1) && gd(-1, 0)).then(|| mn(4, 6)),
            0x16 => (gd(-1, 0) && gd(0, 1)).then(|| mn(1, 4)),
            0x0B => (gd(1, 0) && gd(0, 1)).then(|| mn(1, 3)),
            _ => None,
        },
        2 => match pat {
            0x50 => (gd(0, -1) && gd(-1, 0)).then(|| mn(4, 6)),
            0x12 => (gd(-1, 0) && gd(0, 1)).then(|| mn(1, 4)),
            0x0A => (gd(1, 0) && gd(0, 1)).then(|| mn(1, 3)),
            0x48 => (gd(0, -1) && gd(1, 0)).then(|| mn(3, 6)),
            _ => None,
        },
        _ => None,
    }
}

/// 元の thinExec。`ThinValue` 回ぶん細める。
pub fn exec(out: &mut Buf, info: &Info, ops: &Ops) {
    if info.colour.is_empty() || info.value <= 0 {
        return;
    }
    let mut scan = Scan::new(out);
    for _ in 0..info.value {
        if info.edge {
            for y in 0..out.h {
                scan.copy(out, y);
                for x in 0..out.w {
                    let p = scan.get(x, 0, 0);
                    if !comp_px(info, &p, ops) {
                        continue;
                    }
                    let t = table(&scan, x);
                    if let Some(c) = get_pat_edge(info, &t, ops) {
                        if !should_skip(info, &c, ops) {
                            out.set(x, y, c);
                        }
                    }
                }
            }
        }
        // 本体。getData が画面を直に読むので、順に走らせる。
        for y in 0..out.h {
            scan.copy(out, y);
            for x in 0..out.w {
                let p = scan.get(x, 0, 0);
                if !comp_px(info, &p, ops) {
                    continue;
                }
                let t = table(&scan, x);
                if let Some(c) = get_pat(out, info, &t, x, y, ops) {
                    if !should_skip(info, &c, ops) {
                        out.set(x, y, c);
                    }
                }
            }
        }
        // 点を消す。
        for y in 0..out.h {
            scan.copy(out, y);
            for x in 0..out.w {
                let p = scan.get(x, 0, 0);
                if !comp_px(info, &p, ops) {
                    continue;
                }
                let t = table(&scan, x);
                if let Some(c) = get_pat_dot(info, &t, ops) {
                    if !should_skip(info, &c, ops) {
                        out.set(x, y, c);
                    }
                }
            }
        }
        // 隣を書き替える。
        for y in 0..out.h {
            scan.copy(out, y);
            for x in 0..out.w {
                let p = scan.get(x, 0, 0);
                if !comp_px(info, &p, ops) {
                    continue;
                }
                let t = table(&scan, x);
                let Some((c, dir)) = get_pat_dot2nd(info, &t, ops) else {
                    continue;
                };
                let (tx, ty) = match dir {
                    0 => (x, y - 1),
                    1 => (x + 1, y),
                    2 => (x, y + 1),
                    _ => (x - 1, y),
                };
                if tx < 0 || tx >= out.w || ty < 0 || ty >= out.h {
                    continue;
                }
                let old = out.at(tx, ty);
                if !should_skip(info, &old, ops) {
                    out.set(tx, ty, c);
                }
            }
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    Color1On,
    Color1,
    Color2On,
    Color2,
    Color3On,
    Color3,
    Color4On,
    Color4,
    Level,
    White,
    AlphaZero,
    Edge,
}

/// 色4つ (チェックと色)。
const COLOURS: [(Params, Params); 4] = [
    (Params::Color1On, Params::Color1),
    (Params::Color2On, Params::Color2),
    (Params::Color3On, Params::Color3),
    (Params::Color4On, Params::Color4),
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
        // 元: PF_ADD_SLIDER("ThinValue", 0, 16, 0, 4, 0)
        params.add(
            Params::Value,
            "ThinValue",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(16);
                f.set_slider_min(0);
                f.set_slider_max(4);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        let on = [true, false, false, false];
        let cols = [
            (0x05u8, 0x05u8, 0x05u8),
            (0x0A, 0x0A, 0x0A),
            (0xFF, 0x00, 0x00),
            (0x00, 0xFF, 0x00),
        ];
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
                        red: c.0,
                        green: c.1,
                        blue: c.2,
                    });
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_FLOAT_SLIDER("level", 0, 100, 0, 10, 0)
        params.add(
            Params::Level,
            "level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(10.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        for (id, name) in [
            (Params::White, "NoWhite"),
            (Params::AlphaZero, "NoAlphaZero"),
            (Params::Edge, "EdgeFilter"),
        ] {
            params.add(
                id,
                name,
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(false);
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
                out_data.set_return_msg("fs-rs thin");
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
        let value = params.get(Params::Value)?.as_slider()?.value() as i64;
        let mut colour: Vec<(i64, i64, i64)> = Vec::new();
        for (i, (on_id, col_id)) in COLOURS.iter().enumerate() {
            if !params.get(*on_id)?.as_checkbox()?.value() {
                continue;
            }
            let c = params.get(*col_id)?.as_color()?.value();
            let t = (c.red as i64, c.green as i64, c.blue as i64);
            // 元は Color1 だけ重複を省かない。
            if i > 0 && colour.contains(&t) {
                continue;
            }
            colour.push(t);
        }
        let level = {
            let lv = params.get(Params::Level)?.as_float_slider()?.value() as f64 / 100.0;
            (255.0 * lv + 0.5) as i64
        };
        let info = Info {
            value,
            colour,
            level,
            white: params.get(Params::White)?.as_checkbox()?.value(),
            alpha_zero: params.get(Params::AlphaZero)?.as_checkbox()?.value(),
            edge: params.get(Params::Edge)?.as_checkbox()?.value(),
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
        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        if w == 0 || h == 0 || info.colour.is_empty() || info.value <= 0 {
            return Ok(());
        }

        let mut out = Buf::new(w, h);
        macro_rules! go {
            ($wr:ident, $get:expr, $put:expr) => {{
                for y in 0..h {
                    for x in 0..w {
                        out.set(x, y, $get(out_layer.$wr(x as usize, y as usize)));
                    }
                }
                exec(&mut out, &info, &ops);
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

    fn info(value: i64) -> Info {
        Info {
            value,
            colour: vec![(0, 0, 0)],
            level: 0,
            white: false,
            alpha_zero: false,
            edge: false,
        }
    }

    fn px(v: f64) -> Px {
        Px {
            a: 255.0,
            r: v,
            g: v,
            b: v,
        }
    }

    /// 太い線は細くなる。
    #[test]
    fn a_thick_line_gets_thinner() {
        let o = ops8();
        let mut b = Buf::new(9, 9);
        b.px.iter_mut().for_each(|p| *p = px(255.0));
        // 縦に3画素ぶんの太さの線。
        for y in 0..9 {
            for x in 3..6 {
                b.set(x, y, px(0.0));
            }
        }
        let before = b.px.iter().filter(|p| p.r == 0.0).count();
        exec(&mut b, &info(1), &o);
        let after = b.px.iter().filter(|p| p.r == 0.0).count();
        assert!(after < before, "{before} -> {after}");
        assert!(after > 0, "線は残る");
    }

    /// 縦1画素・横1画素のレイヤーでも落ちない (下の行が無い回)。
    #[test]
    fn a_one_pixel_tall_layer_is_safe() {
        let o = ops8();
        for (w, h) in [(8, 1), (1, 8), (1, 1)] {
            let mut b = Buf::new(w, h);
            b.px.iter_mut().for_each(|p| *p = px(255.0));
            b.set(w / 2, h / 2, px(0.0));
            exec(&mut b, &info(2), &o);
            assert_eq!(b.px.len(), (w * h) as usize, "{w}x{h}");
        }
    }

    /// `ThinValue` を増やすほど細くなる。
    #[test]
    fn more_passes_thin_more() {
        let o = ops8();
        let mk = || {
            let mut b = Buf::new(13, 13);
            b.px.iter_mut().for_each(|p| *p = px(255.0));
            for y in 0..13 {
                for x in 4..9 {
                    b.set(x, y, px(0.0));
                }
            }
            b
        };
        let mut one = mk();
        exec(&mut one, &info(1), &o);
        let mut two = mk();
        exec(&mut two, &info(2), &o);
        let n1 = one.px.iter().filter(|p| p.r == 0.0).count();
        let n2 = two.px.iter().filter(|p| p.r == 0.0).count();
        assert!(n2 < n1, "{n1} -> {n2}");
    }

    /// 孤立点は消える。
    #[test]
    fn a_lone_dot_is_wiped() {
        let o = ops8();
        let mut b = Buf::new(5, 5);
        b.px.iter_mut().for_each(|p| *p = px(255.0));
        b.set(2, 2, px(0.0));
        exec(&mut b, &info(1), &o);
        assert_eq!(b.at(2, 2).r, 255.0);
    }

    /// 色見本に無い色は細らない。
    #[test]
    fn another_colour_is_left_alone() {
        let o = ops8();
        let mut i = info(1);
        i.colour = vec![(255, 0, 0)];
        let mut b = Buf::new(5, 5);
        b.px.iter_mut().for_each(|p| *p = px(255.0));
        for y in 0..5 {
            b.set(2, y, px(0.0));
        }
        let before: Vec<Px> = b.px.clone();
        exec(&mut b, &i, &o);
        assert_eq!(b.px, before);
    }

    /// `NoWhite` が入っていれば白では埋めない。
    #[test]
    fn the_no_white_switch_keeps_the_line() {
        let o = ops8();
        let mut b = Buf::new(5, 5);
        b.px.iter_mut().for_each(|p| *p = px(255.0));
        b.set(2, 2, px(0.0));
        let mut i = info(1);
        i.white = true;
        exec(&mut b, &i, &o);
        assert_eq!(b.at(2, 2).r, 0.0, "白で埋められないので残る");
    }

    /// パターン番号は左上から右下へ詰めたもの。
    #[test]
    fn the_pattern_number_packs_the_neighbours() {
        let o = ops8();
        let i = info(1);
        let mut t = [px(255.0); 8];
        t[0] = px(0.0);
        assert_eq!(pattern(&i, &t, &o), (0x80, 1));
        t[7] = px(0.0);
        assert_eq!(pattern(&i, &t, &o), (0x81, 2));
    }
}
