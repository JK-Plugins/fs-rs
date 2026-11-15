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

    /// HLS から RGB (0.0..=1.0) に戻す。元の `HLStoRGB` と同じ計算。
    ///
    /// 返すのは (red, green, blue, alpha)。彩度が 0 のときは明度をそのまま
    /// 3チャンネルに入れる、という元の分岐も踏襲している。
    pub fn to_rgb(&self) -> (f64, f64, f64, f64) {
        if self.saturation == 0.0 {
            let l = self.lightness;
            return (l, l, l, self.alpha);
        }

        let chroma = if self.lightness <= 0.5 {
            2.0 * self.lightness * self.saturation
        } else {
            (2.0 - 2.0 * self.lightness) * self.saturation
        };
        let m = self.lightness - chroma / 2.0;

        let h6 = 6.0 * self.hue;
        (
            hsl_value(h6 + 2.0, chroma, m),
            hsl_value(h6, chroma, m),
            hsl_value(h6 - 2.0, chroma, m),
            self.alpha,
        )
    }
}

/// 元の `hsl_value`。色相を6分割した位置 `hh` から1チャンネル分の値を出す。
fn hsl_value(mut hh: f64, chroma: f64, m: f64) -> f64 {
    // 元は 6 を一度足す / 引くだけで、範囲外が大きく外れていても畳み込まない。
    if hh < 0.0 {
        hh += 6.0;
    } else if hh >= 6.0 {
        hh -= 6.0;
    }

    let val = if hh < 1.0 {
        chroma * hh
    } else if hh < 3.0 {
        chroma
    } else if hh < 4.0 {
        chroma * (4.0 - hh)
    } else {
        0.0
    };
    val + m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn to_rgb_roundtrips_primaries() {
        // 原色・白・黒・グレーは RGB -> HLS -> RGB で戻るはず。
        for (r, g, b) in [
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (1.0, 1.0, 0.0),
            (0.0, 1.0, 1.0),
            (1.0, 0.0, 1.0),
            (1.0, 1.0, 1.0),
            (0.0, 0.0, 0.0),
            (0.5, 0.5, 0.5),
            (0.2, 0.6, 0.4),
        ] {
            let hls = HLSA::from_rgb(r, g, b, 1.0);
            let (r2, g2, b2, a2) = hls.to_rgb();
            assert!(close(r, r2) && close(g, g2) && close(b, b2), "{r},{g},{b} -> {r2},{g2},{b2}");
            assert!(close(a2, 1.0));
        }
    }

    #[test]
    fn to_rgb_with_zero_saturation_uses_lightness() {
        // 元の分岐: S == 0 なら L を3チャンネルに入れる。
        let hls = HLSA { hue: 0.42, lightness: 0.3, saturation: 0.0, alpha: 0.5 };
        assert_eq!(hls.to_rgb(), (0.3, 0.3, 0.3, 0.5));
    }

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
