// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/FsLibrary/FsHLS.h
//
// AE の型に依存しない純粋な実装（Linux でもテストできるようにするため）。

/// H, L, S, A はすべて 0.0..=1.0
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HLSA {
    pub hue: f64,
    pub lightness: f64,
    pub saturation: f64,
    pub alpha: f64,
}

impl Default for HLSA {
    fn default() -> Self {
        Self {
            hue: 0.0,
            lightness: 0.0,
            saturation: 0.0,
            alpha: 1.0,
        }
    }
}

impl HLSA {
    pub fn new() -> Self {
        Self::default()
    }

    /// RGB (0.0..=1.0) から HLS に変換する。元の `RGBtoHLS` と同じ計算。
    pub fn from_rgb(red: f64, green: f64, blue: f64, alpha: f64) -> Self {
        let mut hls = HLSA {
            alpha,
            ..Default::default()
        };

        let c_max = red.max(green).max(blue);
        let c_min = red.min(green).min(blue);
        let c_delta = c_max - c_min;

        hls.lightness = (c_max + c_min) / 2.0;

        if c_delta != 0.0 {
            if hls.lightness <= 0.5 {
                hls.saturation = c_delta / (2.0 * hls.lightness);
            } else {
                hls.saturation = c_delta / (2.0 - 2.0 * hls.lightness);
            }

            if red == c_max {
                hls.hue = (green - blue) / c_delta;
                if hls.hue < 0.0 {
                    hls.hue += 6.0;
                }
            } else if green == c_max {
                hls.hue = (blue - red) / c_delta + 2.0;
            } else {
                hls.hue = (red - green) / c_delta + 4.0;
            }
            hls.hue /= 6.0;
        }
        hls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn primaries() {
        let r = HLSA::from_rgb(1.0, 0.0, 0.0, 1.0);
        assert!(approx(r.hue, 0.0) && approx(r.saturation, 1.0) && approx(r.lightness, 0.5));
        let g = HLSA::from_rgb(0.0, 1.0, 0.0, 1.0);
        assert!(approx(g.hue, 1.0 / 3.0));
        let b = HLSA::from_rgb(0.0, 0.0, 1.0, 1.0);
        assert!(approx(b.hue, 2.0 / 3.0));
        let m = HLSA::from_rgb(1.0, 0.0, 1.0, 1.0);
        assert!(approx(m.hue, 5.0 / 6.0));
    }

    #[test]
    fn gray_has_no_hue_or_saturation() {
        let c = HLSA::from_rgb(0.4, 0.4, 0.4, 0.5);
        assert!(approx(c.hue, 0.0) && approx(c.saturation, 0.0) && approx(c.lightness, 0.4));
        assert!(approx(c.alpha, 0.5));
    }
}
