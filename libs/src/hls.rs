use after_effects::sys::PF_PixelFloat;

pub struct HLSA {
    pub hue: f64,
    pub lightness: f64,
    pub saturation: f64,
    pub alpha: f64,
}

impl HLSA {
    pub fn new() -> Self {
        Self {
            hue: 0.0,
            lightness: 0.0,
            saturation: 0.0,
            alpha: 1.0,
        }
    }

    pub fn rgb_to_hls(c: &PF_PixelFloat) -> Self {
        let mut hls = HLSA::new();

        hls.alpha = c.alpha as f64;

        let red = c.red as f64;
        let green = c.green as f64;
        let blue = c.blue as f64;

        let c_max = red.max(green).max(blue);
        let c_min = red.min(green).min(blue);
        let c_delta = c_max - c_min;

        hls.hue = 0.0;
        hls.saturation = 0.0;
        hls.lightness = (c_max as f64 + c_min as f64) / 2.0;

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
            } else if blue == c_max {
                hls.hue = (red - green) / c_delta + 4.0;
            }
            hls.hue /= 6.0;
        }
        hls
    }
}
