// F's Plugin の 8bit 処理 (CFsGraph の *8 関数群) を整数演算のまま忠実に移植したもの。
// 元の .aex と出力をビット単位で一致させることを目的にしているので、
// 元実装のバグ（toWhiteMat8 の blue など）もあえてそのまま再現している。
//
// ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/FsLibrary
//      FsMiniMax8.cpp / FsBlur.cpp / FsMat.cpp / FsGraphics.h / FsUtils.h

pub const MAX8: i32 = 255;

/// PF_Pixel と同じ並び（alpha, red, green, blue）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Px8 {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Px8 {
    pub const fn new(alpha: u8, red: u8, green: u8, blue: u8) -> Self {
        Self {
            alpha,
            red,
            green,
            blue,
        }
    }
}

/// RoundByteLong: 0..=255 にクランプ
#[inline]
pub fn round_byte_long(x: i32) -> u8 {
    x.clamp(0, MAX8) as u8
}

/// RoundByteFpLong: 0..=255 にクランプして切り捨て
#[inline]
pub fn round_byte_fp_long(x: f64) -> u8 {
    let t = if x < 0.0 {
        0.0
    } else if x > MAX8 as f64 {
        MAX8 as f64
    } else {
        x
    };
    t as u8
}

/// CONV8TO32 の 1 チャンネル分（小数点以下 4 桁に丸めてから float にする）
#[inline]
pub fn conv8to32(v: u8) -> f32 {
    (((v as f64) * 10000.0 / MAX8 as f64 + 0.5) as i64 as f64 / 10000.0) as f32
}

/// FLOAT2FIX
#[inline]
pub fn float_to_fixed(f: f64) -> i32 {
    (f * 65536.0 + if f < 0.0 { -0.5 } else { 0.5 }) as i32
}

#[derive(Debug, Clone, PartialEq)]
pub struct Image8 {
    pub width: usize,
    pub height: usize,
    pub data: Vec<Px8>,
}

impl Image8 {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            data: vec![Px8::default(); width * height],
        }
    }

    #[inline]
    pub fn get(&self, x: usize, y: usize) -> Px8 {
        self.data[y * self.width + x]
    }

    #[inline]
    pub fn get_mut(&mut self, x: usize, y: usize) -> &mut Px8 {
        &mut self.data[y * self.width + x]
    }

    // ---------------------------------------------------------------- mat (FsMat.cpp)

    pub fn to_black_mat8(&mut self) {
        for p in &mut self.data {
            if (p.alpha as i32) < MAX8 {
                if p.alpha == 0 {
                    p.red = 0;
                    p.green = 0;
                    p.blue = 0;
                } else {
                    let a = p.alpha as i32;
                    p.blue = round_byte_long(p.blue as i32 * a / MAX8);
                    p.green = round_byte_long(p.green as i32 * a / MAX8);
                    p.red = round_byte_long(p.red as i32 * a / MAX8);
                }
            }
        }
    }

    pub fn from_black_mat8(&mut self) {
        for p in &mut self.data {
            if (p.alpha as i32) < MAX8 {
                if p.alpha == 0 {
                    p.red = 0;
                    p.green = 0;
                    p.blue = 0;
                } else {
                    let a = p.alpha as i32;
                    p.blue = round_byte_long(p.blue as i32 * MAX8 / a);
                    p.green = round_byte_long(p.green as i32 * MAX8 / a);
                    p.red = round_byte_long(p.red as i32 * MAX8 / a);
                }
            }
        }
    }

    /// 元実装どおり blue の計算に green を使っている（バグの再現）
    pub fn to_white_mat8(&mut self) {
        for p in &mut self.data {
            if (p.alpha as i32) < MAX8 {
                if p.alpha == 0 {
                    p.red = 255;
                    p.green = 255;
                    p.blue = 255;
                } else {
                    let a = p.alpha as i32;
                    let r = p.red as i32;
                    let g = p.green as i32;
                    p.red = round_byte_long(r * a / MAX8 + MAX8 - a);
                    p.green = round_byte_long(g * a / MAX8 + MAX8 - a);
                    p.blue = round_byte_long(g * a / MAX8 + MAX8 - a);
                }
            }
        }
    }

    pub fn from_white_mat8(&mut self) {
        for p in &mut self.data {
            if (p.alpha as i32) < MAX8 {
                if p.alpha == 0 {
                    p.red = 0;
                    p.green = 0;
                    p.blue = 0;
                } else {
                    let a = p.alpha as i32;
                    p.red = round_byte_long((p.red as i32 - MAX8 + a) * MAX8 / a);
                    p.green = round_byte_long((p.green as i32 - MAX8 + a) * MAX8 / a);
                    p.blue = round_byte_long((p.blue as i32 - MAX8 + a) * MAX8 / a);
                }
            }
        }
    }

    // ---------------------------------------------------------------- minimax (FsMiniMax8.cpp)

    /// Minimax8(value, HorAndVer, rgbAndAlpha)
    pub fn minimax8(&mut self, value: i32) {
        if value == 0 || self.width == 0 || self.height == 0 {
            return;
        }
        let radius = value.unsigned_abs() as usize;
        let minus = value < 0;

        if minus {
            self.to_white_mat8();
        } else {
            self.to_black_mat8();
        }

        // rgb は輝度で比較
        self.minimax_pass(
            radius,
            minus,
            |p| {
                (0.29891 * p.red as f64 + 0.58661 * p.green as f64 + 0.11448 * p.blue as f64 + 0.5)
                    as i32
            },
            |d, s| {
                d.red = s.red;
                d.green = s.green;
                d.blue = s.blue;
            },
        );
        // alpha はそのまま比較
        self.minimax_pass(radius, minus, |p| p.alpha as i32, |d, s| d.alpha = s.alpha);

        if minus {
            self.from_white_mat8();
        } else {
            self.from_black_mat8();
        }
    }

    fn minimax_pass(
        &mut self,
        radius: usize,
        minus: bool,
        level: impl Fn(&Px8) -> i32,
        apply: impl Fn(&mut Px8, &Px8),
    ) {
        let (w, h) = (self.width, self.height);
        let mut scanline = vec![Px8::default(); w.max(h)];
        let mut lv = vec![0i32; w.max(h)];

        for y in 0..h {
            for x in 0..w {
                scanline[x] = self.get(x, y);
                lv[x] = level(&scanline[x]);
            }
            let picks = minimax_line(&lv[..w], radius, minus);
            for x in 0..w {
                apply(self.get_mut(x, y), &scanline[picks[x]]);
            }
        }
        for x in 0..w {
            for y in 0..h {
                scanline[y] = self.get(x, y);
                lv[y] = level(&scanline[y]);
            }
            let picks = minimax_line(&lv[..h], radius, minus);
            for y in 0..h {
                apply(self.get_mut(x, y), &scanline[picks[y]]);
            }
        }
    }

    // ---------------------------------------------------------------- blur (FsBlur.cpp)

    /// gblur8(value, HorAndVer)
    pub fn gblur8(&mut self, value: f64) {
        if value <= 0.0 || self.width == 0 || self.height == 0 {
            return;
        }
        let range = (value.ceil() as i32).max(1) as usize;
        let zone = value / 3.0;
        let tbl: Vec<i32> = (0..=range as i32)
            .map(|i| ((-i * i) as f64 / (2.0 * zone * zone)).exp() * 256.0)
            .map(|v| v as i32)
            .collect();

        let (w, h) = (self.width, self.height);
        let mut scanline = vec![Px8::default(); w.max(h)];

        for y in 0..h {
            for x in 0..w {
                scanline[x] = premultiply8(self.get(x, y));
            }
            for x in 0..w {
                if let Some(c) = gblur_at(&scanline[..w], x, &tbl) {
                    *self.get_mut(x, y) = c;
                }
            }
        }
        for x in 0..w {
            for y in 0..h {
                scanline[y] = premultiply8(self.get(x, y));
            }
            for y in 0..h {
                if let Some(c) = gblur_at(&scanline[..h], y, &tbl) {
                    *self.get_mut(x, y) = c;
                }
            }
        }
    }
}

/// 窓 [i - r, i + r - 1] の最大（minus なら最小）の添字。同値は後ろを採用。
fn minimax_line(lv: &[i32], radius: usize, minus: bool) -> Vec<usize> {
    let n = lv.len();
    let replace = |cur: i32, cand: i32| if minus { cur >= cand } else { cur <= cand };
    let mut picks = vec![0; n];
    let mut best: Option<usize> = None;
    for i in 0..n {
        let k0 = i.saturating_sub(radius);
        let k1 = (i + radius - 1).min(n - 1);
        match best {
            Some(b) if b >= k0 => {
                if replace(lv[b], lv[k1]) {
                    best = Some(k1);
                }
            }
            _ => {
                let mut b = k0;
                for k in k0..=k1 {
                    if replace(lv[b], lv[k]) {
                        b = k;
                    }
                }
                best = Some(b);
            }
        }
        picks[i] = best.unwrap();
    }
    picks
}

fn premultiply8(mut p: Px8) -> Px8 {
    if p.alpha == 0 {
        p.red = 0;
        p.green = 0;
        p.blue = 0;
    } else if (p.alpha as i32) < MAX8 {
        let a = p.alpha as i32;
        p.red = round_byte_long(p.red as i32 * a / MAX8);
        p.green = round_byte_long(p.green as i32 * a / MAX8);
        p.blue = round_byte_long(p.blue as i32 * a / MAX8);
    }
    p
}

fn gblur_at(line: &[Px8], i: usize, tbl: &[i32]) -> Option<Px8> {
    let (mut r, mut g, mut b, mut a, mut count) = (0i32, 0i32, 0i32, 0i32, 0i32);
    let mut add = |p: &Px8, gauss: i32| {
        if p.alpha > 0 {
            r += p.red as i32 * gauss;
            g += p.green as i32 * gauss;
            b += p.blue as i32 * gauss;
            a += p.alpha as i32 * gauss;
        }
        count += gauss;
    };
    add(&line[i], tbl[0]);
    for (d, &gauss) in tbl.iter().enumerate().skip(1) {
        if i >= d {
            add(&line[i - d], gauss);
        }
        if i + d < line.len() {
            add(&line[i + d], gauss);
        }
    }
    if count <= 0 {
        return None;
    }
    let alpha = round_byte_long(a / count);
    let mut c = Px8::new(alpha, 0, 0, 0);
    if alpha == 0 {
        // rgb = 0
    } else if (alpha as i32) < MAX8 {
        let div = (1.0 / count as f64) * MAX8 as f64 / alpha as f64;
        c.red = round_byte_fp_long(r as f64 * div);
        c.green = round_byte_fp_long(g as f64 * div);
        c.blue = round_byte_fp_long(b as f64 * div);
    } else {
        c.red = round_byte_long(r / count);
        c.green = round_byte_long(g / count);
        c.blue = round_byte_long(b / count);
    }
    Some(c)
}

// -------------------------------------------------------------------- blend

/// PixelBlend8(src, dst)
pub fn pixel_blend8(ss: Px8, dd: Px8) -> Px8 {
    if ss.alpha == 0 || dd.alpha as i32 == MAX8 {
        return dd;
    }
    if dd.alpha == 0 {
        return ss;
    }
    let as2 = ((MAX8 + 1 - dd.alpha as i32) * ss.alpha as i32) >> 8;
    let ad2 = (dd.alpha as i32 + as2).min(MAX8);
    if ad2 <= 0 {
        return Px8::default();
    }
    let da = dd.alpha as i32;
    let mix = |d: u8, s: u8| ((d as i32 * da + s as i32 * as2) / ad2).min(MAX8) as u8;
    Px8::new(
        ad2 as u8,
        mix(dd.red, ss.red),
        mix(dd.green, ss.green),
        mix(dd.blue, ss.blue),
    )
}

pub fn normal_blend8(i: Px8, o: Px8) -> Px8 {
    pixel_blend8(i, o)
}

pub fn lighten_blend8(i: Px8, o: Px8) -> Px8 {
    if i.alpha == 0 {
        return o;
    }
    let o = pixel_blend8(i, o);
    Px8::new(
        o.alpha,
        o.red.max(i.red),
        o.green.max(i.green),
        o.blue.max(i.blue),
    )
}

pub fn darken_blend8(i: Px8, o: Px8) -> Px8 {
    if i.alpha == 0 {
        return o;
    }
    let o = pixel_blend8(i, o);
    Px8::new(
        o.alpha,
        o.red.min(i.red),
        o.green.min(i.green),
        o.blue.min(i.blue),
    )
}

pub fn screen_blend8(i: Px8, o: Px8) -> Px8 {
    if i.alpha == 0 {
        return o;
    }
    if o.alpha == 0 {
        return i;
    }
    let (ia, oa) = (i.alpha as i32, o.alpha as i32);
    let a = round_byte_long(ia + oa - (ia * oa) / MAX8) as i32;
    if a == 0 {
        return Px8::default();
    }
    let pm = |c: u8, al: i32| round_byte_long(c as i32 * al / MAX8) as i32;
    let (ir, ig, ib) = (pm(i.red, ia), pm(i.green, ia), pm(i.blue, ia));
    let (or, og, ob) = (pm(o.red, oa), pm(o.green, oa), pm(o.blue, oa));
    let scr = |s: i32, d: i32| {
        let v = s + d - (s * d) / MAX8;
        if a == MAX8 {
            round_byte_long(v)
        } else {
            round_byte_long(v * MAX8 / a)
        }
    };
    Px8::new(a as u8, scr(ir, or), scr(ig, og), scr(ib, ob))
}

pub fn multiply_blend8(i: Px8, o: Px8) -> Px8 {
    if i.alpha == 0 {
        return o;
    }
    if o.alpha == 0 {
        return i;
    }
    let (ia, oa) = (i.alpha as i32, o.alpha as i32);
    let a = round_byte_long(ia + oa - (ia * oa) / MAX8) as i32;
    if a == 0 {
        return Px8::default();
    }
    // 白マットへ
    let wm = |c: u8, al: i32| round_byte_long((c as i32 * al) / MAX8 + MAX8 - al) as i32;
    let (ir, ig, ib) = (wm(i.red, ia), wm(i.green, ia), wm(i.blue, ia));
    let (or, og, ob) = (wm(o.red, oa), wm(o.green, oa), wm(o.blue, oa));
    let mul = |s: i32, d: i32| {
        if a == MAX8 {
            round_byte_long(s * d / MAX8)
        } else {
            round_byte_long(((s * d / MAX8) - MAX8 + a) * MAX8 / a)
        }
    };
    Px8::new(a as u8, mul(ir, or), mul(ig, og), mul(ib, ob))
}

/// persentBlend8。元実装どおり、o の rgb は「新しい」アルファで乗算している（バグの再現）。
pub fn percent_blend8(i: Px8, o: Px8, opacity: f32) -> Px8 {
    if opacity >= 1.0 {
        return o;
    }
    if opacity <= 0.0 {
        return i;
    }
    let r1 = opacity as f64;
    let r2 = 1.0 - r1;
    let alpha = round_byte_fp_long(o.alpha as f64 * r1 + i.alpha as f64 * r2);
    if alpha == 0 {
        return Px8::new(0, 0, 0, 0);
    }
    let (ia, na) = (i.alpha as i32, alpha as i32);
    let pi = |c: u8| round_byte_long(c as i32 * ia / MAX8) as f64;
    let po = |c: u8| round_byte_long(c as i32 * na / MAX8) as f64;
    let mix = |oc: u8, ic: u8| {
        round_byte_fp_long((po(oc) * r1 + pi(ic) * r2) * MAX8 as f64 / alpha as f64)
    };
    Px8::new(
        alpha,
        mix(o.red, i.red),
        mix(o.green, i.green),
        mix(o.blue, i.blue),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conv8to32_rounds_to_4_digits() {
        assert_eq!(conv8to32(0), 0.0);
        assert_eq!(conv8to32(255), 1.0);
        assert_eq!(conv8to32(128), 0.502f32); // 128/255 = 0.50196... -> 0.502
    }

    #[test]
    fn white_mat_reproduces_blue_bug() {
        let mut img = Image8::new(1, 1);
        img.data[0] = Px8::new(128, 0, 200, 0);
        img.to_white_mat8();
        let p = img.data[0];
        assert_eq!(p.blue, p.green); // blue は green から計算される
        assert_eq!(p.red, round_byte_long(255 - 128));
    }

    #[test]
    fn minimax8_dilate() {
        let mut img = Image8::new(6, 1);
        img.data[2] = Px8::new(255, 255, 255, 255);
        for i in [0, 1, 3, 4, 5] {
            img.data[i] = Px8::new(255, 0, 0, 0);
        }
        img.minimax8(2);
        let got: Vec<u8> = img.data.iter().map(|p| p.red).collect();
        assert_eq!(got, vec![0, 255, 255, 255, 255, 0]);
    }

    #[test]
    fn gblur8_flat_is_flat() {
        let mut img = Image8::new(8, 8);
        for p in &mut img.data {
            *p = Px8::new(255, 10, 128, 250);
        }
        img.gblur8(2.5);
        for p in &img.data {
            assert_eq!(*p, Px8::new(255, 10, 128, 250));
        }
    }

    #[test]
    fn blend8_basics() {
        let i = Px8::new(255, 50, 200, 100);
        let o = Px8::new(255, 150, 100, 100);
        assert_eq!(normal_blend8(i, o), o);
        assert_eq!(lighten_blend8(i, o), Px8::new(255, 150, 200, 100));
        assert_eq!(darken_blend8(i, o), Px8::new(255, 50, 100, 100));
        assert_eq!(multiply_blend8(i, o).red, (50 * 150 / 255) as u8);
        assert_eq!(screen_blend8(i, o).red, (50 + 150 - 50 * 150 / 255) as u8);
        assert_eq!(percent_blend8(i, o, 0.0), i);
        assert_eq!(percent_blend8(i, o, 1.0), o);
        let half = percent_blend8(i, o, 0.5);
        assert_eq!(half.alpha, 255);
        assert_eq!(half.red, 100);
    }
}
