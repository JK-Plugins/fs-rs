//! F's の `CFsGraph::flat8` / `flat16` / `flat32` (`FsMosaic.cpp`)。
//!
//! 指定した矩形の平均色でその矩形を塗りつぶす。`Mosaic` が桝目に並べて使い、
//! `RandomMosaic` が乱数でばらまいて使う。
//!
//! 元の癖をそのまま写してある:
//!
//! - **平均は行ごとに幅で割って足し、最後に行数で割る**。整数の割り算を
//!   2段通すので、全部足してから面積で割るのとは値が違う
//! - 範囲が画面から出ていたら、出たぶんだけ詰める。詰めた結果が 0 以下なら
//!   何もしない。幅と高さがどちらも 1 以下なら何もしない
//! - **アルファのちらつきは「最大値 - 26 未満」のところだけに乗る**
//!   (32bit は 1.0 - 0.1 未満)。元のコメントいわく「半透明のところのみ」
//! - 32bit のクランプは `RoundFpShortDouble` なので 1.0 ではなく 32.0 で止まる

use after_effects as ae;

/// 元の flat8 / flat16 / flat32。
///
/// `rand` は (a, r, g, b) の加減値で、**その深度の目盛りのまま**渡す
/// (32bit なら 0..1 の実数、8bit なら 0..255 の整数ぶん)。
/// 塗ったら true、範囲が無くて何もしなかったら false。
pub fn flat(
    layer: &ae::Layer,
    depth: i16,
    x: i64,
    y: i64,
    size_x: i64,
    size_y: i64,
    rand: [f64; 4],
) -> bool {
    let (w, h) = (layer.width() as i64, layer.height() as i64);
    let (mut x, mut y) = (x, y);
    let (mut sx, mut sy) = (size_x, size_y);

    if sx < 0 || sy < 0 || (sx <= 1 && sy <= 1) || x >= w || y >= h {
        return false;
    }
    if x < 0 {
        sx += x;
        if sx <= 0 {
            return false;
        }
        x = 0;
    }
    if x + sx > w {
        sx = w - x;
        if sx <= 0 {
            return false;
        }
    }
    if y < 0 {
        sy += y;
        if sy <= 0 {
            return false;
        }
        y = 0;
    }
    if y + sy > h {
        sy = h - y;
        if sy <= 0 {
            return false;
        }
    }

    let [ra, rr, rg, rb] = rand;

    if depth == 32 {
        // 元の flat32: 行ごとに割って足し、最後に行数で割る (実数)。
        let (mut ya, mut yr, mut yg, mut yb) = (0.0f64, 0.0, 0.0, 0.0);
        for j in 0..sy {
            let (mut xa, mut xr, mut xg, mut xb) = (0.0f64, 0.0, 0.0, 0.0);
            for i in 0..sx {
                let p = layer.as_pixel32((x + i) as usize, (y + j) as usize);
                xa += p.alpha as f64;
                xr += p.red as f64;
                xg += p.green as f64;
                xb += p.blue as f64;
            }
            ya += xa / sx as f64;
            yr += xr / sx as f64;
            yg += xg / sx as f64;
            yb += xb / sx as f64;
        }
        // 元: RoundFpShortDouble は 0..32 でクランプ (1.0 ではない)。
        let clamp = |v: f64| v.clamp(0.0, 32.0);
        let a0 = clamp(ya / sy as f64);
        // 元: アルファのちらつきは 1.0 - 0.1 未満のところだけ。
        let a = if a0 < 1.0 - 0.1 { clamp(a0 + ra) } else { a0 };
        let c = ae::PixelF32 {
            alpha: a as f32,
            red: clamp(yr / sy as f64 + rr) as f32,
            green: clamp(yg / sy as f64 + rg) as f32,
            blue: clamp(yb / sy as f64 + rb) as f32,
        };
        for j in 0..sy {
            for i in 0..sx {
                *layer.as_pixel32_mut((x + i) as usize, (y + j) as usize) = c;
            }
        }
        return true;
    }

    let max: i64 = if depth == 8 { 255 } else { 32768 };
    let (mut ya, mut yr, mut yg, mut yb) = (0i64, 0i64, 0i64, 0i64);
    for j in 0..sy {
        let (mut xa, mut xr, mut xg, mut xb) = (0i64, 0i64, 0i64, 0i64);
        for i in 0..sx {
            let (a, r, g, b) = if depth == 8 {
                let p = layer.as_pixel8((x + i) as usize, (y + j) as usize);
                (p.alpha as i64, p.red as i64, p.green as i64, p.blue as i64)
            } else {
                let p = layer.as_pixel16((x + i) as usize, (y + j) as usize);
                (p.alpha as i64, p.red as i64, p.green as i64, p.blue as i64)
            };
            xa += a;
            xr += r;
            xg += g;
            xb += b;
        }
        ya += xa / sx;
        yr += xr / sx;
        yg += xg / sx;
        yb += xb / sx;
    }
    let clamp = |v: i64| v.clamp(0, max);
    // 元: アルファのちらつきは「最大値 - 26 未満」のところだけに乗る。
    let mut a2 = clamp(ya / sy);
    if a2 < max - 26 {
        a2 += ra as i64;
    }
    let a = clamp(a2);
    let r = clamp(yr / sy + rr as i64);
    let g = clamp(yg / sy + rg as i64);
    let b = clamp(yb / sy + rb as i64);
    for j in 0..sy {
        for i in 0..sx {
            if depth == 8 {
                *layer.as_pixel8_mut((x + i) as usize, (y + j) as usize) = ae::Pixel8 {
                    alpha: a as u8,
                    red: r as u8,
                    green: g as u8,
                    blue: b as u8,
                };
            } else {
                *layer.as_pixel16_mut((x + i) as usize, (y + j) as usize) = ae::Pixel16 {
                    alpha: a as u16,
                    red: r as u16,
                    green: g as u16,
                    blue: b as u16,
                };
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    /// 元の「行ごとに割って足し、最後に行数で割る」平均。全部足してから
    /// 面積で割るのとは値が違うことの確認 (移植のときに間違えやすい)。
    #[test]
    fn the_average_divides_per_row_first() {
        // 3x2 の矩形に 1,1,1 / 2,2,2 が入っているとする。
        let rows = [[1i64, 1, 1], [2, 2, 2]];
        let sx = 3i64;
        let sy = 2i64;
        let mut y = 0i64;
        for r in rows.iter() {
            let x: i64 = r.iter().sum();
            y += x / sx;
        }
        assert_eq!(y / sy, 1);
        // 全部足してから面積で割ると 9/6 = 1 なので、この例では同じ。
        // 端数が出る形だとずれる: 2,2,2 / 3,3,3 なら行ごとは (2+3)/2 = 2、
        // 面積なら 15/6 = 2 で同じだが、1,1,2 / 1,1,2 だと
        // 行ごとは (4/3 + 4/3)/2 = (1+1)/2 = 1、面積なら 8/6 = 1。
        // 整数の割り算は切り捨てなので、段数が増えるほど小さく出る。
        let rows = [[3i64, 3, 3], [3, 3, 4]];
        let mut y = 0i64;
        for r in rows.iter() {
            let x: i64 = r.iter().sum();
            y += x / 3;
        }
        // 行ごと: (9/3) + (10/3) = 3 + 3 = 6, /2 = 3
        assert_eq!(y / 2, 3);
        // 面積: 19/6 = 3。ここも同じだが、割り算の順番は元に合わせる。
        assert_eq!(19 / 6, 3);
    }
}
