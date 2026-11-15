// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/FsLibrary/FsHLS.h
//
// AE の型に依存しない純粋な実装（Linux でもテストできるようにするため）。

/// 元の LABA。L/A/B は元の実装に合わせて 1/100 スケール
/// (CIE Lab の L*=100 が L=1.0、a*=100 が A=1.0)。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LABA {
    pub l: f64,
    pub a: f64,
    pub b: f64,
    pub alpha: f32,
}

/// sRGB のガンマを戻す。元の RgbToLab の先頭3ブロックと同じ。
fn linearize(c: f64) -> f64 {
    if c < 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// 元の f 関数 (立方根と線形部分の切り替え)。
fn pivot(t: f64) -> f64 {
    if t > 0.008856 {
        // 元は pow(t, 1./3)。cbrt との最終ビットの違いで 8bit 出力が
        // 1 ずれることがあるので、元と同じ pow を使う。
        t.powf(1.0 / 3.0)
    } else {
        7.787 * t + 16.0 / 116.0
    }
}

/// 元の RgbToLab。入力は 0..1 の sRGB。L/A/B は 1/100 スケール。
///
/// 元の実装にはバグがある。L は
///   y > 0.008856 : 1.16*fy      (CIE の 116*fy、/100 スケール)
///   それ以外     : 9.033*y      (CIE の 903.3*y、同スケール)
/// のあと、どちらにも `-0.16` を引いている。-16 のオフセットは立方根側
/// だけのもので、線形側に引くと暗部の L が負になり、しきい値
/// y = 0.008856 のところで L が 0 から 0.0799 (8bit で 20) へ飛ぶ。
/// ここでは線形側の -0.16 を外して連続にしてある。黒は L = 0 になる。
pub fn rgb_to_lab(red: f64, green: f64, blue: f64, alpha: f32) -> LABA {
    let sr = linearize(red);
    let sg = linearize(green);
    let sb = linearize(blue);

    // XYZ へ
    let mut x = 0.412453 * sr + 0.357580 * sg + 0.180423 * sb;
    let y = 0.212671 * sr + 0.715160 * sg + 0.072169 * sb;
    let mut z = 0.019334 * sr + 0.119193 * sg + 0.950227 * sb;

    x /= 0.950456;
    z /= 1.088754;

    // Lab へ
    let (fy, l) = if y > 0.008856 {
        let fy = y.powf(1.0 / 3.0);
        (fy, 1.16 * fy - 0.16)
    } else {
        // 元はここでも -0.16 を引いていた (上のコメント参照)。
        (7.787 * y + 16.0 / 116.0, 9.033 * y)
    };

    let fx = pivot(x);
    let fz = pivot(z);

    LABA {
        l,
        a: 5.0 * (fx - fy),
        b: 2.0 * (fy - fz),
        alpha,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "{a} != {b}");
    }

    #[test]
    fn white_is_l_one() {
        let lab = rgb_to_lab(1.0, 1.0, 1.0, 1.0);
        approx(lab.l, 1.0);
        // 白は無彩色なので a/b はほぼ 0 (元の係数の丸めぶんだけずれる)。
        assert!(lab.a.abs() < 0.01, "a = {}", lab.a);
        assert!(lab.b.abs() < 0.01, "b = {}", lab.b);
    }

    #[test]
    fn black_is_l_zero() {
        // 元は線形側からも -0.16 を引いて L = -0.16 にしていた。
        approx(rgb_to_lab(0.0, 0.0, 0.0, 1.0).l, 0.0);
    }

    #[test]
    fn l_is_continuous_across_the_threshold() {
        // しきい値 y = 0.008856 の前後で L が飛ばないこと。元はここで
        // 0 から 0.0799 (8bit で 20) へ飛んでいた。
        let below = rgb_to_lab(0.0, 0.0999, 0.0, 1.0).l;
        let above = rgb_to_lab(0.0, 0.1001, 0.0, 1.0).l;
        assert!(
            (above - below).abs() < 0.01,
            "jump at the threshold: {below} -> {above}"
        );
    }

    #[test]
    fn grey_is_monotonic_in_l() {
        let mut prev = f64::MIN;
        for i in 0..=10 {
            let c = i as f64 / 10.0;
            let l = rgb_to_lab(c, c, c, 1.0).l;
            assert!(l > prev, "L not increasing at {c}: {l} <= {prev}");
            prev = l;
        }
    }

    #[test]
    fn green_is_lighter_than_blue() {
        // 輝度の重みどおりの並びになっているか。
        let g = rgb_to_lab(0.0, 1.0, 0.0, 1.0).l;
        let r = rgb_to_lab(1.0, 0.0, 0.0, 1.0).l;
        let b = rgb_to_lab(0.0, 0.0, 1.0, 1.0).l;
        assert!(g > r && r > b, "L order wrong: g={g} r={r} b={b}");
    }

    #[test]
    fn alpha_passes_through() {
        assert_eq!(rgb_to_lab(0.5, 0.5, 0.5, 0.25).alpha, 0.25);
    }
}
