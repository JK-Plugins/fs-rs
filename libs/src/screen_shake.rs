//! 元の `FsScreenShake.h` の移植。画面全体を固定小数の量だけずらして
//! 拾い直すところと、はみ出した縁の扱い。
//!
//! `ScreenShakeDir` と `ScreenShakeMM` が共有している (この2つは大きさの
//! 指定が px か mm かだけの違い)。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/FsLibrary/FsScreenShake.h

use crate::fs_graphics::XorShift;
use after_effects as ae;

/// 元の edge_none / edge_fill / edge_rep / edge_mirror。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    /// はみ出したところは透明
    None,
    /// 端の画素を伸ばす
    Fill,
    /// 1周ぶん折り返す
    Rep,
    /// 鏡
    Mirror,
}

impl Edge {
    /// 元のポップアップ (1 origin)。
    pub fn from_popup(v: i32) -> Self {
        match v {
            2 => Edge::Fill,
            3 => Edge::Rep,
            4 => Edge::Mirror,
            _ => Edge::None,
        }
    }

    /// 1軸ぶんの折り返し。`None` は透明 (edge_none のはみ出し)。
    ///
    /// 元の `getPixel*_rep` / `getPixel*_mirror` は折り返しを1回しか
    /// しないので、ずらし量が画像の幅や高さを超えると配列の外を読む。
    /// Rust では落ちるので最後に端へ寄せてある (範囲に収まる場合の値は
    /// 元と同じ)。
    pub fn map(self, v: i32, n: i32) -> Option<i32> {
        match self {
            Edge::None => {
                if v < 0 || v >= n {
                    None
                } else {
                    Some(v)
                }
            }
            Edge::Fill => Some(v.clamp(0, n - 1)),
            Edge::Rep => {
                let r = if v < 0 {
                    v + n
                } else if v >= n {
                    v - n
                } else {
                    v
                };
                Some(r.clamp(0, n - 1))
            }
            Edge::Mirror => {
                let r = if v < 0 {
                    -v
                } else if v >= n {
                    (n - (v - n) - 1) % n
                } else {
                    v
                };
                Some(r.clamp(0, n - 1))
            }
        }
    }
}

/// 固定小数の座標を整数部と 0..256 の重みに分ける (元の `>>8` の刻み)。
fn split(v: i32) -> (i32, i32) {
    (v >> 16, (v & 0xFFFF) >> 8)
}

/// 元の getPixel8。重みは 8bit (0..256) の整数で、足したあとに `>>16`。
pub fn get8(src: &[ae::Pixel8], w: i32, h: i32, edge: Edge, x: i32, y: i32) -> ae::Pixel8 {
    let (xx, dx1) = split(x);
    let (yy, dy1) = split(y);
    let at = |px: i32, py: i32| -> ae::Pixel8 {
        match (edge.map(px, w), edge.map(py, h)) {
            (Some(sx), Some(sy)) => src[(sx + sy * w) as usize],
            _ => ae::Pixel8 {
                alpha: 0,
                red: 0,
                green: 0,
                blue: 0,
            },
        }
    };
    if dx1 == 0 && dy1 == 0 {
        return at(xx, yy);
    }
    let (dx2, dy2) = (256 - dx1, 256 - dy1);
    let p = [
        at(xx, yy),
        at(xx + 1, yy),
        at(xx, yy + 1),
        at(xx + 1, yy + 1),
    ];
    let mix = |f: &dyn Fn(&ae::Pixel8) -> i32| -> u8 {
        let t = (f(&p[0]) * dx2 * dy2
            + f(&p[1]) * dx1 * dy2
            + f(&p[2]) * dx2 * dy1
            + f(&p[3]) * dx1 * dy1)
            >> 16;
        // 元は上だけ切る。
        t.min(ae::MAX_CHANNEL8 as i32) as u8
    };
    ae::Pixel8 {
        alpha: mix(&|p| p.alpha as i32),
        red: mix(&|p| p.red as i32),
        green: mix(&|p| p.green as i32),
        blue: mix(&|p| p.blue as i32),
    }
}

/// 元の getPixel16。8bit と違って**項ごとに `>>16`** してから足す。
pub fn get16(src: &[ae::Pixel16], w: i32, h: i32, edge: Edge, x: i32, y: i32) -> ae::Pixel16 {
    let (xx, dx1) = split(x);
    let (yy, dy1) = split(y);
    let at = |px: i32, py: i32| -> ae::Pixel16 {
        match (edge.map(px, w), edge.map(py, h)) {
            (Some(sx), Some(sy)) => src[(sx + sy * w) as usize],
            _ => ae::Pixel16 {
                alpha: 0,
                red: 0,
                green: 0,
                blue: 0,
            },
        }
    };
    if dx1 == 0 && dy1 == 0 {
        return at(xx, yy);
    }
    let (dx2, dy2) = (256 - dx1, 256 - dy1);
    let p = [
        at(xx, yy),
        at(xx + 1, yy),
        at(xx, yy + 1),
        at(xx + 1, yy + 1),
    ];
    let mix = |f: &dyn Fn(&ae::Pixel16) -> i32| -> u16 {
        let t = ((f(&p[0]) * dx2 * dy2) >> 16)
            + ((f(&p[1]) * dx1 * dy2) >> 16)
            + ((f(&p[2]) * dx2 * dy1) >> 16)
            + ((f(&p[3]) * dx1 * dy1) >> 16);
        t.min(ae::MAX_CHANNEL16 as i32) as u16
    };
    ae::Pixel16 {
        alpha: mix(&|p| p.alpha as i32),
        red: mix(&|p| p.red as i32),
        green: mix(&|p| p.green as i32),
        blue: mix(&|p| p.blue as i32),
    }
}

/// 元の getPixel32。f32 で混ぜる。切る処理は無い。
pub fn get32(src: &[ae::PixelF32], w: i32, h: i32, edge: Edge, x: i32, y: i32) -> ae::PixelF32 {
    let xx = x >> 16;
    let yy = y >> 16;
    let dx1 = (x & 0xFFFF) as f32 / 65536.0;
    let dy1 = (y & 0xFFFF) as f32 / 65536.0;
    let at = |px: i32, py: i32| -> ae::PixelF32 {
        match (edge.map(px, w), edge.map(py, h)) {
            (Some(sx), Some(sy)) => src[(sx + sy * w) as usize],
            _ => ae::PixelF32 {
                alpha: 0.0,
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            },
        }
    };
    if dx1 == 0.0 && dy1 == 0.0 {
        return at(xx, yy);
    }
    let (dx2, dy2) = (1.0f32 - dx1, 1.0f32 - dy1);
    let b = [dx2 * dy2, dx1 * dy2, dx2 * dy1, dx1 * dy1];
    let p = [
        at(xx, yy),
        at(xx + 1, yy),
        at(xx, yy + 1),
        at(xx + 1, yy + 1),
    ];
    let mix = |f: &dyn Fn(&ae::PixelF32) -> f32| -> f32 {
        f(&p[0]) * b[0] + f(&p[1]) * b[1] + f(&p[2]) * b[2] + f(&p[3]) * b[3]
    };
    ae::PixelF32 {
        alpha: mix(&|p| p.alpha),
        red: mix(&|p| p.red),
        green: mix(&|p| p.green),
        blue: mix(&|p| p.blue),
    }
}

/// 揺れの指定。長さは固定小数の px、角度は固定小数の度。
#[derive(Debug, Clone, Copy)]
pub struct Shake {
    pub value: i32,
    pub value_rand: i32,
    pub dir: i32,
    pub dir_rand: i32,
    pub randomseed: i32,
}

/// 元の RoundAngle360。
pub fn round_angle_360(f: i32) -> i32 {
    let mut r = f % (360 << 16);
    if r < 0 {
        r += 360 << 16;
    }
    r
}

/// 1フレームぶんのずらし量 (固定小数の px)。元の PramSet。
///
/// `faithful` が真だと元のままの振る舞いになる:
///
/// - **乱数を蒔く前に引いている**。元は PramSet のあとの Exec で
///   `F_SRAND(frame + randomseed)` を呼ぶので、ここで引く2つは種の影響を
///   受けない。プラグインを読み込んだ直後は `gSeed128` が全部 0 で、
///   xorShift は 0 を返し続けるため、`F_RAND2(a,b)` はいつも `a`。
///   つまり **Random Seed はどこにも効かず**、揺れの大きさは最小値、
///   向きはばらつきを引いた値に張り付く (2回目以降の描画では前回の種が
///   残っているので、絵が呼ばれ方で変わる)
/// - **大きさの最小値の計算が 32bit で溢れる**。`value * (100 - ばらつき)`
///   は 328px あたりから A_long に収まらない
///
/// 直した版は先に種を蒔き、最小値を 64bit で計算する。
pub fn shift(s: &Shake, frame: i32, faithful: bool) -> (i32, i32) {
    let mut rng = XorShift::new((frame + s.randomseed) as u32);
    let mut rand2 = |lo: i64, hi: i64| -> i32 {
        if faithful {
            // 種を蒔く前の xorShift は 0 を返し続ける。
            lo as i32
        } else {
            rng.rand2(lo, hi) as i32
        }
    };

    let value_min = if faithful {
        // 元のまま: A_long で掛けてから割る (溢れる)。
        s.value
            .wrapping_mul(100 - s.value_rand)
            .wrapping_div(100)
    } else {
        (s.value as i64 * (100 - s.value_rand) as i64 / 100) as i32
    };
    let value_fin = rand2(value_min as i64, s.value as i64);

    let mut dir_fin = s.dir + rand2(-(s.dir_rand as i64), s.dir_rand as i64);
    // 元: 偶数フレームは逆向き。
    if frame % 2 == 0 {
        dir_fin += 180 << 16;
    }
    let dir_fin = round_angle_360(dir_fin);

    crate::utils::rot_length(dir_fin, value_fin)
}

/// 元の mm2px。
pub fn mm2px(mm: i32, dpi: i32) -> i32 {
    let r = (mm as f64 / 65536.0) * (dpi as f64 / 65536.0) / 25.4;
    crate::utils::flt2fix(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_edges_fold_the_way_the_original_did() {
        assert_eq!(Edge::None.map(-1, 10), None);
        assert_eq!(Edge::None.map(10, 10), None);
        assert_eq!(Edge::None.map(3, 10), Some(3));
        assert_eq!(Edge::Fill.map(-5, 10), Some(0));
        assert_eq!(Edge::Fill.map(12, 10), Some(9));
        assert_eq!(Edge::Rep.map(-1, 10), Some(9));
        assert_eq!(Edge::Rep.map(10, 10), Some(0));
        assert_eq!(Edge::Mirror.map(-3, 10), Some(3));
        assert_eq!(Edge::Mirror.map(10, 10), Some(9));
    }

    fn px(a: u8, v: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: a,
            red: v,
            green: v,
            blue: v,
        }
    }

    /// 刻みが無ければそのままの画素。
    #[test]
    fn a_whole_pixel_is_taken_as_it_is() {
        let src = vec![px(255, 10), px(255, 20)];
        let r = get8(&src, 2, 1, Edge::Fill, 1 << 16, 0);
        assert_eq!(r.red, 20);
    }

    /// 半画素ずれたら隣と半分ずつ混ざる。
    #[test]
    fn a_half_pixel_mixes_with_the_neighbour() {
        let src = vec![px(255, 0), px(255, 200)];
        let r = get8(&src, 2, 1, Edge::Fill, 1 << 15, 0);
        assert_eq!(r.red, 100);
    }

    /// 透明の縁は外から 0 を混ぜてくる。
    #[test]
    fn the_transparent_edge_mixes_in_nothing() {
        let src = vec![px(255, 200)];
        let r = get8(&src, 1, 1, Edge::None, -(1 << 15), 0);
        assert_eq!(r.red, 100);
        assert_eq!(r.alpha, 127);
    }

    /// 元のままの版は大きさの最小値と「ばらつきを引いた向き」に張り付く。
    #[test]
    fn the_faithful_shift_never_uses_the_seed() {
        let s = Shake {
            value: 10 << 16,
            value_rand: 10,
            dir: 90 << 16,
            dir_rand: 10 << 16,
            randomseed: 0,
        };
        let a = shift(&s, 1, true);
        let mut s2 = s;
        s2.randomseed = 42;
        assert_eq!(a, shift(&s2, 1, true));
        // 90 - 10 = 80 度、長さは 10 * 90/100 = 9px。
        assert_eq!(a, crate::utils::rot_length(80 << 16, 9 << 16));
    }

    /// 直した版は種で絵が変わる。
    #[test]
    fn the_fixed_shift_follows_the_seed() {
        let s = Shake {
            value: 10 << 16,
            value_rand: 50,
            dir: 90 << 16,
            dir_rand: 30 << 16,
            randomseed: 0,
        };
        let mut s2 = s;
        s2.randomseed = 7;
        assert_ne!(shift(&s, 1, false), shift(&s2, 1, false));
    }

    /// 偶数フレームは逆向き。
    #[test]
    fn the_even_frames_flip_the_direction() {
        let s = Shake {
            value: 10 << 16,
            value_rand: 0,
            dir: 0,
            dir_rand: 0,
            randomseed: 0,
        };
        let (_, y0) = shift(&s, 0, true);
        let (_, y1) = shift(&s, 1, true);
        assert!(y0 > 0 && y1 < 0, "y0={y0} y1={y1}");
    }

    /// 元のままの版は 328px あたりから溢れる。直した版は溢れない。
    #[test]
    fn the_faithful_minimum_overflows_on_a_big_shake() {
        let s = Shake {
            value: 1000 << 16,
            value_rand: 0,
            dir: 90 << 16,
            dir_rand: 0,
            randomseed: 0,
        };
        // 直した版は 1000px そのまま。
        assert_eq!(shift(&s, 1, false).0, 1000 << 16);
        assert_ne!(shift(&s, 1, true).0, 1000 << 16);
    }

    #[test]
    fn the_millimetres_become_pixels() {
        // 25.4mm = 1 インチ = dpi ぶんの px (25.4 を固定小数にする誤差ぶん
        // だけずれる)。
        let v = mm2px(crate::utils::flt2fix(25.4), 144 << 16);
        assert!(((144 << 16) - v).abs() <= 4, "v={v}");
        // 半分の mm なら半分の px。
        let v = mm2px(crate::utils::flt2fix(12.7), 200 << 16);
        assert!(((100 << 16) - v).abs() <= 4, "v={v}");
    }
}
