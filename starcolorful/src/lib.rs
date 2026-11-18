//! F's Star_Colorful の移植。
//!
//! 明るいところの**縁**を拾って、そこから十字と斜めの光を伸ばす。
//! F's Star と違って、光の色を固定色・元の絵の色・虹の3通りから選べる。
//!
//! 手順 (元の StarExec8):
//!
//! 1. 入力をそのままコピーする
//! 2. `TargetPixel`: 画素ごとに「対象の強さ」を出して、その強さを
//!    アルファに、`元の色 × 強さ` を RGB に入れる (黒マット)。強さは
//!    `Target` の4通り (RGB の最大値 / R+G+B / 明るさ / 指定色との
//!    Lab 距離) を `TargetBorder` と `TargetSoftness` で 0..1 に切ったもの
//! 3. `TargetDisp` が入っていればここで乗算を戻して終わり (選び具合を見る用)
//! 4. 下書きにアルファを写して反転し、`MaxHor`→`MaxVer` を3回かけて
//!    じわじわ太らせる。反転しているので実際には**縮む**
//! 5. `TempSub`: 元のアルファから縮んだアルファを引く。残るのは縁の帯で、
//!    これが光の強さになる (縮んだぶんは緑に残り、`CenterDraw` が使う)
//! 6. 縁の帯を3×3でぼかす
//! 7. `ColorKind` で描き方を選ぶ
//!    - `Color`: 出力を消して `Color` 色の光を描き、中心を `CenterDraw` で光らせる
//!    - `Screen`: 出力は消さず、下書きに**入力の色**を入れて、画素ごとに
//!      その色の光を描く
//!    - `Rainbow`: 出力を消して、長さに沿って色相を回した光を描く
//! 8. 出力全体を3×3でぼかす
//! 9. `Blemnd` が None 以外なら入力と合成する (Screen / Add / Normal)
//! 10. 乗算を戻す
//!
//! 光1本は「長さぶんの色表」を作り、`AddPxD` で小数座標に双一次で
//! スクリーン合成していく。縦・横は左右両方向の2本、斜めは2つの角度の
//! 4本を描く。
//!
//! 元から写した細かいところ:
//!
//! - 長さは `各長さ × Length` に 0.5 を足して切り捨て。向きは
//!   `CRot::SetRotLength` (0 が上、時計回り) で、1画素ぶんの歩幅は
//!   `ベクトル / 長さ`
//! - 色表は `不透明度 × (1 - i/長さ) × 0.5`。前半 (`長さ/2` まで) だけ
//!   `ColorBrightness` で白へ寄せる
//! - `AutoRolling` が入っていると `Rot` に `RollingSpeed × 時間` を足す
//!   (時間は `current_time / time_scale` 秒)
//! - 太らせるのは `org += 隣の最大/2` を上限 `隣の最大` で止めるやりかた。
//!   8/16bit では整数の割り算なので奇数で 1 ずつ損をする
//! - `blur` も `blurTempRed` も**その場で**書き戻すので、左と上は
//!   ぼかし済みの値を読む。元の順どおり画素の並びのまま回している
//! - 虹の色相は `(RainbowOffset - RainbowDelta × i) × 65536` を整数に
//!   して 65536 で割った余り。`RainbowDelta` が 0 のときは 0.001 にされる
//! - 合成は3つとも「入力のアルファで乗算した色」を足す。`Normal` だけ
//!   先に出力のアルファぶんを空ける
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **縁を押さえる `GetPX` が、縦のはみ出しで `xx` に代入している**
//!    (`if (yy >= height) xx = height - 1;`、`yy` のつもり)。上下の端で
//!    ワールドの外を読んでしまう。`yy` を押さえるようにした。元が読む
//!    上の縁は `yy = 0` の枝があるので実は無事で、**下の縁だけ**が
//!    初期化されていないメモリを読む。そこを読んだ値は決まっていない
//!    ので、`FAITHFUL` では 0 として、下の縁から「光の長さ + 1」行を
//!    外したところを画素まで突き合わせてある (13通り、残り全部一致)
//! 2. **Lab の暗いところの -0.16** (`libs::lab` の注を参照)。`Target` が
//!    `TargetColor` のときだけ効く
//!
//! `FAITHFUL` を立てると 1 を「外は 0」に、2 を元の式に戻す。元の .aex と
//! 画素まで一致するのを確かめるための入口で、出荷するのは直した側。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Star_Colorful

use after_effects::{self as ae};

/// 直したバグを戻して、元の .aex と突き合わせるための切り替え。
const FAITHFUL: bool = false;

/// 深さに依らない画素。値はその深さの生の目盛り (8bit なら 0..255)。
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深さごとの丸めと上限。
#[derive(Clone, Copy, Debug)]
pub struct Ops {
    pub max: f64,
    pub float: bool,
}

impl Ops {
    /// 元の RoundByteFpLong / RoundShortFpLong / RoundFpShortDouble。
    /// 整数の深さは切り捨て、32bit は 0..32 で止めるだけ。
    fn clip(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 32.0)
        } else {
            v.clamp(0.0, self.max).trunc()
        }
    }

    /// 整数の深さでは切り捨てる割り算 (`mx1 / 2`)。
    fn half(&self, v: f64) -> f64 {
        if self.float {
            v / 2.0
        } else {
            (v as i64 / 2) as f64
        }
    }

    /// 元の scr。スクリーン合成の1チャンネルぶん。
    fn scr(&self, s: f64, d: f64) -> f64 {
        if self.float {
            (d + s - s * d).clamp(0.0, 1.0)
        } else {
            let m = self.max as i64;
            (d + s - ((s as i64 * d as i64) / m) as f64).clamp(0.0, self.max)
        }
    }

    /// 整数の深さだけ切り捨てる (`(A_long)` のキャスト)。
    fn trunc_int(&self, v: f64) -> f64 {
        if self.float {
            v
        } else {
            v.trunc()
        }
    }

    /// 元の PxPer。濃度をかける。
    fn px_per(&self, p: &Px, per: f64) -> Px {
        Px {
            a: self.clip(p.a * per),
            r: self.clip(p.r * per),
            g: self.clip(p.g * per),
            b: self.clip(p.b * per),
        }
    }
}

/// 画素の並び。元の GInfo (widthTrue の詰めは Rust では要らない)。
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

    /// 元の GetPX。横は縁で押さえる。縦は元が `xx` に代入していて外を
    /// 読むので、`FAITHFUL` では 0、そうでなければ縁で押さえる。
    fn get_px(&self, x: i64, y: i64) -> Px {
        let xx = x.clamp(0, self.w - 1);
        if y >= self.h && FAITHFUL {
            // 元はここでワールドの外を読む。中身は決まっていないので
            // 0 を返しておく (下の縁の帯は突き合わせから外す)。
            return Px::default();
        }
        if y < 0 || y >= self.h {
            return self.at(xx, y.clamp(0, self.h - 1));
        }
        self.at(xx, y)
    }

    /// 元の AddPxL。範囲内なら1画素スクリーン合成する。
    fn add_px_l(&mut self, x: i64, y: i64, p: &Px, ops: &Ops) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return;
        }
        let d = self.at(x, y);
        self.set(
            x,
            y,
            Px {
                a: ops.scr(d.a, p.a),
                r: ops.scr(d.r, p.r),
                g: ops.scr(d.g, p.g),
                b: ops.scr(d.b, p.b),
            },
        );
    }

    /// 元の AddPxD。小数座標に双一次でばらまく。`(A_long)` の切り捨ては
    /// 0 へ向かうので、負の座標では重みが 1 を超えたり負になったりする
    /// (PxPer の側で 0 に押さえられる)。ここも元のまま。
    fn add_px_d(&mut self, xd: f64, yd: f64, p: &Px, ops: &Ops) {
        let x0 = xd as i64;
        let y0 = yd as i64;
        let ax1 = xd - x0 as f64;
        let ay1 = yd - y0 as f64;
        let ax0 = 1.0 - ax1;
        let ay0 = 1.0 - ay1;
        let q = ops.px_per(p, ax0 * ay0);
        self.add_px_l(x0, y0, &q, ops);
        let q = ops.px_per(p, ax0 * ay1);
        self.add_px_l(x0, y0 + 1, &q, ops);
        let q = ops.px_per(p, ax1 * ay0);
        self.add_px_l(x0 + 1, y0, &q, ops);
        let q = ops.px_per(p, ax1 * ay1);
        self.add_px_l(x0 + 1, y0 + 1, &q, ops);
    }
}

/// 光1本ぶん。元の StarInfo。
#[derive(Clone, Copy, Debug, Default)]
pub struct Line {
    pub len: i64,
    pub add_x: f64,
    pub add_y: f64,
    pub opa: f64,
}

/// 読み込んだパラメータ。元の ParamInfo + ParamInfo8 の計算済みのぶん。
#[derive(Clone, Debug)]
pub struct Info {
    pub target_kind: i32,
    pub target_border: f64,
    pub target_softness: f64,
    pub target_start: f64,
    pub target_lab: libs::lab::LABA,
    pub mask_draw: bool,
    pub brightness: f64,
    pub color_kind: i32,
    pub colour: Px,
    pub rainbow_offset: f64,
    pub rainbow_delta: f64,
    pub ver: Line,
    pub hor: Line,
    pub dia: Line,
    pub dia2: Line,
    pub blend: i32,
}

/// 元の TargetPixel。対象の強さを出して、黒マットの画素にする。
fn target_pixel(inp: &Px, info: &Info, ops: &Ops) -> Px {
    let maxf = ops.max as f32;
    let a0 = inp.a as f32 / maxf;
    let r = inp.r as f32 * a0 / maxf;
    let g = inp.g as f32 * a0 / maxf;
    let b = inp.b as f32 * a0 / maxf;

    let mut v: f64 = match info.target_kind {
        // 元は float のまま足して 3 で割る。
        2 => ((r + g + b) / 3.0) as f64,
        // 係数が double なので、ここだけ double で計算される。
        3 => 0.29891 * r as f64 + 0.58661 * g as f64 + 0.11448 * b as f64,
        4 => {
            let lab = libs::lab::rgb_to_lab_with(r as f64, g as f64, b as f64, a0, FAITHFUL);
            1.0 - ((lab.l - info.target_lab.l).powi(2)
                + (lab.a - info.target_lab.a).powi(2)
                + (lab.b - info.target_lab.b).powi(2))
            .sqrt()
                / 3.0f64.sqrt()
        }
        _ => r.max(g).max(b) as f64,
    };

    if info.target_softness == 0.0 {
        v = if v >= info.target_border { 1.0 } else { 0.0 };
    } else if v <= info.target_start {
        v = 0.0;
    } else if v >= info.target_border {
        v = 1.0;
    } else {
        v = (v - info.target_start) / info.target_softness;
    }

    // 元はここで float に落としてから 0..1 に押さえる。
    let a = (v as f32).clamp(0.0, 1.0) as f64;

    if ops.float {
        Px {
            a: ops.clip(a),
            r: ops.clip(inp.r * a),
            g: ops.clip(inp.g * a),
            b: ops.clip(inp.b * a),
        }
    } else {
        Px {
            a: ops.clip((a * ops.max + 0.5).trunc()),
            r: ops.clip((inp.r * a + 0.5).trunc()),
            g: ops.clip((inp.g * a + 0.5).trunc()),
            b: ops.clip((inp.b * a + 0.5).trunc()),
        }
    }
}

/// 元の CopyAtoTemp。出力のアルファを下書きの RGB に写す。
fn copy_a_to_temp(out: &Buf, tmp: &mut Buf, ops: &Ops) {
    for i in 0..out.px.len() {
        let a = out.px[i].a;
        tmp.px[i] = Px {
            a: ops.max,
            r: a,
            g: a,
            b: a,
        };
    }
}

/// 元の TempRev。赤と緑だけ反転する (青は元のアルファのまま残る)。
fn temp_rev(tmp: &mut Buf, ops: &Ops) {
    for p in tmp.px.iter_mut() {
        p.r = ops.max - p.r;
        p.g = ops.max - p.g;
    }
}

/// 元の TempSub。緑を戻して、元のアルファ (青) から引く。
fn temp_sub(tmp: &mut Buf, ops: &Ops) {
    for p in tmp.px.iter_mut() {
        p.g = ops.max - p.g;
        p.r = ops.clip(p.b - p.g);
    }
}

/// 元の MaxHor。横に太らせる。緑に書いてから行ごとに赤へ戻すので、
/// 同じ行の中では読む値が変わらない。
fn max_hor(tmp: &mut Buf, ops: &Ops) {
    for y in 0..tmp.h {
        for x in 0..tmp.w {
            let org = tmp.at(x, y).r;
            let mut mx1 = if x > 0 { tmp.at(x - 1, y).r } else { 0.0 };
            let mx2 = if x < tmp.w - 1 {
                tmp.at(x + 1, y).r
            } else {
                0.0
            };
            if mx1 < mx2 {
                mx1 = mx2;
            }
            if org < mx1 {
                let mut v = org + ops.half(mx1);
                if v > mx1 {
                    v = mx1;
                }
                let mut p = tmp.at(x, y);
                p.g = ops.clip(v);
                tmp.set(x, y, p);
            }
        }
        for x in 0..tmp.w {
            let mut p = tmp.at(x, y);
            p.r = p.g;
            tmp.set(x, y, p);
        }
    }
}

/// 元の MaxVer。縦に太らせる。
fn max_ver(tmp: &mut Buf, ops: &Ops) {
    for x in 0..tmp.w {
        for y in 0..tmp.h {
            let org = tmp.at(x, y).r;
            let mut mx1 = if y > 0 { tmp.at(x, y - 1).r } else { 0.0 };
            let mx2 = if y < tmp.h - 1 {
                tmp.at(x, y + 1).r
            } else {
                0.0
            };
            if mx1 < mx2 {
                mx1 = mx2;
            }
            if org < mx1 {
                let mut v = org + ops.half(mx1);
                if v > mx1 {
                    v = mx1;
                }
                let mut p = tmp.at(x, y);
                p.g = ops.clip(v);
                tmp.set(x, y, p);
            }
        }
        for y in 0..tmp.h {
            let mut p = tmp.at(x, y);
            p.r = p.g;
            tmp.set(x, y, p);
        }
    }
}

/// 元の MakePixelTable。長さぶんの色表を作る。`per <= 0` や `len <= 0`
/// なら作らない (元は前の中身が残るが、呼ぶ側が必ず弾いている)。
fn make_pixel_table(len: i64, col: &Px, per: f64, br: f64, ops: &Ops) -> Vec<Px> {
    let mut tbl = vec![Px::default(); len.max(0) as usize];
    if len <= 0 || per <= 0.0 {
        return tbl;
    }
    for i in 0..len {
        let p = per * (1.0 - i as f64 / len as f64) * 0.5;
        tbl[i as usize] = ops.px_per(col, p);
    }
    lift_brightness(&mut tbl, len, br, ops);
    tbl
}

/// 色表の前半を白へ寄せる (MakePixelTable / MakeRainbowTable の後半)。
fn lift_brightness(tbl: &mut [Px], len: i64, br: f64, ops: &Ops) {
    let len2 = len / 2;
    if len2 <= 0 || br <= 0.0 {
        return;
    }
    let top = if ops.float { 1.0 } else { ops.max };
    for i in 0..len2 {
        let t = &mut tbl[i as usize];
        let k = br * (len2 - i) as f64 / len2 as f64;
        t.r = ops.clip(t.r + ops.trunc_int((top - t.r) * k));
        t.g = ops.clip(t.g + ops.trunc_int((top - t.g) * k));
        t.b = ops.clip(t.b + ops.trunc_int((top - t.b) * k));
    }
}

/// 元の MakeRainbowTable。色相を長さに沿って回す。
fn make_rainbow_table(len: i64, ro: f64, rd: f64, per: f64, br: f64, ops: &Ops) -> Vec<Px> {
    let mut tbl = vec![Px::default(); len.max(0) as usize];
    if len <= 0 || per <= 0.0 {
        return tbl;
    }
    for i in 0..len {
        let p = per * (1.0 - i as f64 / len as f64) * 0.5;
        let mut v = ((ro - rd * i as f64) * 65536.0) as i32 % 65536;
        if v < 0 {
            v += 65536;
        }
        let hls = libs::hls::HLSA {
            hue: v as f64 / 65536.0,
            saturation: 1.0,
            lightness: 0.5,
            alpha: 1.0,
        };
        let (r, g, b, a) = hls.to_rgb();
        // 元は 32bit 画素にしてから深さごとに落とす。
        let c = if ops.float {
            Px { a, r, g, b }
        } else {
            let p8 = libs::utils::conv_32_to_8(&ae::PixelF32 {
                alpha: a as f32,
                red: r as f32,
                green: g as f32,
                blue: b as f32,
            });
            if ops.max > 255.0 {
                let p16 = libs::utils::conv_8_to_16(&p8);
                Px {
                    a: p16.alpha as f64,
                    r: p16.red as f64,
                    g: p16.green as f64,
                    b: p16.blue as f64,
                }
            } else {
                Px {
                    a: p8.alpha as f64,
                    r: p8.red as f64,
                    g: p8.green as f64,
                    b: p8.blue as f64,
                }
            }
        };
        tbl[i as usize] = ops.px_per(&c, p);
    }
    lift_brightness(&mut tbl, len, br, ops);
    tbl
}

/// 光の長さぶんのうち、強さ `lv` で実際に描く画素数。
fn seg_len(len: i64, lv: f64, ops: &Ops) -> i64 {
    if ops.float {
        (len as f64 * lv + 0.5) as i64
    } else {
        (len * lv as i64) / ops.max as i64
    }
}

/// 元の StarDrawColor / StarDrawRainbow / StarDrawScreen。
///
/// `tables` が `None` なら Screen の経路で、画素ごとにその色の表を作る。
fn star_draw(out: &mut Buf, tmp: &Buf, info: &Info, tables: Option<&[Vec<Px>; 3]>, ops: &Ops) {
    for y in 0..tmp.h {
        for x in 0..tmp.w {
            let c = tmp.at(x, y);
            // Color / Rainbow は赤 (縁の帯)、Screen はアルファを見る。
            let (lv, lvb) = if tables.is_some() {
                let lv = if ops.float {
                    (c.r as f32).clamp(0.0, 1.0) as f64
                } else {
                    c.r
                };
                (lv, 0.0)
            } else {
                (c.a, c.a / ops.max)
            };
            if lv <= 0.0 {
                continue;
            }
            for which in 0..3usize {
                let line = match which {
                    0 => &info.ver,
                    1 => &info.hor,
                    _ => &info.dia,
                };
                if line.len <= 0 || line.opa <= 0.0 {
                    continue;
                }
                let sl = seg_len(line.len, lv, ops);
                if sl <= 0 {
                    continue;
                }
                let own;
                let tbl: &[Px] = match tables {
                    Some(t) => &t[which],
                    None => {
                        own = make_pixel_table(line.len, &c, line.opa * lvb, info.brightness, ops);
                        &own
                    }
                };
                let d = line.len - sl;
                let (mut x1, mut y1) = (x as f64, y as f64);
                let (mut x2, mut y2) = (x as f64, y as f64);
                let (mut x3, mut y3) = (x as f64, y as f64);
                let (mut x4, mut y4) = (x as f64, y as f64);
                for i in d..line.len {
                    let p = tbl[i as usize];
                    out.add_px_d(x1, y1, &p, ops);
                    out.add_px_d(x2, y2, &p, ops);
                    if which == 2 {
                        out.add_px_d(x3, y3, &p, ops);
                        out.add_px_d(x4, y4, &p, ops);
                        x3 += info.dia2.add_x;
                        y3 += info.dia2.add_y;
                        x4 -= info.dia2.add_x;
                        y4 -= info.dia2.add_y;
                    }
                    x1 += line.add_x;
                    y1 += line.add_y;
                    x2 -= line.add_x;
                    y2 -= line.add_y;
                }
            }
        }
    }
}

/// 元の CenterDraw。縮んだアルファ (緑) のぶんだけ中心を光らせる。
fn center_draw(out: &mut Buf, tmp: &Buf, info: &Info, ops: &Ops) {
    let top = if ops.float { 1.0 } else { ops.max };
    let r = info.colour.r + (top - info.colour.r) * info.brightness;
    let g = info.colour.g + (top - info.colour.g) * info.brightness;
    let b = info.colour.b + (top - info.colour.b) * info.brightness;
    for i in 0..out.px.len() {
        let t = tmp.px[i].g;
        let v = t / ops.max;
        let p = out.px[i];
        out.px[i] = Px {
            a: ops.clip(ops.trunc_int(p.a) + ops.trunc_int(t)),
            r: ops.clip(p.r + r * v),
            g: ops.clip(p.g + g * v),
            b: ops.clip(p.b + b * v),
        };
    }
}

/// 元の blur。3×3 のぼかし (1/32, 1/8, 4/8)。その場で書き戻す。
fn blur(buf: &mut Buf, ops: &Ops) {
    for y in 0..buf.h {
        for x in 0..buf.w {
            let mut s = Px::default();
            let add = |c: Px, d: f64, s: &mut Px| {
                s.r += c.r / d;
                s.g += c.g / d;
                s.b += c.b / d;
                s.a += c.a / d;
            };
            add(buf.get_px(x - 1, y - 1), 32.0, &mut s);
            add(buf.get_px(x, y - 1), 8.0, &mut s);
            add(buf.get_px(x + 1, y - 1), 32.0, &mut s);
            add(buf.get_px(x - 1, y), 8.0, &mut s);
            let c = buf.at(x, y);
            s.r += c.r * 4.0 / 8.0;
            s.g += c.g * 4.0 / 8.0;
            s.b += c.b * 4.0 / 8.0;
            s.a += c.a * 4.0 / 8.0;
            add(buf.get_px(x + 1, y), 8.0, &mut s);
            add(buf.get_px(x - 1, y + 1), 32.0, &mut s);
            add(buf.get_px(x, y + 1), 8.0, &mut s);
            add(buf.get_px(x + 1, y + 1), 32.0, &mut s);
            buf.set(
                x,
                y,
                Px {
                    a: ops.clip(s.a),
                    r: ops.clip(s.r),
                    g: ops.clip(s.g),
                    b: ops.clip(s.b),
                },
            );
        }
    }
}

/// 元の blurTempRed。赤だけ、中心を 2/3 にしたぼかし。
fn blur_temp_red(tmp: &mut Buf, ops: &Ops) {
    const DEC1: f64 = 16.0;
    const DEC2: f64 = 32.0;
    for y in 0..tmp.h {
        for x in 0..tmp.w {
            let mut r = 0.0;
            r += tmp.get_px(x - 1, y - 1).r / DEC2;
            r += tmp.get_px(x, y - 1).r / DEC1;
            r += tmp.get_px(x + 1, y - 1).r / DEC2;
            r += tmp.get_px(x - 1, y).r / DEC1;
            r += tmp.at(x, y).r * 2.0 / 3.0;
            r += tmp.get_px(x + 1, y).r / DEC1;
            r += tmp.get_px(x - 1, y + 1).r / DEC2;
            r += tmp.get_px(x, y + 1).r / DEC1;
            r += tmp.get_px(x + 1, y + 1).r / DEC2;
            // 元は 32bit だけここで 1 に押さえる。
            if ops.float && r > 1.0 {
                r = 1.0;
            }
            let mut p = tmp.at(x, y);
            p.r = ops.clip(r);
            tmp.set(x, y, p);
        }
    }
}

/// 元の ScreenTemp。下書きを「入力の色 + 縁の帯をアルファに」にする。
fn screen_temp(inp: &Buf, tmp: &mut Buf) {
    for i in 0..tmp.px.len() {
        let red = tmp.px[i].r;
        tmp.px[i] = inp.px[i];
        tmp.px[i].a = red;
    }
}

/// 元の BlendScreen / BlendAdd / BlendNormal。
fn blend(out: &mut Buf, inp: &Buf, kind: i32, ops: &Ops) {
    for i in 0..out.px.len() {
        let s = inp.px[i];
        let mut d = out.px[i];
        // Normal は出力のアルファぶんを先に見る。
        let ar = 1.0 - d.a / ops.max;
        let a = s.a / ops.max;
        d.a = ops.scr(d.a, s.a);
        match kind {
            2 => {
                d.r = ops.scr(d.r, ops.trunc_int(s.r * a));
                d.g = ops.scr(d.g, ops.trunc_int(s.g * a));
                d.b = ops.scr(d.b, ops.trunc_int(s.b * a));
            }
            3 => {
                d.r = ops.clip(ops.trunc_int(d.r) + ops.trunc_int(s.r * a));
                d.g = ops.clip(ops.trunc_int(d.g) + ops.trunc_int(s.g * a));
                d.b = ops.clip(ops.trunc_int(d.b) + ops.trunc_int(s.b * a));
            }
            _ => {
                d.r = ops.clip(d.r + s.r * a * ar);
                d.g = ops.clip(d.g + s.g * a * ar);
                d.b = ops.clip(d.b + s.b * a * ar);
            }
        }
        out.px[i] = d;
    }
}

/// 元の fromBlackMat。黒マットの乗算を戻す。
fn from_black_mat(out: &mut Buf, ops: &Ops) {
    let top = if ops.float { 1.0 } else { ops.max };
    for p in out.px.iter_mut() {
        if p.a >= top {
            continue;
        }
        if p.a <= 0.0 {
            p.r = 0.0;
            p.g = 0.0;
            p.b = 0.0;
        } else if ops.float {
            p.r = ops.clip(p.r / p.a);
            p.g = ops.clip(p.g / p.a);
            p.b = ops.clip(p.b / p.a);
        } else {
            let m = ops.max as i64;
            let a = p.a as i64;
            p.r = ((p.r as i64 * m) / a).clamp(0, m) as f64;
            p.g = ((p.g as i64 * m) / a).clamp(0, m) as f64;
            p.b = ((p.b as i64 * m) / a).clamp(0, m) as f64;
        }
    }
}

/// 元の StarExec。手順ぜんぶ。
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, ops: &Ops) {
    for i in 0..out.px.len() {
        out.px[i] = target_pixel(&inp.px[i], info, ops);
    }
    if info.mask_draw {
        from_black_mat(out, ops);
        return;
    }

    let mut tmp = Buf::new(out.w, out.h);
    copy_a_to_temp(out, &mut tmp, ops);
    temp_rev(&mut tmp, ops);
    for _ in 0..3 {
        max_hor(&mut tmp, ops);
        max_ver(&mut tmp, ops);
    }
    temp_sub(&mut tmp, ops);
    blur_temp_red(&mut tmp, ops);

    match info.color_kind {
        3 => {
            out.px.iter_mut().for_each(|p| *p = Px::default());
            let t = [
                make_rainbow_table(
                    info.ver.len,
                    info.rainbow_offset,
                    info.rainbow_delta,
                    info.ver.opa,
                    info.brightness,
                    ops,
                ),
                make_rainbow_table(
                    info.hor.len,
                    info.rainbow_offset,
                    info.rainbow_delta,
                    info.hor.opa,
                    info.brightness,
                    ops,
                ),
                make_rainbow_table(
                    info.dia.len,
                    info.rainbow_offset,
                    info.rainbow_delta,
                    info.dia.opa,
                    info.brightness,
                    ops,
                ),
            ];
            star_draw(out, &tmp, info, Some(&t), ops);
        }
        2 => {
            screen_temp(inp, &mut tmp);
            star_draw(out, &tmp, info, None, ops);
        }
        _ => {
            out.px.iter_mut().for_each(|p| *p = Px::default());
            let t = [
                make_pixel_table(
                    info.ver.len,
                    &info.colour,
                    info.ver.opa,
                    info.brightness,
                    ops,
                ),
                make_pixel_table(
                    info.hor.len,
                    &info.colour,
                    info.hor.opa,
                    info.brightness,
                    ops,
                ),
                make_pixel_table(
                    info.dia.len,
                    &info.colour,
                    info.dia.opa,
                    info.brightness,
                    ops,
                ),
            ];
            star_draw(out, &tmp, info, Some(&t), ops);
            center_draw(out, &tmp, info, ops);
        }
    }
    blur(out, ops);
    if info.blend != 1 {
        blend(out, inp, info.blend, ops);
    }
    from_black_mat(out, ops);
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    TargetTopic,
    TargetKind,
    TargetBorder,
    TargetSoftness,
    TargetColor,
    TargetDisp,
    TargetTopicEnd,
    CtrlTopic,
    Length,
    Opacity,
    Rot,
    AutoRolling,
    RollingSpeed,
    CtrlTopicEnd,
    ColorTopic,
    ColorKind,
    Color,
    Brightness,
    RainbowOffset,
    RainbowDelta,
    ColorTopicEnd,
    DetailTopic,
    VerLength,
    VerRot,
    VerOpacity,
    HorLength,
    HorRot,
    HorOpacity,
    DiaLength,
    DiaRot,
    DiaRot2,
    DiaOpacity,
    DetailTopicEnd,
    Blend,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// 元の PF_ADD_FLOAT_SLIDER (精度1桁) をひとまとめに。
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
        params.add_group(
            Params::TargetTopic,
            Params::TargetTopicEnd,
            "Target",
            true,
            |p| {
                // 元: PF_ADD_POPUP("Target", 4, 1, "RGBMax|R+G+B|Brightness|TargetColor")
                p.add(
                    Params::TargetKind,
                    "Target",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["RGBMax", "R+G+B", "Brightness", "TargetColor"]);
                        f.set_default(1);
                        f.set_value(f.default());
                    }),
                )?;
                fslider(
                    p,
                    Params::TargetBorder,
                    "TargetBorder",
                    50.0,
                    100.0,
                    50.0,
                    100.0,
                    100.0,
                )?;
                fslider(
                    p,
                    Params::TargetSoftness,
                    "TargetSoftness",
                    0.0,
                    100.0,
                    0.0,
                    100.0,
                    100.0,
                )?;
                // 元: PF_ADD_COLOR("TargetColor", 0xFF, 0x00, 0x00)
                p.add(
                    Params::TargetColor,
                    "TargetColor",
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
                // 元: PF_ADD_CHECKBOX("TargetDisp", "on", FALSE)
                p.add(
                    Params::TargetDisp,
                    "TargetDisp",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_label("on");
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            },
        )?;
        params.add_group(
            Params::CtrlTopic,
            Params::CtrlTopicEnd,
            "Control",
            true,
            |p| {
                fslider(p, Params::Length, "Length", 0.0, 600.0, 0.0, 200.0, 100.0)?;
                fslider(p, Params::Opacity, "Opacity", 0.0, 100.0, 0.0, 100.0, 20.0)?;
                // 元: PF_ADD_ANGLE("Rot", 0)
                p.add(
                    Params::Rot,
                    "Rot",
                    ae::AngleDef::setup(|f| {
                        f.set_default(0.0);
                        f.set_value(f.default());
                    }),
                )?;
                p.add(
                    Params::AutoRolling,
                    "AutoRolling",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_label("on");
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                fslider(
                    p,
                    Params::RollingSpeed,
                    "RollingSpeed",
                    -720.0,
                    720.0,
                    -180.0,
                    180.0,
                    90.0,
                )?;
                Ok(())
            },
        )?;
        params.add_group(
            Params::ColorTopic,
            Params::ColorTopicEnd,
            "Color",
            true,
            |p| {
                // 元: PF_ADD_POPUP("ColorKind", 3, 1, "Color|Screen|Rainbow")
                p.add(
                    Params::ColorKind,
                    "ColorKind",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Color", "Screen", "Rainbow"]);
                        f.set_default(1);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_COLOR("Color", 0xFF, 0xFF, 0x00)
                p.add(
                    Params::Color,
                    "Color",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 0xFF,
                            red: 0xFF,
                            green: 0xFF,
                            blue: 0,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                fslider(
                    p,
                    Params::Brightness,
                    "ColorBrightness",
                    0.0,
                    100.0,
                    0.0,
                    20.0,
                    5.0,
                )?;
                fslider(
                    p,
                    Params::RainbowOffset,
                    "RainbowOffset",
                    -30000.0,
                    30000.0,
                    -1.0,
                    1.0,
                    0.0,
                )?;
                fslider(
                    p,
                    Params::RainbowDelta,
                    "RainbowDelta",
                    0.0,
                    100.0,
                    0.0,
                    5.0,
                    1.0,
                )?;
                Ok(())
            },
        )?;
        params.add_group(
            Params::DetailTopic,
            Params::DetailTopicEnd,
            "StarDetail",
            true,
            |p| {
                for (len_id, rot_id, opa_id, name, dflt_len, dflt_rot, dflt_opa) in [
                    (
                        Params::VerLength,
                        Params::VerRot,
                        Params::VerOpacity,
                        "Vertical",
                        70,
                        0.0f32,
                        100.0f64,
                    ),
                    (
                        Params::HorLength,
                        Params::HorRot,
                        Params::HorOpacity,
                        "Horizon",
                        70,
                        90.0,
                        100.0,
                    ),
                    (
                        Params::DiaLength,
                        Params::DiaRot,
                        Params::DiaOpacity,
                        "Diagonal",
                        50,
                        45.0,
                        75.0,
                    ),
                ] {
                    // 元: PF_ADD_SLIDER("...Length", 0, 300, 0, 150, 既定)
                    p.add(
                        len_id,
                        &format!("{name}Length"),
                        ae::SliderDef::setup(move |f| {
                            f.set_valid_min(0);
                            f.set_valid_max(300);
                            f.set_slider_min(0);
                            f.set_slider_max(150);
                            f.set_default(dflt_len);
                            f.set_value(f.default());
                        }),
                    )?;
                    p.add(
                        rot_id,
                        &format!("{name}Rot"),
                        ae::AngleDef::setup(move |f| {
                            f.set_default(dflt_rot);
                            f.set_value(f.default());
                        }),
                    )?;
                    // 元は斜めだけ "Diagonal2Rot" が間に入る。
                    if name == "Diagonal" {
                        p.add(
                            Params::DiaRot2,
                            "Diagonal2Rot",
                            ae::AngleDef::setup(|f| {
                                f.set_default(-45.0);
                                f.set_value(f.default());
                            }),
                        )?;
                    }
                    // 元は縦だけ名前が小文字はじまり ("verticalOpacity")。
                    let opa_name = if name == "Vertical" {
                        "verticalOpacity".to_string()
                    } else {
                        format!("{name}Opacity")
                    };
                    // 元: PF_ADD_FLOAT_SLIDER(..., CURVE_TOLERANCE 0, 既定)
                    p.add(
                        opa_id,
                        &opa_name,
                        ae::FloatSliderDef::setup(move |f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(100.0);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(dflt_opa);
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;
        // 元: PF_ADD_POPUP("Blemnd", 4, 1, "None|Screen|Add|Normal")
        params.add(
            Params::Blend,
            "Blemnd",
            ae::PopupDef::setup(|f| {
                f.set_options(&["None", "Screen", "Add", "Normal"]);
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
                out_data.set_return_msg("fs-rs starcolorful");
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
        // 元の downScale。0 はそのまま。
        let down = |v: i64| -> i64 {
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        // 元の CRot::RoundRot (double 版)。
        let round_rot = |r: f64| -> f64 {
            let mut v = (r * 65536.0 + 0.5) as i32 % (360 * 65536);
            if v < 0 {
                v += 360 * 65536;
            }
            v as f64 / 65536.0
        };
        let flt = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            Ok(params.get(p)?.as_float_slider()?.value() as f64)
        };
        let ang = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f64, Error> {
            // 元の GetANGLE_FLT: 固定小数を 65536 で割る。
            Ok(libs::utils::flt2fix(params.get(p)?.as_angle()?.value() as f64) as f64 / 65536.0)
        };

        // 元の GetParams。
        let target_kind = params.get(Params::TargetKind)?.as_popup()?.value();
        let target_border = flt(Params::TargetBorder, params)? / 100.0;
        let mut target_softness = flt(Params::TargetSoftness, params)? / 100.0;
        let target_colour = params.get(Params::TargetColor)?.as_color()?.value();
        let mask_draw = params.get(Params::TargetDisp)?.as_checkbox()?.value();
        let mut rot = round_rot(ang(Params::Rot, params)?);
        let auto_rolling = params.get(Params::AutoRolling)?.as_checkbox()?.value();
        let rolling_speed = flt(Params::RollingSpeed, params)?;
        let length = flt(Params::Length, params)? / 100.0;
        let opacity = flt(Params::Opacity, params)? / 100.0;
        let brightness = flt(Params::Brightness, params)? / 100.0;
        let color_kind = params.get(Params::ColorKind)?.as_popup()?.value();
        let colour8 = params.get(Params::Color)?.as_color()?.value();
        let rainbow_offset = flt(Params::RainbowOffset, params)?;
        let mut rainbow_delta = flt(Params::RainbowDelta, params)? / 100.0;
        // 元: downSaleValue() は den/num の整数の割り算。
        rainbow_delta *= (ds.den as i64 / ds.num as i64) as f64;
        if rainbow_delta == 0.0 {
            rainbow_delta = 0.001;
        }
        let ver_len = down(params.get(Params::VerLength)?.as_slider()?.value() as i64);
        let ver_rot = round_rot(ang(Params::VerRot, params)?);
        let ver_opa = flt(Params::VerOpacity, params)? / 100.0;
        let hor_len = down(params.get(Params::HorLength)?.as_slider()?.value() as i64);
        let hor_rot = round_rot(ang(Params::HorRot, params)?);
        let hor_opa = flt(Params::HorOpacity, params)? / 100.0;
        let dia_len = down(params.get(Params::DiaLength)?.as_slider()?.value() as i64);
        let dia_rot = round_rot(ang(Params::DiaRot, params)?);
        let dia_rot2 = round_rot(ang(Params::DiaRot2, params)?);
        let dia_opa = flt(Params::DiaOpacity, params)? / 100.0;
        let blend_kind = params.get(Params::Blend)?.as_popup()?.value();

        // 元の ToParam8。
        let target_start = {
            let s = target_border - target_softness;
            if s < 0.0 {
                target_softness = target_border;
                0.0
            } else {
                s
            }
        };
        if auto_rolling && rolling_speed != 0.0 {
            let t = in_data.current_time() as f64 / in_data.time_scale() as f64;
            rot += rolling_speed * t;
        }
        let line = |len: i64, lrot: f64, opa: f64| -> Line {
            let len = (len as f64 * length + 0.5) as i64;
            let (x, y) = if len > 0 {
                libs::shine::rot_vector(round_rot(rot + lrot), len as f64)
            } else {
                (0.0, 0.0)
            };
            Line {
                len,
                add_x: if len > 0 { x / len as f64 } else { 0.0 },
                add_y: if len > 0 { y / len as f64 } else { 0.0 },
                opa: opa * opacity,
            }
        };
        let ver = line(ver_len, ver_rot, ver_opa);
        let hor = line(hor_len, hor_rot, hor_opa);
        let dia = line(dia_len, dia_rot, dia_opa);
        // 斜めの2本目は長さと濃さを斜めと共有し、向きだけ違う。
        let mut dia2 = line(dia_len, dia_rot2, dia_opa);
        dia2.len = dia.len;
        dia2.opa = dia.opa;
        if dia.len <= 0 {
            dia2.add_x = 0.0;
            dia2.add_y = 0.0;
        }

        let depth = out_layer.bit_depth();
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };

        let target_lab = {
            let p = libs::utils::conv_8_to_32(&target_colour);
            libs::lab::rgb_to_lab_with(
                p.red as f64,
                p.green as f64,
                p.blue as f64,
                p.alpha,
                FAITHFUL,
            )
        };
        // 元: 8bit はそのまま、16/32bit は深さへ上げる。
        let colour = match depth {
            8 => Px {
                a: colour8.alpha as f64,
                r: colour8.red as f64,
                g: colour8.green as f64,
                b: colour8.blue as f64,
            },
            16 => {
                let c = libs::utils::conv_8_to_16(&colour8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
            _ => {
                let c = libs::utils::conv_8_to_32(&colour8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
        };

        let info = Info {
            target_kind,
            target_border,
            target_softness,
            target_start,
            target_lab,
            mask_draw,
            brightness,
            color_kind,
            colour,
            rainbow_offset,
            rainbow_delta,
            ver,
            hor,
            dia,
            dia2,
            blend: blend_kind,
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
            target_kind: 1,
            target_border: 1.0,
            target_softness: 1.0,
            target_start: 0.0,
            target_lab: libs::lab::rgb_to_lab(1.0, 0.0, 0.0, 1.0),
            mask_draw: false,
            brightness: 0.0,
            color_kind: 1,
            colour: Px {
                a: 255.0,
                r: 255.0,
                g: 255.0,
                b: 0.0,
            },
            rainbow_offset: 0.0,
            rainbow_delta: 0.01,
            ver: Line::default(),
            hor: Line::default(),
            dia: Line::default(),
            dia2: Line::default(),
            blend: 1,
        }
    }

    /// 白はまるごと対象、黒は対象外。
    #[test]
    fn the_target_picks_the_bright_pixels() {
        let o = ops8();
        let i = info();
        let white = target_pixel(
            &Px {
                a: 255.0,
                r: 255.0,
                g: 255.0,
                b: 255.0,
            },
            &i,
            &o,
        );
        assert_eq!(white.a, 255.0);
        let black = target_pixel(
            &Px {
                a: 255.0,
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            &i,
            &o,
        );
        assert_eq!(black.a, 0.0);
        assert_eq!(black.r, 0.0);
    }

    /// `TargetSoftness` が 0 なら境目で 0/1 に切れる。
    #[test]
    fn a_zero_softness_cuts_hard() {
        let o = ops8();
        let mut i = info();
        i.target_border = 0.5;
        i.target_softness = 0.0;
        let grey = |v: f64| {
            target_pixel(
                &Px {
                    a: 255.0,
                    r: v,
                    g: v,
                    b: v,
                },
                &i,
                &o,
            )
            .a
        };
        assert_eq!(grey(200.0), 255.0);
        assert_eq!(grey(100.0), 0.0);
    }

    /// 縮めてから引くと、残るのは縁の帯だけ。真ん中は 0 になる。
    #[test]
    fn the_edge_band_is_what_is_left() {
        let o = ops8();
        let mut out = Buf::new(11, 11);
        for y in 2..9 {
            for x in 2..9 {
                out.set(
                    x,
                    y,
                    Px {
                        a: 255.0,
                        r: 255.0,
                        g: 255.0,
                        b: 255.0,
                    },
                );
            }
        }
        let mut tmp = Buf::new(11, 11);
        copy_a_to_temp(&out, &mut tmp, &o);
        temp_rev(&mut tmp, &o);
        for _ in 0..3 {
            max_hor(&mut tmp, &o);
            max_ver(&mut tmp, &o);
        }
        temp_sub(&mut tmp, &o);
        assert!(tmp.at(2, 5).r > 0.0, "縁は残る: {:?}", tmp.at(2, 5));
        assert_eq!(tmp.at(5, 5).r, 0.0, "真ん中は消える");
        assert_eq!(tmp.at(0, 0).r, 0.0, "外は何も無い");
        assert!(tmp.at(5, 5).g > 0.0, "縮んだぶんは緑に残る");
    }

    /// 色表は手前がいちばん濃くて、奥へ向かって薄くなる。
    #[test]
    fn the_table_fades_away_from_the_pixel() {
        let o = ops8();
        let col = Px {
            a: 255.0,
            r: 255.0,
            g: 128.0,
            b: 0.0,
        };
        let t = make_pixel_table(8, &col, 1.0, 0.0, &o);
        assert_eq!(t[0].r, 127.0, "不透明度 1 の半分 (切り捨て)");
        assert!(t[1].r < t[0].r);
        assert!(t[7].r < t[4].r);
        assert_eq!(t[0].b, 0.0, "色はそのまま薄くなる");
    }

    /// `ColorBrightness` は前半だけ白へ寄せる。
    #[test]
    fn the_brightness_lifts_only_the_near_half() {
        let o = ops8();
        let col = Px {
            a: 255.0,
            r: 100.0,
            g: 0.0,
            b: 0.0,
        };
        let plain = make_pixel_table(10, &col, 1.0, 0.0, &o);
        let lift = make_pixel_table(10, &col, 1.0, 0.5, &o);
        assert!(lift[0].g > plain[0].g, "手前は白へ寄る");
        assert_eq!(lift[9].g, plain[9].g, "後ろ半分はそのまま");
    }

    /// 虹の表は色相が回る (隣どうしで色が違う)。
    #[test]
    fn the_rainbow_table_turns_the_hue() {
        let o = ops8();
        let t = make_rainbow_table(16, 0.0, 0.05, 1.0, 0.0, &o);
        assert!(t[0] != t[8], "{:?} {:?}", t[0], t[8]);
        assert!(t.iter().any(|p| p.g > 0.0), "赤以外も出る");
    }

    /// 光の長さは縁の強さに比例する。
    #[test]
    fn the_segment_length_follows_the_level() {
        let o = ops8();
        assert_eq!(seg_len(10, 255.0, &o), 10);
        assert_eq!(seg_len(10, 128.0, &o), 5);
        assert_eq!(seg_len(10, 1.0, &o), 0);
    }

    /// 1点の縁から、縦の光が上下に伸びる。
    #[test]
    fn one_point_draws_a_line_both_ways() {
        let o = ops8();
        let mut i = info();
        i.ver = Line {
            len: 5,
            add_x: 0.0,
            add_y: -1.0,
            opa: 1.0,
        };
        let mut tmp = Buf::new(11, 11);
        tmp.set(
            5,
            5,
            Px {
                a: 0.0,
                r: 255.0,
                g: 0.0,
                b: 0.0,
            },
        );
        let mut out = Buf::new(11, 11);
        let t = [
            make_pixel_table(5, &i.colour, i.ver.opa, 0.0, &o),
            vec![],
            vec![],
        ];
        star_draw(&mut out, &tmp, &i, Some(&t), &o);
        assert!(out.at(5, 3).r > 0.0, "上に伸びる");
        assert!(out.at(5, 7).r > 0.0, "下にも伸びる");
        assert_eq!(out.at(2, 5).r, 0.0, "横には出ない");
    }

    /// 黒マットを戻すと、乗算前の色に近いところへ帰る。
    #[test]
    fn the_black_mat_comes_back() {
        let o = ops8();
        let mut b = Buf::new(2, 1);
        b.set(
            0,
            0,
            Px {
                a: 128.0,
                r: 64.0,
                g: 32.0,
                b: 0.0,
            },
        );
        from_black_mat(&mut b, &o);
        assert_eq!(b.at(0, 0).r, 127.0);
        assert_eq!(b.at(0, 0).a, 128.0);
        // アルファ 0 のところは色も消える。
        assert_eq!(b.at(1, 0).r, 0.0);
    }

    /// `TargetDisp` は選び具合だけを出して、光は描かない。
    #[test]
    fn the_target_display_stops_early() {
        let o = ops8();
        let mut i = info();
        i.mask_draw = true;
        i.ver = Line {
            len: 5,
            add_x: 0.0,
            add_y: -1.0,
            opa: 1.0,
        };
        let mut inp = Buf::new(5, 5);
        inp.set(
            2,
            2,
            Px {
                a: 255.0,
                r: 255.0,
                g: 255.0,
                b: 255.0,
            },
        );
        let mut out = Buf::new(5, 5);
        exec(&inp, &mut out, &i, &o);
        assert_eq!(out.at(2, 2).a, 255.0);
        assert_eq!(out.at(2, 0).a, 0.0, "光は伸びていない");
    }
}
