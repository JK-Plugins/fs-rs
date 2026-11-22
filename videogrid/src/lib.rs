//! F's VideoGrid の移植。
//!
//! 画面を桝目に区切り、桝の上辺と左辺を明るく、下辺と右辺を暗くして
//! 立体的な格子を描く。あわせて桝ごとにモザイクもかけられる。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしているので
//! 英語名にしてある。
//!
//! 元から写した細かいところ:
//!
//! - 桝の数は切り上げ。最後の桝は画面からはみ出す
//! - ハイライトとシャドウは `(PF_FpShort)固定小数 / 6553600` なので
//!   0..200% が 0..2。1.0 で素通し、0 以下で黒、2 以上で白、
//!   1 未満は `v * c`、1 を超えると `c + (v-1) * (最大 - c)`
//! - **ライン幅は桝の大きさの半分で止める** (`gridSize / 2`、整数の割り算)。
//!   1 より小さければ 1
//! - 立体の縁は1周ごとに桝を2つ小さくして内側へ寄せる。桝が 2 より
//!   小さくなったら打ち切り
//! - **桝が 2 以下のときは専用の経路**で、2x2 のうち右下だけシャドウ、
//!   残り3つはハイライト
//! - 辺ごとの始まりと終わりの詰め方が元は揃っていない
//!   (上辺は詰めない、左辺と下辺は +1、右辺は +1 して -1)。
//!   そのまま写してあるので、角の重なり方も元と同じ
//! - モザイクの平均は**範囲すべてを足して画素数で割る** (`flat` の
//!   行ごとに割る形とは違う)
//! - モザイクも立体も、出力に直接書いて次の桝がその結果を読む
//! - モザイクを使わず、ハイライトとシャドウがどちらも 1.0 なら何もしない
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/VideoGrid

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

/// 深度をまたいで1画素を扱う形。値は深度そのままの目盛り。
#[derive(Clone, Copy)]
struct Px {
    alpha: f64,
    red: f64,
    green: f64,
    blue: f64,
}

/// 元の lineBright8 / 16 / 32。`max` はその深度の最大値 (32bit は 1.0)。
/// `round` が true なら切り捨てて整数にする深度。
fn line_bright(c: Px, v: f32, max: f64) -> Px {
    let (r, g, b) = if v == 1.0 {
        (c.red, c.green, c.blue)
    } else if v <= 0.0 {
        (0.0, 0.0, 0.0)
    } else if v >= 2.0 {
        (max, max, max)
    } else if v < 1.0 {
        let f = |x: f64| (v as f64 * x).clamp(0.0, max);
        (f(c.red), f(c.green), f(c.blue))
    } else {
        let vv = v as f64 - 1.0;
        let f = |x: f64| (x + vv * (max - x)).clamp(0.0, max);
        (f(c.red), f(c.green), f(c.blue))
    };
    Px {
        alpha: c.alpha,
        red: r,
        green: g,
        blue: b,
    }
}

/// 出力レイヤーを深度に関係なく読み書きする。
struct Canvas<'a> {
    layer: &'a ae::Layer,
    depth: i16,
    w: i64,
    h: i64,
    max: f64,
}

impl Canvas<'_> {
    fn get(&self, x: i64, y: i64) -> Px {
        let (x, y) = (x as usize, y as usize);
        match self.depth {
            8 => {
                let p = self.layer.as_pixel8(x, y);
                Px {
                    alpha: p.alpha as f64,
                    red: p.red as f64,
                    green: p.green as f64,
                    blue: p.blue as f64,
                }
            }
            16 => {
                let p = self.layer.as_pixel16(x, y);
                Px {
                    alpha: p.alpha as f64,
                    red: p.red as f64,
                    green: p.green as f64,
                    blue: p.blue as f64,
                }
            }
            _ => {
                let p = self.layer.as_pixel32(x, y);
                Px {
                    alpha: p.alpha as f64,
                    red: p.red as f64,
                    green: p.green as f64,
                    blue: p.blue as f64,
                }
            }
        }
    }

    fn set(&self, x: i64, y: i64, c: Px) {
        let (xi, yi) = (x as usize, y as usize);
        match self.depth {
            8 => {
                *self.layer.as_pixel8_mut(xi, yi) = ae::Pixel8 {
                    alpha: c.alpha as u8,
                    red: c.red as u8,
                    green: c.green as u8,
                    blue: c.blue as u8,
                }
            }
            16 => {
                *self.layer.as_pixel16_mut(xi, yi) = ae::Pixel16 {
                    alpha: c.alpha as u16,
                    red: c.red as u16,
                    green: c.green as u16,
                    blue: c.blue as u16,
                }
            }
            _ => {
                *self.layer.as_pixel32_mut(xi, yi) = ae::PixelF32 {
                    alpha: c.alpha as f32,
                    red: c.red as f32,
                    green: c.green as f32,
                    blue: c.blue as f32,
                }
            }
        }
    }

    fn bright(&self, x: i64, y: i64, v: f32) {
        let c = self.get(x, y);
        self.set(x, y, line_bright(c, v, self.max));
    }

    /// 元の BlockBevelSubTwo8: 桝が 2 以下のときの 2x2。
    fn bevel_two(&self, x: i64, y: i64, hilight: f32, shadow: f32) {
        let (x0, x1, y0, y1) = (x, x + 1, y, y + 1);
        if x1 < 0 || x0 >= self.w || y1 < 0 || y0 >= self.h {
            return;
        }
        if x0 >= 0 && y0 >= 0 {
            self.bright(x0, y0, hilight);
        }
        if x1 < self.w && y0 >= 0 {
            self.bright(x1, y0, hilight);
        }
        if x0 >= 0 && y1 < self.h {
            self.bright(x0, y1, hilight);
        }
        // 元: 右下だけシャドウ。
        if x1 < self.w && y1 < self.h {
            self.bright(x1, y1, shadow);
        }
    }

    /// 元の BlockBevelSub8: 1周ぶんの縁。
    fn bevel_ring(&self, x: i64, y: i64, size: i64, hilight: f32, shadow: f32) {
        if size <= 2 {
            self.bevel_two(x, y, hilight, shadow);
            return;
        }
        let (x0, x1, y0, y1) = (x, x + size - 1, y, y + size - 1);
        if x1 < 0 || x0 >= self.w || y1 < 0 || y0 >= self.h {
            return;
        }
        // 上辺 (詰めない)。
        if y0 >= 0 {
            let s0 = x0.max(0);
            let s1 = if x1 >= self.w { self.w - 1 } else { x1 };
            for i in 0..(s1 - s0 + 1) {
                self.bright(s0 + i, y0, hilight);
            }
        }
        // 左辺 (始まりを +1)。
        if x0 >= 0 {
            let s0 = y0.max(0) + 1;
            let s1 = if y1 >= self.h { self.h - 1 } else { y1 };
            for i in 0..(s1 - s0 + 1) {
                self.bright(x0, s0 + i, hilight);
            }
        }
        // 下辺 (始まりを +1)。
        if y1 < self.h {
            let s0 = x0.max(0) + 1;
            let s1 = if x1 >= self.w { self.w - 1 } else { x1 };
            for i in 0..(s1 - s0 + 1) {
                self.bright(s0 + i, y1, shadow);
            }
        }
        // 右辺 (始まりを +1、終わりを -1)。
        if x1 < self.w {
            let s0 = y0.max(0) + 1;
            let s1 = (if y1 >= self.h { self.h - 1 } else { y1 }) - 1;
            for i in 0..(s1 - s0 + 1) {
                self.bright(x1, s0 + i, shadow);
            }
        }
    }

    /// 元の BlockBevel8: ライン幅ぶん内側へ重ねる。
    fn bevel(&self, x: i64, y: i64, size: i64, height: i64, hilight: f32, shadow: f32) {
        let (mut x, mut y, mut size) = (x, y, size);
        for _ in 0..height {
            if size < 2 {
                break;
            }
            self.bevel_ring(x, y, size, hilight, shadow);
            size -= 2;
            x += 1;
            y += 1;
        }
    }

    /// 元の BlockMosaic8: 範囲すべての平均で塗る。
    fn mosaic(&self, x: i64, y: i64, size: i64) {
        let (mut x0, mut y0) = (x, y);
        let (mut x1, mut y1) = (x + size - 1, y + size - 1);
        if x1 < 0 || x0 >= self.w || y1 < 0 || y0 >= self.h {
            return;
        }
        if x0 < 0 {
            x0 = 0;
        }
        if y0 < 0 {
            y0 = 0;
        }
        if x1 >= self.w {
            x1 = self.w - 1;
        }
        if y1 >= self.h {
            y1 = self.h - 1;
        }
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);

        // 元: 範囲すべてを足して画素数で割る (flat の行ごととは違う)。
        let (mut a, mut r, mut g, mut b) = (0.0f64, 0.0, 0.0, 0.0);
        let count = (bw * bh) as f64;
        for j in 0..bh {
            for i in 0..bw {
                let p = self.get(x0 + i, y0 + j);
                a += p.alpha;
                r += p.red;
                g += p.green;
                b += p.blue;
            }
        }
        let q = |v: f64| -> f64 {
            if self.depth == 32 {
                // 元の 32bit も RoundFpShort なので 0..1 でクランプ。
                (v / count).clamp(0.0, 1.0)
            } else {
                // 元: 整数の割り算で切り捨ててからクランプ。
                ((v as i64) / (count as i64)).clamp(0, self.max as i64) as f64
            }
        };
        let c = Px {
            alpha: q(a),
            red: q(r),
            green: q(g),
            blue: q(b),
        };
        for j in 0..bh {
            for i in 0..bw {
                self.set(x0 + i, y0 + j, c);
            }
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    GridSize,
    Mode,
    Hilight,
    Shadow,
    Height,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn can_load(_host_name: &str, _host_version: &str) -> bool {
        true
    }

    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: InData,
        _: OutData,
    ) -> Result<(), Error> {
        // 元: PF_ADD_SLIDER(グリッドサイズ, 2, 128, 2, 32, 4, ..)
        params.add(
            Params::GridSize,
            "GridSize",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(128);
                f.set_slider_min(2);
                f.set_slider_max(32);
                f.set_default(4);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX(モード, モザイク, FALSE, 0, ..)
        params.add(
            Params::Mode,
            "Mode",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("Mosaic");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED(ハイライト/シャドウ, 0, 200, 0, 200, 120/75, 1, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        for (p, name, dflt) in [
            (Params::Hilight, "Hilight(%)", 120.0),
            (Params::Shadow, "Shadow(%)", 75.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(200.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(200.0);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_SLIDER(ライン幅, 1, 16, 1, 4, 1, ..)
        params.add(
            Params::Height,
            "LineWidth",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(16);
                f.set_slider_min(1);
                f.set_slider_max(4);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: InData,
        mut out_data: OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg("fs-rs videogrid");
            }
            ae::Command::GlobalSetup => {
                self.global_setup(&in_data)?;
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                if !in_data.is_premiere() {
                    return Err(Error::BadCallbackParameter);
                }
                self.do_render(&in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { extra } => {
                self.smart_pre_render(&in_data, extra)?;
            }
            ae::Command::SmartRender { extra } => {
                let cb = extra.callbacks();
                let Some(input_world) = cb.checkout_layer_pixels(0)? else {
                    return Ok(());
                };
                if let Ok(Some(output_world)) = cb.checkout_output() {
                    self.do_render(&in_data, input_world, output_world, params)?;
                }
                cb.checkin_layer_pixels(0)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn global_setup(&mut self, in_data: &InData) -> Result<(), ae::Error> {
        win_dbg_logger::DEBUGGER_LOGGER.set_force_log_without_debugger(true);
        log::info!("GlobalSetup");
        if in_data.is_premiere() {
            let suite = ae::pf::suites::PixelFormat::new()?;
            suite.clear_supported_pixel_formats(in_data.effect_ref())?;
            for x in [
                ae::pr::PixelFormat::Bgra4444_8u,
                ae::pr::PixelFormat::Bgra4444_16u,
                ae::pr::PixelFormat::Bgra4444_32f,
            ] {
                suite.add_supported_pixel_format(in_data.effect_ref(), x)?;
            }
        }
        Ok(())
    }

    fn smart_pre_render(
        &mut self,
        in_data: &InData,
        mut extra: ae::PreRenderExtra,
    ) -> Result<(), ae::Error> {
        let req = extra.output_request();
        if let Ok(in_result) = extra.callbacks().checkout_layer(
            0,
            0,
            &req,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale(),
        ) {
            let _ = extra.union_result_rect(in_result.result_rect.into());
            let _ = extra.union_max_result_rect(in_result.max_result_rect.into());
        }
        Ok(())
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        // 元の GetParams: downScale してから 2 で下止め。
        let grid_size = {
            let v = params.get(Params::GridSize)?.as_slider()?.value() as i64;
            let ds = in_data.downsample_x();
            let v = if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            };
            v.max(2)
        };
        let is_mosaic = params.get(Params::Mode)?.as_checkbox()?.value();
        // 元: (PF_FpShort)固定小数 / 6553600 なので 0..200% が 0..2。
        let pct = |p: Params, params: &mut ae::Parameters<Params>| -> Result<f32, Error> {
            let f = libs::utils::flt2fix(params.get(p)?.as_float_slider()?.value());
            Ok(f as f32 / 6_553_600.0)
        };
        let hilight = pct(Params::Hilight, params)?;
        let shadow = pct(Params::Shadow, params)?;
        // 元: ライン幅は桝の半分で止める (整数の割り算)。1 より小さければ 1。
        let height = {
            let v = params.get(Params::Height)?.as_slider()?.value() as i64;
            let half = grid_size / 2;
            let v = if v > half { half } else { v };
            v.max(1)
        };

        // 元の Render: まず入力をそのままコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;

        // 元の Exec: モザイクを使わず、明暗がどちらも 1.0 なら何もしない。
        if !is_mosaic && hilight == 1.0 && shadow == 1.0 {
            return Ok(());
        }

        let depth = out_layer.bit_depth();
        let canvas = Canvas {
            layer: &out_layer,
            depth,
            w: out_layer.width() as i64,
            h: out_layer.height() as i64,
            max: match depth {
                8 => MAX_CHANNEL8 as f64,
                16 => MAX_CHANNEL16 as f64,
                _ => 1.0,
            },
        };

        // 元: 桝の数は切り上げ。
        let mut loop_x = canvas.w / grid_size;
        if canvas.w % grid_size != 0 {
            loop_x += 1;
        }
        let mut loop_y = canvas.h / grid_size;
        if canvas.h % grid_size != 0 {
            loop_y += 1;
        }

        let bevel_on = !(hilight == 1.0 && shadow == 1.0);
        for j in 0..loop_y {
            let y = j * grid_size;
            for i in 0..loop_x {
                let x = i * grid_size;
                if is_mosaic {
                    canvas.mosaic(x, y, grid_size);
                }
                if bevel_on {
                    canvas.bevel(x, y, grid_size, height, hilight, shadow);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(a: f64, r: f64, g: f64, b: f64) -> Px {
        Px {
            alpha: a,
            red: r,
            green: g,
            blue: b,
        }
    }

    #[test]
    fn line_bright_passes_through_at_one() {
        let c = px(255.0, 10.0, 20.0, 30.0);
        let r = line_bright(c, 1.0, 255.0);
        assert_eq!((r.red, r.green, r.blue), (10.0, 20.0, 30.0));
        // アルファは触らない。
        assert_eq!(r.alpha, 255.0);
    }

    #[test]
    fn line_bright_goes_black_and_white_at_the_ends() {
        let c = px(255.0, 10.0, 20.0, 30.0);
        let r = line_bright(c, 0.0, 255.0);
        assert_eq!((r.red, r.green, r.blue), (0.0, 0.0, 0.0));
        let r = line_bright(c, 2.0, 255.0);
        assert_eq!((r.red, r.green, r.blue), (255.0, 255.0, 255.0));
    }

    #[test]
    fn line_bright_scales_below_one_and_lifts_above() {
        let c = px(255.0, 100.0, 100.0, 100.0);
        // 0.5 なら半分。
        assert_eq!(line_bright(c, 0.5, 255.0).red, 50.0);
        // 1.5 なら残りの半分ぶん白に寄る。
        assert_eq!(line_bright(c, 1.5, 255.0).red, 100.0 + 0.5 * 155.0);
    }

    #[test]
    fn the_line_width_is_capped_at_half_the_grid() {
        let cap = |v: i64, grid: i64| {
            let half = grid / 2;
            let v = if v > half { half } else { v };
            v.max(1)
        };
        assert_eq!(cap(4, 4), 2);
        assert_eq!(cap(1, 4), 1);
        assert_eq!(cap(8, 32), 8);
        // 桝が 2 なら半分は 1。
        assert_eq!(cap(4, 2), 1);
        // 桝が 3 でも半分は 1 (整数の割り算)。
        assert_eq!(cap(4, 3), 1);
    }

    #[test]
    fn the_percentage_becomes_zero_to_two() {
        let f = |v: f64| libs::utils::flt2fix(v) as f32 / 6_553_600.0;
        assert_eq!(f(100.0), 1.0);
        assert_eq!(f(0.0), 0.0);
        assert_eq!(f(200.0), 2.0);
        assert!((f(120.0) - 1.2).abs() < 1e-6);
    }
}
