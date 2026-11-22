//! F's UsedColorList の移植。
//!
//! 選んだ範囲に使われている色を数え、多い順に桝目で並べて見せる。
//! あわせて範囲の枠を、下の色を反転した色で描く。塗り分けの確認用。
//!
//! 元から写した細かいところ:
//!
//! - 色は**完全一致**でまとめる (RGB がそのまま同じもの同士)。アルファが
//!   最大でない画素、真っ黒、真っ白、それに除外色3つは数えない
//! - 数えるのは先頭 64 色まで。64 に達したらそこで走査をやめる
//! - 並べ替えは元と同じ「数の少ない順に並べてから前後をひっくり返す」
//!   ので、同じ数の色の順番まで元のまま
//! - 範囲は `RectCheck` を通す: 入れ替え → 左上を 0 で止める →
//!   右下を幅・高さで止める → 4未満なら左上 +4 まで広げる。
//!   **最後の「+4」のあとは止め直さない**ので範囲が画像の外に出ることが
//!   あるが、元の `SetPix8` は外を黙って捨て、`GetPix8` は端に寄せて読む
//!   のでそのまま写してある
//! - 枠は内側2画素ぶん。横の帯を先に全部描いてから縦の帯を描く
//! - 桝目の並びは `i / sy` 列目・`i % sy` 行目 (縦に詰めてから次の列)
//!
//! 元と違えたところ:
//!
//! - **桝が画像より高いと 0 除算で落ちる**のを直した。元は
//!   `sy = height()/GridHeight` をそのまま `i / sy` に使うので、
//!   GridHeight を画像の高さより大きくすると落ちる (300 まで上げられる
//!   のでUIから踏める)。1 で下止めして、その場合は1列に並べる
//! - **元は 8bit しか動かない**。`NFsWorld` は深度ごとに別のポインタを
//!   持つのに、この処理は `GetPix8` / `SetPix8` しか呼ばないので、
//!   16/32bit のワールドでは NULL を触って落ちる。出荷 .aex は深い色を
//!   申告していないので AE から 8bit を渡されていて落ちない
//!   (16bit で描かせても 8bit と同じ絵になる)。ここでは 16/32bit でも
//!   同じ結果になるように、色を数えるときは 8bit に落とし、描くときに
//!   その深度へ戻している
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/UsedColorList

use after_effects::{self as ae};
use libs::utils::{conv_8_to_16, conv_8_to_32, round_byte_fp_double};

/// 元の ColorScan が数える上限。
const COLOR_MAX: usize = 64;

/// 元の A_Rect。右と下は「含まない」。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rect {
    left: i64,
    top: i64,
    right: i64,
    bottom: i64,
}

/// 元の RevColor。下の色をアルファぶん薄めてから反転し、不透明にする。
fn rev_color(p: ae::Pixel8) -> ae::Pixel8 {
    let op = p.alpha as f64 / 255.0;
    ae::Pixel8 {
        alpha: 255,
        red: round_byte_fp_double(255.0 - p.red as f64 * op + 0.5),
        green: round_byte_fp_double(255.0 - p.green as f64 * op + 0.5),
        blue: round_byte_fp_double(255.0 - p.blue as f64 * op + 0.5),
    }
}

/// 8bit の絵。深度に関係なく、ここで数えて描いてから元の深度へ戻す。
struct Canvas {
    w: i64,
    h: i64,
    px: Vec<ae::Pixel8>,
}

impl Canvas {
    /// 元の GetPix8: 範囲の外は端に寄せて読む。
    fn get(&self, x: i64, y: i64) -> ae::Pixel8 {
        let x = x.clamp(0, self.w - 1);
        let y = y.clamp(0, self.h - 1);
        self.px[(y * self.w + x) as usize]
    }

    /// 元の SetPix8: 範囲の外は黙って捨てる。
    fn set(&mut self, x: i64, y: i64, c: ae::Pixel8) {
        if x < 0 || x >= self.w || y < 0 || y >= self.h {
            return;
        }
        self.px[(y * self.w + x) as usize] = c;
    }

    /// 元の RectCheck。最後の「4未満なら広げる」のあとは止め直さない。
    fn rect_check(&self, r: &mut Rect) {
        if r.left > r.right {
            std::mem::swap(&mut r.left, &mut r.right);
        }
        if r.top > r.bottom {
            std::mem::swap(&mut r.top, &mut r.bottom);
        }
        if r.left < 0 {
            r.left = 0;
        }
        if r.top < 0 {
            r.top = 0;
        }
        if r.right > self.w {
            r.right = self.w;
        }
        if r.bottom > self.h {
            r.bottom = self.h;
        }
        if r.right - r.left < 4 {
            r.right = r.left + 4;
        }
        if r.bottom - r.top < 4 {
            r.bottom = r.top + 4;
        }
    }

    /// 元の DrawRect。内側2画素ぶんの枠を、下の色の反転で描く。
    fn draw_rect(&mut self, mut r: Rect) {
        self.rect_check(&mut r);
        for x in r.left..r.right {
            for yy in [r.top, r.top + 1, r.bottom - 1, r.bottom - 2] {
                let c = self.get(x, yy);
                self.set(x, yy, rev_color(c));
            }
        }
        for y in (r.top + 2)..(r.bottom - 2) {
            for xx in [r.left, r.left + 1, r.right - 1, r.right - 2] {
                let c = self.get(xx, y);
                self.set(xx, y, rev_color(c));
            }
        }
    }

    /// 元の DrawGrid。
    fn draw_grid(&mut self, col: ae::Pixel8, mut r: Rect) {
        self.rect_check(&mut r);
        for j in r.top..r.bottom {
            for i in r.left..r.right {
                self.set(i, j, col);
            }
        }
    }

    /// 元の ColorScan。使われている色とその数を、多い順に返す。
    /// `area` は元と同じく RectCheck を通して書き換える。
    fn color_scan(&self, area: &mut Rect, except: &[ae::Pixel8; 3]) -> Vec<ae::Pixel8> {
        self.rect_check(area);
        let mut colors: Vec<ae::Pixel8> = Vec::new();
        let mut counts: Vec<i64> = Vec::new();

        'outer: for y in area.top..area.bottom {
            for x in area.left..area.right {
                if colors.len() >= COLOR_MAX {
                    break 'outer;
                }
                let c = self.get(x, y);
                // 元: 不透明でない画素は数えない。
                if c.alpha != 255 {
                    continue;
                }
                // 元: 真っ黒と真っ白は数えない。
                if c.red == 0 && c.blue == 0 && c.green == 0 {
                    continue;
                }
                if c.red == 255 && c.blue == 255 && c.green == 255 {
                    continue;
                }
                // 元: 除外色3つ (アルファは見ない)。
                if except
                    .iter()
                    .any(|e| e.red == c.red && e.green == c.green && e.blue == c.blue)
                {
                    continue;
                }
                match colors
                    .iter()
                    .position(|t| t.red == c.red && t.green == c.green && t.blue == c.blue)
                {
                    Some(i) => counts[i] += 1,
                    None => {
                        colors.push(c);
                        counts.push(1);
                    }
                }
            }
        }

        if colors.is_empty() {
            return colors;
        }
        // 元: 数の少ない順にバブルソートしてから、前後をひっくり返す。
        // 同じ数のときの順番まで元に合わせるため、そのまま写してある。
        let n = colors.len();
        for i in 0..n.saturating_sub(1) {
            let mut j = n - 1;
            while i < j {
                if counts[j] < counts[j - 1] {
                    colors.swap(j, j - 1);
                    counts.swap(j, j - 1);
                }
                j -= 1;
            }
        }
        let half = n / 2;
        for i in 0..half {
            let ii = n - i - 1;
            colors.swap(i, ii);
            counts.swap(i, ii);
        }
        colors
    }

    /// 元の DrawList。桝目に並べる。
    fn draw_list(&mut self, colors: &[ae::Pixel8], grid_w: i64, grid_h: i64, down: f64) {
        if colors.is_empty() {
            return;
        }
        // 元: (int)((double)GridWidth * downScale())。
        let gw = (grid_w as f64 * down) as i64;
        let gh = (grid_h as f64 * down) as i64;
        if gw <= 0 || gh <= 0 {
            return;
        }
        // 元は `sy = height()/gh` のまま `i / sy` と `i % sy` に使うので、
        // 桝が画像より高いと sy が 0 になって 0 除算で落ちる
        // (GridHeight は 300 まで上げられるので UI から踏める)。
        // 1 で下止めして、その場合は1列に並べる。
        let sy = (self.h / gh).max(1) as i64;
        for (i, col) in colors.iter().enumerate() {
            let i = i as i64;
            let left = (i / sy) * gw;
            let top = (i % sy) * gh;
            self.draw_grid(
                *col,
                Rect {
                    left,
                    top,
                    right: left + gw,
                    bottom: top + gh,
                },
            );
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Pos0,
    Pos1,
    ExceptColor0,
    ExceptColor1,
    ExceptColor2,
    IsDisp,
    GridWidth,
    GridHeight,
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
        // 元: PF_ADD_POINT(STR_POS0, 50, 50, 0, ..) / (STR_POS1, 75, 75, 0, ..)
        for (p, name, xy) in [
            (Params::Pos0, "PosStart", (50.0f32, 50.0f32)),
            (Params::Pos1, "PosEnd", (75.0, 75.0)),
        ] {
            params.add(
                p,
                name,
                ae::PointDef::setup(|f| {
                    f.set_default(xy);
                    f.set_value(f.default());
                }),
            )?;
        }
        for (p, name, rgb) in [
            (Params::ExceptColor0, "ExceptColor0", [0xFF, 0x00, 0x00]),
            (Params::ExceptColor1, "ExceptColor1", [0x00, 0xFF, 0x00]),
            (Params::ExceptColor2, "ExceptColor2", [0x00, 0x00, 0xFF]),
        ] {
            params.add(
                p,
                name,
                ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: rgb[0],
                        green: rgb[1],
                        blue: rgb[2],
                    });
                    f.set_value(f.default());
                }),
            )?;
        }
        params.add(
            Params::IsDisp,
            "Disp",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(true);
                f.set_value(f.default());
            }),
        )?;
        // 元の綴りのまま ("GridWidrh")。保存した値を引き継ぐため直していない。
        for (p, name) in [
            (Params::GridWidth, "GridWidrh"),
            (Params::GridHeight, "GridHeight"),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(10);
                    f.set_valid_max(300);
                    f.set_slider_min(10);
                    f.set_slider_max(100);
                    f.set_default(50);
                    f.set_value(f.default());
                }),
            )?;
        }
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
                out_data.set_return_msg("fs-rs usedcolorlist");
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
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        // 元の GetParams: 点は +0.5 して丸める。
        let p0 = params.get(Params::Pos0)?.as_point()?.value();
        let p1 = params.get(Params::Pos1)?.as_point()?.value();
        let mut area = Rect {
            left: (p0.0 as f64 + 0.5) as i64,
            top: (p0.1 as f64 + 0.5) as i64,
            right: (p1.0 as f64 + 0.5) as i64,
            bottom: (p1.1 as f64 + 0.5) as i64,
        };
        // 元: ここで入れ替えて、左上だけ 0 で止める (右下は RectCheck 任せ)。
        if area.left > area.right {
            std::mem::swap(&mut area.left, &mut area.right);
        }
        if area.top > area.bottom {
            std::mem::swap(&mut area.top, &mut area.bottom);
        }
        if area.left < 0 {
            area.left = 0;
        }
        if area.top < 0 {
            area.top = 0;
        }

        let is_disp = params.get(Params::IsDisp)?.as_checkbox()?.value();
        let grid_w = params.get(Params::GridWidth)?.as_slider()?.value() as i64;
        let grid_h = params.get(Params::GridHeight)?.as_slider()?.value() as i64;
        let except = [
            params.get(Params::ExceptColor0)?.as_color()?.value(),
            params.get(Params::ExceptColor1)?.as_color()?.value(),
            params.get(Params::ExceptColor2)?.as_color()?.value(),
        ];

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をそのままコピーしてから、その上に描く。
        // 元は 8bit しか扱えないので、ここで 8bit に落として処理する
        // (上の説明を参照)。
        let mut canvas = Canvas {
            w: w as i64,
            h: h as i64,
            px: Vec::with_capacity(w * h),
        };
        for y in 0..h {
            for x in 0..w {
                let c = match depth {
                    8 => *in_layer.as_pixel8(x, y),
                    16 => libs::utils::conv_16_to_8(in_layer.as_pixel16(x, y)),
                    _ => libs::utils::conv_32_to_8(in_layer.as_pixel32(x, y)),
                };
                canvas.px.push(c);
            }
        }

        if is_disp {
            let colors = canvas.color_scan(&mut area, &except);
            let ds = in_data.downsample_x();
            let down = if ds.den <= 0 {
                1.0
            } else {
                ds.num as f64 / ds.den as f64
            };
            canvas.draw_list(&colors, grid_w, grid_h, down);
        }
        // 元: 枠は Disp に関係なく描く。
        canvas.draw_rect(area);

        for y in 0..h {
            for x in 0..w {
                let c = canvas.px[y * w + x];
                match depth {
                    8 => *out_layer.as_pixel8_mut(x, y) = c,
                    16 => *out_layer.as_pixel16_mut(x, y) = conv_8_to_16(&c),
                    _ => *out_layer.as_pixel32_mut(x, y) = conv_8_to_32(&c),
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(a: u8, r: u8, g: u8, b: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: a,
            red: r,
            green: g,
            blue: b,
        }
    }

    /// PF_Pixel は PartialEq を持たないので、比べるときは組にする。
    fn t(p: ae::Pixel8) -> (u8, u8, u8, u8) {
        (p.alpha, p.red, p.green, p.blue)
    }

    fn ts(v: &[ae::Pixel8]) -> Vec<(u8, u8, u8, u8)> {
        v.iter().map(|p| t(*p)).collect()
    }

    fn canvas(w: i64, h: i64, fill: ae::Pixel8) -> Canvas {
        Canvas {
            w,
            h,
            px: vec![fill; (w * h) as usize],
        }
    }

    #[test]
    fn rev_color_inverts_and_makes_it_opaque() {
        assert_eq!(t(rev_color(px(255, 0, 0, 0))), t(px(255, 255, 255, 255)));
        assert_eq!(t(rev_color(px(255, 255, 255, 255))), t(px(255, 0, 0, 0)));
        // アルファが 0 なら薄めた結果は 0 なので、反転して真っ白。
        assert_eq!(t(rev_color(px(0, 200, 100, 50))), t(px(255, 255, 255, 255)));
    }

    #[test]
    fn rect_check_enforces_a_minimum_without_clamping_again() {
        let c = canvas(10, 10, px(0, 0, 0, 0));
        // 入れ替えと左上の止め。
        let mut r = Rect {
            left: 8,
            top: 9,
            right: 2,
            bottom: 1,
        };
        c.rect_check(&mut r);
        assert_eq!(r.left, 2);
        assert_eq!(r.top, 1);
        assert_eq!(r.right, 8);
        assert_eq!(r.bottom, 9);
        // 幅が足りないときは右へ広げる。元はそのあと止め直さないので
        // 画像の外に出る。
        let mut r = Rect {
            left: 9,
            top: 9,
            right: 10,
            bottom: 10,
        };
        c.rect_check(&mut r);
        assert_eq!(r.right, 13);
        assert_eq!(r.bottom, 13);
    }

    #[test]
    fn set_outside_is_dropped_and_get_outside_is_clamped() {
        let mut c = canvas(4, 4, px(255, 10, 20, 30));
        c.set(-1, 0, px(255, 1, 1, 1));
        c.set(4, 0, px(255, 1, 1, 1));
        assert_eq!(t(c.get(0, 0)), t(px(255, 10, 20, 30)));
        // 読みは端に寄る。
        c.set(3, 3, px(255, 9, 9, 9));
        assert_eq!(t(c.get(99, 99)), t(px(255, 9, 9, 9)));
    }

    #[test]
    fn color_scan_skips_black_white_transparent_and_the_except_colours() {
        let mut c = canvas(4, 4, px(255, 0, 0, 0)); // 真っ黒
        c.set(0, 0, px(255, 255, 255, 255)); // 真っ白
        c.set(1, 0, px(200, 7, 7, 7)); // 不透明でない
        c.set(2, 0, px(255, 10, 20, 30)); // これだけ数える
        c.set(3, 0, px(255, 40, 50, 60)); // 除外色
        let except = [
            px(255, 40, 50, 60),
            px(255, 1, 2, 3),
            px(255, 4, 5, 6),
        ];
        let mut area = Rect {
            left: 0,
            top: 0,
            right: 4,
            bottom: 4,
        };
        let got = c.color_scan(&mut area, &except);
        assert_eq!(ts(&got), ts(&[px(255, 10, 20, 30)]));
    }

    #[test]
    fn color_scan_orders_by_count_descending() {
        let mut c = canvas(4, 1, px(255, 0, 0, 0));
        c.set(0, 0, px(255, 1, 1, 1));
        c.set(1, 0, px(255, 2, 2, 2));
        c.set(2, 0, px(255, 2, 2, 2));
        c.set(3, 0, px(255, 2, 2, 2));
        let except = [px(255, 0, 0, 1), px(255, 0, 0, 2), px(255, 0, 0, 3)];
        let mut area = Rect {
            left: 0,
            top: 0,
            right: 4,
            bottom: 1,
        };
        let got = c.color_scan(&mut area, &except);
        assert_eq!(ts(&got), ts(&[px(255, 2, 2, 2), px(255, 1, 1, 1)]));
    }

    #[test]
    fn color_scan_stops_at_sixty_four_colours() {
        let mut c = canvas(100, 1, px(255, 0, 0, 0));
        for i in 0..100u8 {
            c.set(i as i64, 0, px(255, i.wrapping_add(1), 7, 7));
        }
        let except = [px(0, 0, 0, 0); 3];
        let mut area = Rect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 1,
        };
        let got = c.color_scan(&mut area, &except);
        assert_eq!(got.len(), COLOR_MAX);
    }

    #[test]
    fn draw_rect_draws_a_two_pixel_frame() {
        let mut c = canvas(8, 8, px(255, 0, 0, 0));
        c.draw_rect(Rect {
            left: 0,
            top: 0,
            right: 8,
            bottom: 8,
        });
        // 内側2画素は白 (黒の反転)、真ん中は黒のまま。
        assert_eq!(t(c.get(0, 0)), t(px(255, 255, 255, 255)));
        assert_eq!(t(c.get(1, 1)), t(px(255, 255, 255, 255)));
        assert_eq!(t(c.get(3, 3)), t(px(255, 0, 0, 0)));
        assert_eq!(t(c.get(7, 7)), t(px(255, 255, 255, 255)));
    }
}
