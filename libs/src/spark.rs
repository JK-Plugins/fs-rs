//! 元の `Spark/CPointInfo.h` と `Spark/CLineDraw.h` の移植。
//!
//! Spark / SparkStorm / SparkRing / SparkGrand / SparkMult の5本が共有して
//! いる (5本とも `..\Spark\` 側を include していて、各フォルダにある同名の
//! コピーは古い置き忘れ)。
//!
//! 考え方はこう:
//!
//! 1. 出力ワールドを**真っ白に塗る**
//! 2. 折れ線を描く。描くといっても**赤のチャンネルを暗くするだけ** (太さは
//!    点ごとに持つ)。縁は 1px ぶんのアンチエイリアスが乗る
//! 3. 最後に `colorize` で、赤の暗さをアルファに読み替えて (アルファ =
//!    最大値 - 赤)、RGB を指定の色に塗り替える
//!
//! 乱数は**3本の別々の列**を使う。元の `xorShift` (位置)、`xorShiftM`
//! (折れ線の揺らし)、`xorShiftR` (SparkStorm 系の間引き) で、それぞれ別の
//! 種から始まる。混ぜると絵が変わるので、どの列から引くかは元のまま。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Spark

use crate::fs_graphics::XorShift;

/// 点1つ。`s` は太さ (負だと「ここで終わり」の印)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: i64,
    pub y: i64,
    pub s: f64,
    pub index: i64,
}

impl Default for Point {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            s: -1.0,
            index: -1,
        }
    }
}

impl Point {
    pub fn new(x: i64, y: i64, s: f64) -> Self {
        Self { x, y, s, index: -1 }
    }
}

/// 元の角度の丸め。度を 16bit の固定小数にして 0..360 に収め、戻す。
/// 途中で f32 に落ちるところまで元のまま。
fn wrap_deg(rot: f64) -> f32 {
    let mut rr = (65536.0 * rot + 0.5) as i64;
    rr %= 360 << 16;
    if rr < 0 {
        rr += 360 << 16;
    }
    rr as f32 / 65536.0f32
}

/// 元の CPointInfo。点の並びを持つだけの入れ物。
///
/// 元は `PF_NEW_HANDLE` で取った 1024 個の配列で、`Push` は
/// **`size - 1` 個しか入らない** (最後の1つは「終わり」の印として
/// 残す)。そこも写してある。
#[derive(Debug, Clone)]
pub struct PointList {
    pts: Vec<Point>,
    count: usize,
    /// 元の PointsBak
    bak: Vec<Point>,
    bak_count: usize,
}

impl PointList {
    pub const SIZE_MAX: usize = 1024;

    pub fn new() -> Self {
        Self::with_size(Self::SIZE_MAX)
    }

    pub fn with_size(size: usize) -> Self {
        Self {
            pts: vec![Point::default(); size],
            count: 0,
            bak: vec![Point::default(); size],
            bak_count: 0,
        }
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn size(&self) -> usize {
        self.pts.len()
    }

    pub fn at(&self, i: usize) -> Point {
        self.pts[i]
    }

    pub fn set(&mut self, i: usize, p: Point) {
        self.pts[i] = p;
    }

    pub fn clear(&mut self) {
        self.count = 0;
        for p in self.pts.iter_mut() {
            *p = Point::default();
        }
    }

    /// 元の Push。`size - 1` でいっぱいになる。
    pub fn push(&mut self, p: Point) {
        if self.count >= self.pts.len() - 1 {
            return;
        }
        let mut p = p;
        p.index = self.count as i64;
        self.pts[self.count] = p;
        self.count += 1;
    }

    pub fn first(&self, idx: usize) -> Point {
        self.pts[idx]
    }

    pub fn last(&self, idx: usize) -> Point {
        self.pts[self.count - 1 - idx]
    }

    pub fn swap(&mut self, a: usize, b: usize) {
        if a == b {
            return;
        }
        let (i0, i1) = if a > b { (b, a) } else { (a, b) };
        if i0 >= self.count {
            return;
        }
        self.pts.swap(i0, i1);
        self.pts[i0].index = i0 as i64;
        self.pts[i1].index = i1 as i64;
    }

    /// 元の insert。`idx` が末尾より後ろなら末尾に置くだけ (ずらさない)。
    pub fn insert(&mut self, idx: usize, p: Point) {
        if self.pts.is_empty() || self.count == 0 {
            return;
        }
        if idx >= self.pts.len() {
            return;
        }
        let mut p = p;
        if idx >= self.count {
            p.index = self.count as i64;
            self.pts[self.count] = p;
        } else {
            for i in (idx + 1..=self.count).rev() {
                self.pts[i] = self.pts[i - 1];
                self.pts[i].index = i as i64;
            }
            p.index = idx as i64;
            self.pts[idx] = p;
        }
        self.count += 1;
    }

    /// 元の Center。
    pub fn center(s: Point, d: Point) -> Point {
        Point {
            x: (s.x + d.x) / 2,
            y: (s.y + d.y) / 2,
            s: (s.s + d.s) / 2.0,
            index: -1,
        }
    }

    /// 元の ShiftFromRot。角度 (度) と長さで点をずらす。
    pub fn shift_from_rot(s: Point, rot: f32, len: i64) -> Point {
        let mut ret = s;
        if len <= 0 {
            return ret;
        }
        let rot = wrap_deg(rot as f64);
        // 元: PF_FpShort r = (PF_FpShort)(rot * PF_PI / 180)
        let r = (rot as f64 * std::f64::consts::PI / 180.0) as f32 as f64;
        ret.x += ((len as f32 as f64) * r.cos() + 0.5) as i64;
        ret.y += ((len as f32 as f64) * r.sin() + 0.5) as i64;
        ret
    }

    /// 元の EnFromRot。中心から角度と半径で楕円の上の点を出し、
    /// さらに全体を `rot2` 度回す。
    pub fn en_from_rot(cx: i64, cy: i64, rot: f64, len: i64, aspect: f64, rot2: f64) -> Point {
        let mut ret = Point::new(cx, cy, 0.0);
        if len <= 0 {
            return ret;
        }
        let rot = wrap_deg(rot);
        let r = (rot as f64 * std::f64::consts::PI / 180.0) as f32 as f64;
        ret.x += ((len as f32 as f64) * r.cos() + 0.5) as i64;
        ret.y += ((len as f32 as f64) * r.sin() * aspect + 0.5) as i64;

        if rot2 != 0.0 {
            let dx = (ret.x - cx) as f64;
            let dy = (ret.y - cy) as f64;
            let len2 = dx.hypot(dy);
            let r2 = dy.atan2(dx) * 180.0 / std::f64::consts::PI + rot2;
            let r2 = wrap_deg(r2);
            let r2 = (r2 as f64 * std::f64::consts::PI / 180.0) as f32 as f64;
            ret.x = cx + ((len2 as f32 as f64) * r2.cos() + 0.5) as i64;
            ret.y = cy + ((len2 as f32 as f64) * r2.sin() + 0.5) as i64;
        }
        ret
    }

    /// 元の Random。**ずらし幅が 0 でも乱数を1つ捨てる**ので、
    /// ここを飛ばすと以降の並びが全部ずれる。
    pub fn random(s: Point, rx: i64, ry: i64, rng: &mut XorShift) -> Point {
        let mut ret = s;
        if rx > 0 {
            ret.x += (-(rx as f64) + rx as f64 * 2.0 * rng.next_double()) as i64;
        } else {
            rng.next_double();
        }
        if ry > 0 {
            ret.y += (-(ry as f64) + ry as f64 * 2.0 * rng.next_double()) as i64;
        } else {
            rng.next_double();
        }
        ret
    }

    /// 元の CalcCenterPos。隣り合う点の間に、乱数でずらした中点を挿む
    /// (折れ線を1段細かくする)。乱数は M の列から引く。
    pub fn calc_center_pos(
        &mut self,
        line_size: f64,
        line_move: i64,
        rot_offset: f64,
        depth: i64,
        rng_m: &mut XorShift,
    ) {
        if self.count < 2 {
            return;
        }
        self.calc_center_pos_from(line_size, line_move, rot_offset, depth, 0, rng_m);
    }

    /// 元の CalcCenterPos2。`start` から後ろだけを細かくする。
    pub fn calc_center_pos2(
        &mut self,
        line_size: f64,
        line_move: i64,
        rot_offset: f64,
        depth: i64,
        start: usize,
        rng_m: &mut XorShift,
    ) {
        if self.count < start + 1 {
            return;
        }
        self.calc_center_pos_from(line_size, line_move, rot_offset, depth, start, rng_m);
    }

    fn calc_center_pos_from(
        &mut self,
        line_size: f64,
        line_move: i64,
        rot_offset: f64,
        depth: i64,
        start: usize,
        rng_m: &mut XorShift,
    ) {
        let cnt = self.count;
        if cnt < start + 2 {
            return;
        }
        for i in (start + 1..cnt).rev() {
            let s = self.pts[i - 1];
            let d = self.pts[i];
            let mut c = Point::new((s.x + d.x) / 2, (s.y + d.y) / 2, 0.0);
            let len = (line_move as f64 * rng_m.next_double()) as i64;
            let len = (len as f64 + len as f64 * 0.1 * depth as f64) as i64;

            let mut r = 360.0 * rng_m.next_double() + rot_offset;
            if depth % 2 == 1 {
                r = -r;
            }
            let r = wrap_deg(r);
            let mut sz = line_size;
            sz *= 0.1 + 0.9 * rng_m.next_double();
            sz *= 1.0 - 0.1 * depth as f64;
            // 元はここの `if (sz < 1) sz = 1;` をコメントアウトしている。
            c = Self::shift_from_rot(c, r, len);
            c.s = sz;
            self.insert(i, c);
        }
    }

    /// 元の RemoveTail。
    pub fn remove_tail(&mut self, idx: usize) {
        if self.count == 0 || idx >= self.count {
            return;
        }
        for i in idx..self.count {
            self.pts[i] = Point::default();
        }
        self.count = idx;
    }

    /// 元の RotFramPoints。**x と y を逆に渡している** (atan2(dx, dy))
    /// ので、ここで返るのは「上向きを 0 とした時計回りの角度」。
    pub fn rot_from_points(s: Point, d: Point) -> f64 {
        let dx = (d.x - s.x) as f64;
        let dy = (d.y - s.y) as f64;
        let mut r = dx.atan2(dy);
        if r < 0.0 {
            r += 2.0 * std::f64::consts::PI;
        }
        r * 180.0 / std::f64::consts::PI
    }

    /// 元の CalcRotPos。`s` のまわりに `d` を `ar` 度回す。
    pub fn calc_rot_pos(s: Point, d: Point, ar: f64) -> Point {
        let mut d = d;
        let dx = (d.x - s.x) as f64;
        let dy = (d.y - s.y) as f64;
        let mut r = dy.atan2(dx);
        if r < 0.0 {
            r += 2.0 * std::f64::consts::PI;
        }
        r = r * 180.0 / std::f64::consts::PI + ar;
        r = r * std::f64::consts::PI / 180.0;
        let len = (dx * dx + dy * dy).sqrt();
        d.x = s.x + (len * r.cos() + 0.5) as i64;
        d.y = s.y + (len * r.sin() + 0.5) as i64;
        d
    }

    /// 元の Wipe。折れ線の途中までで切る (0..1 は後ろから、1..2 は前から)。
    pub fn wipe(&mut self, par: f64) {
        if par == 1.0 || self.count == 0 {
            return;
        }
        if par <= 0.0 || par >= 2.0 {
            self.pts[0].s = -1.0;
            self.pts[1].s = -1.0;
            return;
        }
        let c = (self.count - 1) as f64;
        if par > 0.0 && par < 1.0 {
            let idx = (c * par) as usize;
            let idx_p = c * par - idx as f64;
            self.pts[idx + 1].s *= idx_p;
            self.pts[idx + 2].s = -1.0;
        } else {
            let idx = (c * (par - 1.0)) as usize;
            let idx_p = c * (par - 1.0) - idx as f64;
            for i in 0..idx {
                self.pts[i].s = 0.0;
            }
            self.pts[idx].s *= 1.0 - idx_p;
        }
    }

    /// 元の PointInterRandom。p0 から p1 へ向かって乱数ぶん進んだ点
    /// (M の列)。太さは入れない。
    pub fn point_inter_random(p0: Point, p1: Point, rng_m: &mut XorShift) -> Point {
        Self::point_inter_rand_with(p0, p1, rng_m)
    }

    /// 元の PointInterRandomR (R の列)。
    pub fn point_inter_random_r(p0: Point, p1: Point, rng_r: &mut XorShift) -> Point {
        Self::point_inter_rand_with(p0, p1, rng_r)
    }

    fn point_inter_rand_with(p0: Point, p1: Point, rng: &mut XorShift) -> Point {
        let dx = (p1.x - p0.x) as f64;
        let dy = (p1.y - p0.y) as f64;
        let len = dx.hypot(dy) * rng.next_double();
        let r = dy.atan2(dx);
        Point {
            x: p0.x + (r.cos() * len + 0.5) as i64,
            y: p0.y + (r.sin() * len + 0.5) as i64,
            s: 0.0,
            index: -1,
        }
    }

    /// 元の PointStrom。4点の表 (始点の線分と終点の線分) から、同じ乱数を
    /// 元にした始点と終点を出す (R の列)。
    pub fn point_strom(tbl: &[Point; 4], rng_r: &mut XorShift) -> (Point, Point) {
        let sdx = (tbl[1].x - tbl[0].x) as f64;
        let sdy = (tbl[1].y - tbl[0].y) as f64;
        let ldx = (tbl[3].x - tbl[2].x) as f64;
        let ldy = (tbl[3].y - tbl[2].y) as f64;

        let len_start = sdx.hypot(sdy);
        let len_last = ldx.hypot(ldy);
        let sr = sdy.atan2(sdx);
        let lr = ldy.atan2(ldx);

        let mut rnd = rng_r.next_double();
        let ls = len_start * rnd;
        let st = Point {
            x: tbl[0].x + (sr.cos() * ls + 0.5) as i64,
            y: tbl[0].y + (sr.sin() * ls + 0.5) as i64,
            s: 0.0,
            index: -1,
        };
        rnd += -0.1 + 0.2 * rng_r.next_double();
        rnd = rnd.clamp(0.0, 1.0);
        let ll = len_last * rnd;
        let lt = Point {
            x: tbl[2].x + (lr.cos() * ll + 0.5) as i64,
            y: tbl[2].y + (lr.sin() * ll + 0.5) as i64,
            s: 0.0,
            index: -1,
        };
        (st, lt)
    }

    /// 元の PointInter。ただの中点。
    pub fn point_inter(p0: Point, p1: Point) -> Point {
        Point {
            x: p0.x + (p1.x - p0.x) / 2,
            y: p0.y + (p1.y - p0.y) / 2,
            s: 0.0,
            index: -1,
        }
    }

    /// 元の Backup。
    pub fn backup(&mut self) {
        self.bak.copy_from_slice(&self.pts);
        self.bak_count = self.count;
    }

    pub fn count_bak(&self) -> usize {
        self.bak_count
    }

    pub fn bak_at(&self, i: usize) -> Point {
        self.bak[i]
    }
}

impl Default for PointList {
    fn default() -> Self {
        Self::new()
    }
}

/// 下書き用の「赤のチャンネル」。深度ごとの量子化を抱えている。
pub trait Chan: Copy {
    /// FillWhite の値
    fn white() -> Self;
    /// 元の BlendPX*。`cov` は 0.0..=1.0 の「残す割合」
    /// (0 なら真っ黒に塗り、1 なら何もしない)。
    fn blend(self, cov: f64) -> Self;
    /// Colorize のアルファ (最大値 - 赤)。
    fn to_alpha(self) -> Self;
}

impl Chan for u8 {
    fn white() -> Self {
        255
    }
    fn blend(self, cov: f64) -> Self {
        // 元: v は `(A_u_char)(cov * PF_MAX_CHAN8)`。画面の左や上の縁では
        // cov が 0..1 の外に出ることがあり (x0 が負の小数のとき
        // `1 - (x0 - (A_long)x0)` が 1 を超える)、そのとき元は double →
        // int → unsigned char と落として**折り返す**。飽和させると
        // 縁の数画素が元とずれるので、折り返しをそのまま写している。
        let v = ((cov * 255.0) as i32) as u8 as i64;
        (self as i64 * v / 255).clamp(0, 255) as u8
    }
    fn to_alpha(self) -> Self {
        255 - self
    }
}

impl Chan for u16 {
    fn white() -> Self {
        32768
    }
    fn blend(self, cov: f64) -> Self {
        // 8bit と同じく (A_u_short) への折り返し。
        let v = ((cov * 32768.0) as i32) as u16 as i64;
        (self as i64 * v / 32768).clamp(0, 32768) as u16
    }
    fn to_alpha(self) -> Self {
        32768 - self
    }
}

impl Chan for f32 {
    fn white() -> Self {
        1.0
    }
    fn blend(self, cov: f64) -> Self {
        // 元: RoundFpShort は 0..32 で切る。
        (self * cov as f32).clamp(0.0, 32.0)
    }
    fn to_alpha(self) -> Self {
        1.0 - self.min(1.0)
    }
}

/// 元の CLineDraw。白い下書きに折れ線を描く。
pub struct LineDraw<C: Chan> {
    pub w: i64,
    pub h: i64,
    /// 赤のチャンネルだけの下書き (FillWhite 済み)
    pub buf: Vec<C>,
    pub points: PointList,
}

impl<C: Chan> LineDraw<C> {
    pub fn new(w: i64, h: i64) -> Self {
        Self::with_point_size(w, h, PointList::SIZE_MAX)
    }

    /// 元の CLineDraw(.., bufSize)。SparkMult だけ 2048 を渡している。
    pub fn with_point_size(w: i64, h: i64, size: usize) -> Self {
        Self {
            w,
            h,
            buf: vec![C::white(); (w * h).max(0) as usize],
            points: PointList::with_size(size.max(256)),
        }
    }

    /// 元の FillWhite。
    pub fn fill_white(&mut self) {
        for p in self.buf.iter_mut() {
            *p = C::white();
        }
    }

    fn blend_px(&mut self, x: i64, y: i64, cov: f64) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return;
        }
        let i = (x + y * self.w) as usize;
        self.buf[i] = self.buf[i].blend(cov);
    }

    /// 元の Xline*。両端だけ 1px のアンチエイリアス、中は塗り潰し。
    pub fn xline(&mut self, x0: f64, x1: f64, y: i64) {
        let (x0, x1) = if x0 > x1 { (x1, x0) } else { (x0, x1) };
        if x1 - x0 <= 0.0 {
            return;
        }
        let xi0 = x0 as i64;
        let xv0 = 1.0 - (x0 - xi0 as f64);
        let xi1 = x1 as i64;
        let xv1 = x1 - xi1 as f64;
        let dxi = xi1 - xi0;
        if dxi == 0 {
            // 元: 1画素に収まるときだけ 2つの端の割合を掛ける。
            self.blend_px(xi0, y, 1.0 - xv0 * xv1);
        } else {
            self.blend_px(xi0, y, 1.0 - xv0);
            self.blend_px(xi1, y, 1.0 - xv1);
            if dxi == 1 {
            } else if dxi == 2 {
                self.blend_px(xi0 + 1, y, 0.0);
            } else {
                for i in xi0 + 1..=xi1 - 1 {
                    self.blend_px(i, y, 0.0);
                }
            }
        }
    }

    /// 元の Yline*。
    pub fn yline(&mut self, x: i64, y0: f64, y1: f64) {
        let (y0, y1) = if y0 > y1 { (y1, y0) } else { (y0, y1) };
        if y1 - y0 <= 0.0 {
            return;
        }
        let yi0 = y0 as i64;
        let yv0 = 1.0 - (y0 - yi0 as f64);
        let yi1 = y1 as i64;
        let yv1 = y1 - yi1 as f64;
        let dyi = yi1 - yi0;
        if dyi == 0 {
            self.blend_px(x, yi0, 1.0 - yv0 * yv1);
        } else {
            self.blend_px(x, yi0, 1.0 - yv0);
            self.blend_px(x, yi1, 1.0 - yv1);
            if dyi == 1 {
            } else if dyi == 2 {
                self.blend_px(x, yi0 + 1, 0.0);
            } else {
                for i in yi0 + 1..=yi1 - 1 {
                    self.blend_px(x, i, 0.0);
                }
            }
        }
    }

    /// 元の Fill_EN。点のところに円を塗る (直径は太さの 0.85 倍)。
    pub fn fill_en(&mut self, p: Point) {
        // 元: p.s * 0.85 / 2。
        let r2 = p.s * 0.85 / 2.0;
        let ri = (r2 + 0.5) as i64;
        if ri <= 0 {
            return;
        }
        for i in 0..ri {
            let xx = ((ri * ri - i * i) as f64).sqrt();
            let x0 = p.x as f64 - xx;
            let x1 = p.x as f64 + xx;
            self.xline(x0, x1, p.y + i);
            self.xline(x0, x1, p.y - i);
        }
    }

    /// 元の Line(start, last)。太さを端から端へ線形に変えながら描く。
    pub fn line(&mut self, start: Point, last: Point) {
        // 元: 両端の太さがどちらも 0 以下なら描かない。
        if start.s <= 0.0 && last.s <= 0.0 {
            return;
        }
        let dx = (last.x - start.x).abs();
        let dy = (last.y - start.y).abs();
        let gx: i64 = if last.x < start.x { -1 } else { 1 };
        let gy: i64 = if last.y < start.y { -1 } else { 1 };
        let st = start.s / 2.0;
        let lt = last.s / 2.0;

        if dx >= dy {
            if dx == 0 {
                return;
            }
            let ay = (dy as f64 / dx as f64) * gy as f64;
            let mut x = start.x;
            let mut y = start.y as f64;
            for i in 0..dx {
                let sz = st + (lt - st) * i as f64 / dx as f64;
                self.yline(x, y - sz, y + sz);
                x += gx;
                y += ay;
            }
        } else {
            let ax = (dx as f64 / dy as f64) * gx as f64;
            let mut y = start.y;
            let mut x = start.x as f64;
            for i in 0..dy {
                let sz = st + (lt - st) * i as f64 / dy as f64;
                self.xline(x - sz, x + sz, y);
                x += ax;
                y += gy;
            }
        }
    }

    /// 元の Line(CPointInfo)。点の並びをつないで描く。太さが負の点に
    /// 当たったところで打ち切る。
    pub fn line_all(&mut self) {
        let n = self.points.count();
        if n == 0 {
            return;
        }
        for i in 0..n {
            let s = self.points.at(i);
            // 元: 太さが負なら抜ける。
            if s.s < 0.0 {
                break;
            }
            self.fill_en(s);
            let d = self.points.at(i + 1);
            if d.s < 0.0 {
                break;
            }
            self.line(s, d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_push_leaves_the_last_slot_as_the_end_marker() {
        let mut l = PointList::with_size(4);
        for i in 0..10 {
            l.push(Point::new(i, i, 1.0));
        }
        assert_eq!(l.count(), 3);
        assert_eq!(l.at(3).s, -1.0, "最後は終わりの印のまま");
    }

    #[test]
    fn the_insert_pushes_the_rest_back() {
        let mut l = PointList::with_size(8);
        l.push(Point::new(0, 0, 1.0));
        l.push(Point::new(10, 0, 2.0));
        l.insert(1, Point::new(5, 5, 3.0));
        assert_eq!(l.count(), 3);
        assert_eq!((l.at(0).x, l.at(1).x, l.at(2).x), (0, 5, 10));
    }

    /// 0 度は右、90 度は下 (画面の座標)。
    #[test]
    fn the_angles_point_the_way_the_original_did() {
        let p = PointList::shift_from_rot(Point::new(0, 0, 1.0), 0.0, 10);
        assert_eq!((p.x, p.y), (10, 0));
        let p = PointList::shift_from_rot(Point::new(0, 0, 1.0), 90.0, 10);
        assert_eq!((p.x, p.y), (0, 10));
        // -90 度は 270 度に回る。元は `(A_long)(len * sin + 0.5)` で
        // 0 方向へ切り捨てるので、マイナス側は 1 足りなくなる。
        let p = PointList::shift_from_rot(Point::new(0, 0, 1.0), -90.0, 10);
        assert_eq!((p.x, p.y), (0, -9));
    }

    /// aspect は縦だけに掛かる。
    #[test]
    fn the_aspect_squashes_only_the_height() {
        let p = PointList::en_from_rot(100, 100, 90.0, 20, 0.5, 0.0);
        assert_eq!((p.x, p.y), (100, 110));
        let p = PointList::en_from_rot(100, 100, 0.0, 20, 0.5, 0.0);
        assert_eq!((p.x, p.y), (120, 100));
    }

    /// ずらし幅が 0 でも乱数を2つ使う (並びを合わせるため)。
    #[test]
    fn the_random_always_draws_two() {
        let mut a = XorShift::new(1);
        let mut b = XorShift::new(1);
        let p = PointList::random(Point::new(0, 0, 1.0), 0, 0, &mut a);
        assert_eq!((p.x, p.y), (0, 0));
        b.next_double();
        b.next_double();
        assert_eq!(a.next_double(), b.next_double());
    }

    /// 中点を挿むと点の数が倍々に増える。
    #[test]
    fn the_folding_doubles_the_points() {
        let mut l = PointList::new();
        l.push(Point::new(0, 0, 10.0));
        l.push(Point::new(100, 100, 10.0));
        let mut rng = XorShift::new(3);
        l.calc_center_pos(10.0, 20, 0.0, 0, &mut rng);
        assert_eq!(l.count(), 3);
        l.calc_center_pos(10.0, 20, 0.0, 1, &mut rng);
        assert_eq!(l.count(), 5);
    }

    /// 下書きは白から始まって、塗ったところが 0 になる。
    #[test]
    fn the_drawing_darkens_the_sketch() {
        let mut ld: LineDraw<u8> = LineDraw::new(16, 16);
        assert!(ld.buf.iter().all(|v| *v == 255));
        ld.xline(2.0, 10.0, 8);
        let row = &ld.buf[(8 * 16) as usize..(9 * 16) as usize];
        assert_eq!(row[5], 0, "中は真っ黒");
        assert_eq!(row[0], 255, "外はそのまま");
    }

    /// アルファは「白からどれだけ暗くなったか」。
    #[test]
    fn the_alpha_comes_from_the_darkness() {
        assert_eq!(<u8 as Chan>::to_alpha(255), 0);
        assert_eq!(<u8 as Chan>::to_alpha(0), 255);
        assert_eq!(<u16 as Chan>::to_alpha(32768), 0);
        assert_eq!(<f32 as Chan>::to_alpha(1.0), 0.0);
        assert_eq!(<f32 as Chan>::to_alpha(0.0), 1.0);
    }

    /// 太さが負の点で打ち切る。
    #[test]
    fn the_line_stops_at_a_negative_size() {
        let mut ld: LineDraw<u8> = LineDraw::new(32, 32);
        ld.points.push(Point::new(2, 16, 4.0));
        ld.points.push(Point::new(14, 16, 4.0));
        // 3つめは太さが負なので、ここから先は描かない。
        ld.points.push(Point::new(30, 16, -1.0));
        ld.line_all();
        assert!(ld.buf[(8 + 16 * 32) as usize] < 255, "最初の線は出る");
        assert_eq!(ld.buf[(24 + 16 * 32) as usize], 255, "2本目は出ない");
    }
}

// ------------------------------------------------------- 入力との重ね合わせ

/// 元の RedBlendIn8/16/32。
///
/// 描いた火花 (`out`) を入力 (`inp`) の上に乗せる。どちらもストレート
/// アルファなので、いったんアルファを掛けて混ぜ、混ざったアルファで
/// 割り戻す。火花のアルファが 0 の画素は入力をそのまま通す。
pub fn red_blend_in8(inp: after_effects::Pixel8, out: after_effects::Pixel8) -> after_effects::Pixel8 {
    if out.alpha == 0 {
        return inp;
    }
    let v0 = inp.alpha as f64 / 255.0;
    let v1 = out.alpha as f64 / 255.0;
    let mix = |i: u8, o: u8| {
        let a = i as f64 * v0;
        let b = o as f64 * v1;
        a * (1.0 - v1) + b * v1
    };
    let (r, g, b) = (
        mix(inp.red, out.red),
        mix(inp.green, out.green),
        mix(inp.blue, out.blue),
    );
    let va = v0 + v1 - v0 * v1;
    after_effects::Pixel8 {
        alpha: crate::utils::round_byte_fp_double(va * 255.0),
        red: crate::utils::round_byte_fp_double(r / va),
        green: crate::utils::round_byte_fp_double(g / va),
        blue: crate::utils::round_byte_fp_double(b / va),
    }
}

pub fn red_blend_in16(
    inp: after_effects::Pixel16,
    out: after_effects::Pixel16,
) -> after_effects::Pixel16 {
    if out.alpha == 0 {
        return inp;
    }
    let v0 = inp.alpha as f64 / 32768.0;
    let v1 = out.alpha as f64 / 32768.0;
    let mix = |i: u16, o: u16| {
        let a = i as f64 * v0;
        let b = o as f64 * v1;
        a * (1.0 - v1) + b * v1
    };
    let (r, g, b) = (
        mix(inp.red, out.red),
        mix(inp.green, out.green),
        mix(inp.blue, out.blue),
    );
    let va = v0 + v1 - v0 * v1;
    after_effects::Pixel16 {
        alpha: crate::utils::round_short_fp_double(va * 32768.0),
        red: crate::utils::round_short_fp_double(r / va),
        green: crate::utils::round_short_fp_double(g / va),
        blue: crate::utils::round_short_fp_double(b / va),
    }
}

pub fn red_blend_in32(
    inp: after_effects::PixelF32,
    out: after_effects::PixelF32,
) -> after_effects::PixelF32 {
    if out.alpha <= 0.0 {
        return inp;
    }
    // 元: 1 を超えるアルファは 1 で切る。
    let v0 = (inp.alpha as f64).min(1.0);
    let v1 = (out.alpha as f64).min(1.0);
    let mix = |i: f32, o: f32| {
        let a = i as f64 * v0;
        let b = o as f64 * v1;
        a * (1.0 - v1) + b * v1
    };
    let (r, g, b) = (
        mix(inp.red, out.red),
        mix(inp.green, out.green),
        mix(inp.blue, out.blue),
    );
    let va = v0 + v1 - v0 * v1;
    after_effects::PixelF32 {
        alpha: crate::utils::round_fp_short_double(va),
        red: crate::utils::round_fp_short_double(r / va),
        green: crate::utils::round_fp_short_double(g / va),
        blue: crate::utils::round_fp_short_double(b / va),
    }
}
