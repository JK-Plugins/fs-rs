// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/MaxFast/MaxSub.cpp
//      https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/MaxBlur/Tiny_MinMax.cpp
//
//! van Herk / Gil-Werman の最大値・最小値フィルタ。`MaxFast` と
//! `MaxBlur` (Tiny_MinMax) がまったく同じものを持っているので、ここに
//! まとめてある。AE の型に依存しない (`P` は `Copy` なら何でもよい)。

/// 元の `lineLevel`。double で足して float に落とす。
pub fn level(r: f64, g: f64, b: f64, a: f32, max_chan: f32) -> f32 {
    ((r * 0.299 + g * 0.587 + b * 0.114) * 10.0 + (a / max_chan) as f64) as f32
}

/// 1本ぶんの最大 (または最小) フィルタ。元の Max_SubH / Min_SubH /
/// Max_SubV / Min_SubV は向きと深度が違うだけで中身は同じ。
///
/// `src` は走査前の写し、`out` に結果を書く。`levels` は `src` と同じ長さ。
pub fn line_pass<P: Copy>(src: &[P], levels: &[f32], out: &mut [P], radius: i64, is_max: bool) {
    let wu = src.len();
    let w = wu as i64;
    if w == 0 || radius <= 0 {
        return;
    }
    let better = |v: f32, cur: f32| if is_max { v > cur } else { v < cur };

    // 元: 半径が幅以上なら全体の最大で塗りつぶす。
    if radius >= w {
        let mut bi = 0usize;
        let mut bv = levels[0];
        for x in 1..wu {
            if better(levels[x], bv) {
                bi = x;
                bv = levels[x];
            }
        }
        for x in 0..wu {
            out[x] = src[bi];
        }
        return;
    }

    let l = (radius * 2 + 1) as usize;
    // ブロックごとの前向き・後ろ向きの最大 (値とその位置)。
    let mut fwd = vec![(0.0f32, 0usize); wu];
    let mut bwd = vec![(0.0f32, 0usize); wu];
    let mut b = 0usize;
    while b < wu {
        let end = (b + l).min(wu);
        fwd[b] = (levels[b], b);
        for i in b + 1..end {
            // 元: 同点なら前のを引き継ぐ (最大は >=、最小は <=)。
            let keep = if is_max {
                fwd[i - 1].0 >= levels[i]
            } else {
                fwd[i - 1].0 <= levels[i]
            };
            fwd[i] = if keep { fwd[i - 1] } else { (levels[i], i) };
        }
        bwd[end - 1] = (levels[end - 1], end - 1);
        for i in (b..end - 1).rev() {
            let keep = if is_max {
                bwd[i + 1].0 >= levels[i]
            } else {
                bwd[i + 1].0 <= levels[i]
            };
            bwd[i] = if keep { bwd[i + 1] } else { (levels[i], i) };
        }
        b += l;
    }

    for i in 0..wu {
        let safe_left = (i as i64 - radius).max(0) as usize;
        let safe_right = (i as i64 + radius).min(w - 1) as usize;
        let left_block = safe_left / l;
        let right_block = safe_right / l;

        let mut best = if is_max { f32::MIN } else { f32::MAX };
        let mut bi = safe_left;
        if left_block == right_block {
            // 元: ブロックをまたがないときは素直に見る。
            for j in safe_left..=safe_right {
                if better(levels[j], best) {
                    best = levels[j];
                    bi = j;
                }
            }
        } else {
            if better(bwd[safe_left].0, best) {
                best = bwd[safe_left].0;
                bi = bwd[safe_left].1;
            }
            for bb in left_block + 1..right_block {
                let block_end = (bb * l + l - 1).min(wu - 1);
                if better(fwd[block_end].0, best) {
                    best = fwd[block_end].0;
                    bi = fwd[block_end].1;
                }
            }
            if better(fwd[safe_right].0, best) {
                bi = fwd[safe_right].1;
            }
        }
        out[i] = src[bi];
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn run(vals: &[u8], radius: i64, is_max: bool) -> Vec<u8> {
        let levels: Vec<f32> = vals
            .iter()
            .map(|v| level(*v as f64, *v as f64, *v as f64, 255.0, 255.0))
            .collect();
        let mut out = vals.to_vec();
        line_pass(vals, &levels, &mut out, radius, is_max);
        out
    }

    /// 最大値フィルタは明るいところを半径ぶん広げる。
    #[test]
    fn the_max_spreads_the_bright_spot() {
        let v = [0u8, 0, 0, 9, 0, 0, 0];
        assert_eq!(run(&v, 1, true), vec![0, 0, 9, 9, 9, 0, 0]);
        assert_eq!(run(&v, 2, true), vec![0, 9, 9, 9, 9, 9, 0]);
    }

    /// 最小値フィルタは暗いところを広げる。
    #[test]
    fn the_min_spreads_the_dark_spot() {
        let v = [9u8, 9, 9, 0, 9, 9, 9];
        assert_eq!(run(&v, 1, false), vec![9, 9, 0, 0, 0, 9, 9]);
    }

    /// ブロックの境界をまたいでも窓は正しい。
    #[test]
    fn the_window_crosses_the_block_boundary() {
        let v = [0u8, 0, 5, 0, 0, 0, 0, 0, 0];
        assert_eq!(run(&v, 1, true), vec![0, 5, 5, 5, 0, 0, 0, 0, 0]);
    }

    /// 窓が列より広ければ全体がいちばん明るい画素になる。
    #[test]
    fn a_window_wider_than_the_line_flattens_it() {
        let v = [1u8, 7, 3];
        assert_eq!(run(&v, 3, true), vec![7, 7, 7]);
        assert_eq!(run(&v, 99, false), vec![1, 1, 1]);
    }

    /// 半径 0 なら触らない。
    #[test]
    fn no_radius_changes_nothing() {
        let v = [1u8, 7, 3];
        assert_eq!(run(&v, 0, true), vec![1, 7, 3]);
    }

    /// 同じ明るさならアルファの高いほうが勝つ。
    #[test]
    fn alpha_breaks_the_tie() {
        let la = level(100.0, 100.0, 100.0, 10.0, 255.0);
        let lb = level(100.0, 100.0, 100.0, 250.0, 255.0);
        assert!(lb > la);
        let px = [0u8, 1, 0];
        let levels = vec![la, lb, la];
        let mut out = px.to_vec();
        line_pass(&px, &levels, &mut out, 1, true);
        assert_eq!(out, vec![1, 1, 1]);
    }
}
