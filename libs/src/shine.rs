// ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Shine
//      https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/ShineParallel
//
//! `Shine` と `ShineParallel` が共通で持っている光条 (シャイン) の部品。
//!
//! 絵を**縦横半分に縮めた下書き**を作り、画素ごとに「光の向き」へ下書きを
//! 舐めながら足し合わせて、元の大きさへ戻す。向きが中心へ向かうのが
//! `Shine`、一定の角度なのが `ShineParallel`。
//!
//! AE の型に依存しない (画素は f64 で持つ)。

/// 1画素。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 深度ごとの作法。
#[derive(Debug, Clone, Copy)]
pub struct Ops {
    /// 8bit なら 255、16bit なら 32768、32bit なら 1.0。
    pub max: f64,
    pub float: bool,
}

impl Ops {
    /// 元の RoundByteFpLong / RoundShortFpLong / RoundFpShortDouble。
    /// どれも丸めずに切り捨てて押さえる (32bit の上は 32.0)。
    pub fn round(&self, v: f64) -> f64 {
        if self.float {
            v.clamp(0.0, 32.0)
        } else {
            v.trunc().clamp(0.0, self.max)
        }
    }
    /// 元の PMat。乗算済みにする。8/16bit は `+0.5` してから切り捨て
    /// (つまり四捨五入)、32bit は掛けるだけ。
    pub fn premultiply(&self, p: &Px) -> Px {
        if self.float {
            Px {
                a: p.a,
                r: self.round(p.r * p.a),
                g: self.round(p.g * p.a),
                b: self.round(p.b * p.a),
            }
        } else {
            let f = |v: f64| self.round(v * p.a / self.max + 0.5);
            Px {
                a: p.a,
                r: f(p.r),
                g: f(p.g),
                b: f(p.b),
            }
        }
    }
    /// 元の gray8/16。乗算済みの画素に掛ける。
    pub fn gray(&self, p: &Px) -> f64 {
        0.29891 * p.r + 0.58661 * p.g + 0.11448 * p.b
    }
    /// 元の FromMat。乗算を戻す。
    pub fn from_mat(&self, p: &Px) -> Px {
        if p.a == 0.0 {
            return Px::default();
        }
        if p.a >= self.max {
            return *p;
        }
        let a = p.a / self.max;
        Px {
            a: p.a,
            r: self.round(p.r / a),
            g: self.round(p.g / a),
            b: self.round(p.b / a),
        }
    }
}

/// 画素の並び。
#[derive(Clone)]
pub struct Buf {
    pub w: i64,
    pub h: i64,
    pub px: Vec<Px>,
}

impl Buf {
    pub fn new(w: i64, h: i64) -> Self {
        Self {
            w,
            h,
            px: vec![Px::default(); (w * h).max(0) as usize],
        }
    }
    /// 元の GetPix。縁で押さえる。
    pub fn get(&self, x: i64, y: i64) -> Px {
        if self.w <= 0 || self.h <= 0 {
            return Px::default();
        }
        let x = x.clamp(0, self.w - 1);
        let y = y.clamp(0, self.h - 1);
        self.px[(x + y * self.w) as usize]
    }
    pub fn set(&mut self, x: i64, y: i64, p: Px) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        self.px[(x + y * self.w) as usize] = p;
    }
    /// 元の GetPixD。双一次で拾う。
    pub fn get_d(&self, x: f64, y: f64, ops: &Ops) -> Px {
        let ax = x as i64;
        let ay = y as i64;
        let dx1 = x - ax as f64;
        let dy1 = y - ay as f64;
        let dx0 = 1.0 - dx1;
        let dy0 = 1.0 - dy1;
        let a = self.get(ax, ay);
        let b = self.get(ax + 1, ay);
        let c = self.get(ax, ay + 1);
        let d = self.get(ax + 1, ay + 1);
        let f = |pa: f64, pb: f64, pc: f64, pd: f64| {
            pa * dx0 * dy0 + pb * dx1 * dy0 + pc * dx0 * dy1 + pd * dx1 * dy1
        };
        Px {
            a: ops.round(f(a.a, b.a, c.a, d.a)),
            r: ops.round(f(a.r, b.r, c.r, d.r)),
            g: ops.round(f(a.g, b.g, c.g, d.g)),
            b: ops.round(f(a.b, b.b, c.b, d.b)),
        }
    }
    /// 元の GetPixDA。アルファだけ双一次で拾い、RGB は最大にする。
    pub fn get_da(&self, x: f64, y: f64, ops: &Ops) -> Px {
        let p = self.get_d(x, y, ops);
        Px {
            a: p.a,
            r: ops.max,
            g: ops.max,
            b: ops.max,
        }
    }
}

/// 光の向き。
#[derive(Debug, Clone, Copy)]
pub enum Dir {
    /// `Shine`: 下書きの座標で見た中心へ向かう。
    ToPoint { cx: f64, cy: f64 },
    /// `ShineParallel`: 一定の向き (長さ `length` のベクトル)。
    Parallel { dx: f64, dy: f64 },
}

/// 元の CopyIntoBuf1。4点のうちいちばん明るい乗算済みの画素を拾う。
pub fn copy_into_buf1(src: &Buf, dst: &mut Buf, ops: &Ops) {
    for y in 0..dst.h {
        for x in 0..dst.w {
            let p: [Px; 4] = [
                ops.premultiply(&src.get(2 * x, 2 * y)),
                ops.premultiply(&src.get(2 * x + 1, 2 * y)),
                ops.premultiply(&src.get(2 * x, 2 * y + 1)),
                ops.premultiply(&src.get(2 * x + 1, 2 * y + 1)),
            ];
            let mut idx = 0usize;
            let mut vmax = p[0].r + p[0].g + p[0].b;
            for i in 1..4 {
                let v = p[i].r + p[i].g + p[i].b;
                if v > vmax {
                    idx = i;
                    vmax = v;
                }
            }
            let mut c = p[idx];
            c.a = ops.max;
            dst.set(x, y, c);
        }
    }
}

/// 元の CopyIntoBuf1G。4点のうちいちばん明るい灰色の値をアルファに入れ、
/// RGB は最大にする。
pub fn copy_into_buf1_gray(src: &Buf, dst: &mut Buf, ops: &Ops) {
    for y in 0..dst.h {
        for x in 0..dst.w {
            let g: [f64; 4] = [
                ops.gray(&ops.premultiply(&src.get(2 * x, 2 * y))),
                ops.gray(&ops.premultiply(&src.get(2 * x + 1, 2 * y))),
                ops.gray(&ops.premultiply(&src.get(2 * x, 2 * y + 1))),
                ops.gray(&ops.premultiply(&src.get(2 * x + 1, 2 * y + 1))),
            ];
            let mut vmax = g[0];
            for i in 1..4 {
                if g[i] > vmax {
                    vmax = g[i];
                }
            }
            dst.set(
                x,
                y,
                Px {
                    a: ops.round(vmax),
                    r: ops.max,
                    g: ops.max,
                    b: ops.max,
                },
            );
        }
    }
}

/// その画素での光の向きと、中心までの距離 (元の `len`)。
fn ray(dir: &Dir, x: i64, y: i64) -> (f64, f64, i64) {
    match *dir {
        Dir::ToPoint { cx, cy } => {
            let len_d = ((cx - x as f64).powi(2) + (cy - y as f64).powi(2)).sqrt();
            let len = (len_d + 0.5) as i64;
            if len <= 0 {
                return (0.0, 0.0, len);
            }
            ((cx - x as f64) / len as f64, (cy - y as f64) / len as f64, len)
        }
        Dir::Parallel { dx, dy } => {
            // 長さ `length` のベクトルなので、1歩ぶんに割る。
            let len_d = (dx * dx + dy * dy).sqrt();
            let len = (len_d + 0.5) as i64;
            if len <= 0 {
                return (0.0, 0.0, len);
            }
            (dx / len as f64, dy / len as f64, len)
        }
    }
}

/// 元の sub8/16/32。RGB の光条。
pub fn sub(src: &Buf, dst: &mut Buf, dir: &Dir, length: i64, strong: f64, ops: &Ops) {
    let vlen = (length as f64 / 2.0 + 0.5) as i64;
    if vlen == 0 {
        return;
    }
    for y in 0..src.h {
        for x in 0..src.w {
            let inp = src.get(x, y);
            let (dx, dy, len) = ray(dir, x, y);
            if len <= 0 {
                let f = |v: f64| ops.round(v * strong);
                let (r, g, b) = (f(inp.r), f(inp.g), f(inp.b));
                dst.set(
                    x,
                    y,
                    Px {
                        a: r.max(g).max(b),
                        r,
                        g,
                        b,
                    },
                );
                continue;
            }
            let len2 = vlen.min(len);
            let (mut r, mut g, mut b) = (inp.r, inp.g, inp.b);
            if len2 > 0 {
                let mut xd = x as f64 + dx;
                let mut yd = y as f64 + dy;
                for i in 1..len2 {
                    let c = src.get_d(xd, yd, ops);
                    let av = (len2 - i) as f64 / len2 as f64;
                    r += c.r * av;
                    g += c.g * av;
                    b += c.b * av;
                    xd += dx;
                    yd += dy;
                }
                r = r * strong / len2 as f64;
                g = g * strong / len2 as f64;
                b = b * strong / len2 as f64;
            }
            let (r, g, b) = (ops.round(r), ops.round(g), ops.round(b));
            dst.set(
                x,
                y,
                Px {
                    a: r.max(g).max(b),
                    r,
                    g,
                    b,
                },
            );
        }
    }
}

/// 元の subG8/16/32。アルファだけの光条を指定色で塗る。
pub fn sub_gray(
    src: &Buf,
    dst: &mut Buf,
    dir: &Dir,
    length: i64,
    strong: f64,
    colour: &Px,
    ops: &Ops,
) {
    let vlen = (length as f64 / 2.0 + 0.5) as i64;
    if vlen <= 0 {
        return;
    }
    for y in 0..src.h {
        for x in 0..src.w {
            let inp = src.get(x, y);
            let (dx, dy, len) = ray(dir, x, y);
            if len <= 0 {
                let mut c = *colour;
                c.a = ops.round(inp.a * strong);
                dst.set(x, y, c);
                continue;
            }
            let len2 = vlen.min(len);
            let mut a = inp.a;
            if len2 > 0 {
                let mut xd = x as f64 + dx;
                let mut yd = y as f64 + dy;
                for i in 1..len2 {
                    let c = src.get_da(xd, yd, ops);
                    let av = (len2 - i) as f64 / len2 as f64;
                    a += c.a * av;
                    xd += dx;
                    yd += dy;
                }
            }
            let mut c = *colour;
            c.a = ops.round(a * strong / len2 as f64);
            dst.set(x, y, c);
        }
    }
}

/// 元の CopyBuf2ToDst。下書きを出力の大きさへ双一次で戻す。
pub fn copy_buf2_to_dst(src: &Buf, dst: &mut Buf, ops: &Ops) {
    for y in 0..dst.h {
        let yy = y as f64 * src.h as f64 / dst.h as f64;
        for x in 0..dst.w {
            let xx = x as f64 * src.w as f64 / dst.w as f64;
            let p = src.get_d(xx, yy, ops);
            dst.set(x, y, p);
        }
    }
}

/// 元の CRot::SetRotLength。角度と長さからベクトルを出す
/// (0 が上、時計回り。象限ごとに場合分けするところも元のまま)。
pub fn rot_vector(rot: f64, length: f64) -> (f64, f64) {
    if length == 0.0 {
        return (0.0, 0.0);
    }
    // 元の RoundRot: 固定小数に落として 0..360 に収める。
    let rr = {
        let mut v = (rot * 65536.0 + 0.5) as i64 % (360 * 65536);
        if v < 0 {
            v += 360 * 65536;
        }
        v as f64 / 65536.0
    };
    let rad = |d: f64| d * std::f64::consts::PI / 180.0;
    if (0.0..90.0).contains(&rr) {
        (rad(rr).sin() * length, -rad(rr).cos() * length)
    } else if (90.0..180.0).contains(&rr) {
        (rad(rr - 90.0).cos() * length, rad(rr - 90.0).sin() * length)
    } else if (180.0..270.0).contains(&rr) {
        (-rad(rr - 180.0).sin() * length, rad(rr - 180.0).cos() * length)
    } else {
        (-rad(rr - 270.0).cos() * length, -rad(rr - 270.0).sin() * length)
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

    fn px(r: f64, a: f64) -> Px {
        Px { a, r, g: r, b: r }
    }

    /// 乗算と乗算戻し。
    #[test]
    fn the_premultiply_round_trips() {
        let o = ops8();
        let p = px(200.0, 128.0);
        let m = o.premultiply(&p);
        assert!(m.r < 200.0);
        let back = o.from_mat(&m);
        assert!((back.r - 200.0).abs() <= 2.0, "{:?}", back);
    }

    /// 縮小は4点のうちいちばん明るいものを拾う。
    #[test]
    fn the_shrink_takes_the_brightest_of_four() {
        let o = ops8();
        let mut src = Buf::new(2, 2);
        src.set(0, 0, px(10.0, 255.0));
        src.set(1, 0, px(200.0, 255.0));
        src.set(0, 1, px(30.0, 255.0));
        src.set(1, 1, px(40.0, 255.0));
        let mut dst = Buf::new(1, 1);
        copy_into_buf1(&src, &mut dst, &o);
        assert_eq!(dst.get(0, 0).r, 200.0);
        assert_eq!(dst.get(0, 0).a, 255.0);
    }

    /// 灰色版はいちばん明るい灰色をアルファに入れ、RGB は最大。
    #[test]
    fn the_gray_shrink_puts_the_light_in_the_alpha() {
        let o = ops8();
        let mut src = Buf::new(2, 2);
        src.set(1, 1, px(200.0, 255.0));
        let mut dst = Buf::new(1, 1);
        copy_into_buf1_gray(&src, &mut dst, &o);
        let p = dst.get(0, 0);
        assert_eq!(p.r, 255.0);
        assert!(p.a > 190.0 && p.a <= 200.0, "{p:?}");
    }

    /// 光は中心から**外へ**伸びる。画素は自分と中心のあいだを舐めるので、
    /// 中心寄りの明るい画素が外側の画素に乗る。中心そのものの画素は
    /// 舐める範囲に入らない (元の `for (i = 1; i < len2; i++)`)。
    #[test]
    fn the_shine_streaks_outwards_from_the_centre() {
        let o = ops8();
        let mut src = Buf::new(32, 1);
        // 中心 (x=16) の少し手前を明るくする。
        src.set(14, 0, px(255.0, 255.0));
        let mut dst = Buf::new(32, 1);
        sub(
            &src,
            &mut dst,
            &Dir::ToPoint { cx: 16.0, cy: 0.0 },
            20,
            1.0,
            &o,
        );
        // 外側 (x < 14) に光が伸びる。
        assert!(dst.get(10, 0).r > 0.0, "{:?}", dst.get(10, 0));
        // 近いほうが明るい。
        assert!(dst.get(12, 0).r > dst.get(8, 0).r);
        // 中心の向こう側 (x > 16) には伸びない。
        assert_eq!(dst.get(20, 0).r, 0.0);
    }

    /// 一定の向きなら、向きの先にあるものを引っぱってくる。
    #[test]
    fn the_parallel_shine_pulls_along_one_direction() {
        let o = ops8();
        let mut src = Buf::new(32, 1);
        src.set(20, 0, px(255.0, 255.0));
        let mut dst = Buf::new(32, 1);
        // 右向き (dx > 0)。
        sub(
            &src,
            &mut dst,
            &Dir::Parallel { dx: 10.0, dy: 0.0 },
            20,
            1.0,
            &o,
        );
        assert!(dst.get(14, 0).r > 0.0, "右の明るさを拾う");
        assert_eq!(dst.get(26, 0).r, 0.0, "左側へは漏れない");
    }

    /// 角度 0 は上向き、90 は右向き。
    #[test]
    fn the_rotation_vector_points_the_right_way() {
        let (x, y) = rot_vector(0.0, 10.0);
        assert!(x.abs() < 1e-9 && (y + 10.0).abs() < 1e-9, "{x} {y}");
        let (x, y) = rot_vector(90.0, 10.0);
        assert!((x - 10.0).abs() < 1e-9 && y.abs() < 1e-9, "{x} {y}");
        let (x, y) = rot_vector(180.0, 10.0);
        assert!(x.abs() < 1e-9 && (y - 10.0).abs() < 1e-9, "{x} {y}");
        assert_eq!(rot_vector(45.0, 0.0), (0.0, 0.0));
    }

    /// 拡大は双一次でなめらかに戻す。
    #[test]
    fn the_upscale_interpolates() {
        let o = ops8();
        let mut src = Buf::new(2, 1);
        src.set(0, 0, px(0.0, 255.0));
        src.set(1, 0, px(200.0, 255.0));
        let mut dst = Buf::new(4, 1);
        copy_buf2_to_dst(&src, &mut dst, &o);
        assert_eq!(dst.get(0, 0).r, 0.0);
        assert!(dst.get(1, 0).r > 0.0 && dst.get(1, 0).r < 200.0);
        assert_eq!(dst.get(2, 0).r, 200.0);
    }
}
