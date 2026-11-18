//! F's CellLineEraser の移植 (元のプロジェクト名は CellLineEraser_old)。
//!
//! セル画の線画を消すためのもの。選んだ色の画素をいったん「目印」
//! (RGB = 0,0,0) に塗り替えてから、目印の画素を周り8近傍のいちばん暗い色で
//! 塗りつぶすのを、変化が止まるまで (最大100回) 繰り返す。最後に残った
//! 目印を白にするか、透明にするか、指定色で埋める。
//!
//! 元から写した細かいところ:
//!
//! - 目印とぶつかる「元から真っ黒」の画素は 1 / 128 / 0.004 にずらす
//! - 明るさは 30R + 59G + 11B (整数の重み) を double で計算して比べる
//! - 16/32bit でも色の判定は 8bit に落としてから。落とし方も元のまま
//!   (16bit は `(v*255 + 16384)/32768` の整数演算、32bit は
//!   `v*255.0f + 0.5f` を f32 で計算して切り捨て)
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/CellLineEraser_old

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

/// 元の LUME_R / LUME_G / LUME_B。
const LUME: [f64; 3] = [30.0, 59.0, 11.0];

/// 画素は深度そのままの目盛りを f64 で持つ (a, r, g, b)。
/// 比較とコピーしかしないので、どの深度でも値はそのまま往復する。
type Px = [f64; 4];
const A: usize = 0;
const R: usize = 1;
const G: usize = 2;
const B: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Depth {
    Eight,
    Sixteen,
    ThirtyTwo,
}

impl Depth {
    /// 元の IS_WHITE。32bit は 1.0 以上を白とみなす。
    fn is_white(self, p: &Px) -> bool {
        match self {
            Depth::Eight => {
                p[R] == MAX_CHANNEL8 as f64
                    && p[G] == MAX_CHANNEL8 as f64
                    && p[B] == MAX_CHANNEL8 as f64
            }
            Depth::Sixteen => {
                p[R] == MAX_CHANNEL16 as f64
                    && p[G] == MAX_CHANNEL16 as f64
                    && p[B] == MAX_CHANNEL16 as f64
            }
            Depth::ThirtyTwo => p[R] >= 1.0 && p[G] >= 1.0 && p[B] >= 1.0,
        }
    }

    fn white(self) -> f64 {
        match self {
            Depth::Eight => MAX_CHANNEL8 as f64,
            Depth::Sixteen => MAX_CHANNEL16 as f64,
            Depth::ThirtyTwo => 1.0,
        }
    }

    /// 目印 (0,0,0) とぶつからないようにずらす値。
    fn nudge(self) -> f64 {
        match self {
            Depth::Eight => 1.0,
            Depth::Sixteen => 128.0,
            // 元は 0.004f なので f32 の値そのまま。
            Depth::ThirtyTwo => 0.004f32 as f64,
        }
    }

    /// 元の IsTargetColor の 8bit 化。
    fn to8(self, p: &Px) -> [u8; 3] {
        let mut out = [0u8; 3];
        for (i, ch) in [R, G, B].iter().enumerate() {
            let v = p[*ch];
            out[i] = match self {
                Depth::Eight => v as u8,
                // 元: (v * 255 + 16384) / 32768 の整数演算。
                Depth::Sixteen => ((v as i64 * MAX_CHANNEL8 as i64 + 16384)
                    / MAX_CHANNEL16 as i64) as u8,
                // 元: v * 255.0f + 0.5f を f32 で計算して切り捨て。
                // 1.0 を超える値では元は範囲外の変換になる (Rust は 255 で止まる)。
                Depth::ThirtyTwo => ((v as f32) * MAX_CHANNEL8 as f32 + 0.5f32) as u8,
            };
        }
        out
    }
}

/// 元の IS_MARKER。
fn is_marker(p: &Px) -> bool {
    p[R] == 0.0 && p[G] == 0.0 && p[B] == 0.0
}

struct Img {
    w: usize,
    h: usize,
    px: Vec<Px>,
}

impl Img {
    fn at(&self, x: usize, y: usize) -> &Px {
        &self.px[y * self.w + x]
    }
}

struct Fill {
    depth: Depth,
    targets: Vec<[u8; 3]>,
    keep_pixel: bool,
    fill_unknown: bool,
    give_up: Px,
    make_white_transparent: bool,
}

impl Fill {
    /// 元の IsTargetColor。
    fn is_target(&self, p: &Px) -> bool {
        if p[A] == 0.0 {
            return false;
        }
        let p8 = self.depth.to8(p);
        self.targets.iter().any(|t| *t == p8)
    }

    /// 元の UnifyToMarkerIterate (パス1)。
    fn unify(&self, p: &Px) -> Px {
        if self.is_target(p) {
            [p[A], 0.0, 0.0, 0.0]
        } else if self.make_white_transparent && self.depth.is_white(p) {
            [0.0, 0.0, 0.0, 0.0]
        } else if is_marker(p) {
            let n = self.depth.nudge();
            [p[A], n, n, n]
        } else {
            *p
        }
    }

    /// 元の DarkestFillIterate (パス2を1回)。変えた画素数を返す。
    fn darkest(&self, src: &Img) -> (Img, usize) {
        let mut out = Vec::with_capacity(src.px.len());
        let mut changed = 0usize;
        for y in 0..src.h {
            for x in 0..src.w {
                let p = src.at(x, y);
                let mut done = false;
                if is_marker(p) && p[A] != 0.0 {
                    let mut darkest = *p;
                    let mut min_lum = 2_000_000_000.0f64;
                    let mut found = false;
                    for ky in -1i64..=1 {
                        for kx in -1i64..=1 {
                            let tx = x as i64 + kx;
                            let ty = y as i64 + ky;
                            if tx < 0 || tx >= src.w as i64 || ty < 0 || ty >= src.h as i64 {
                                continue;
                            }
                            let n = src.at(tx as usize, ty as usize);
                            if n[A] != 0.0 && !is_marker(n) && !self.depth.is_white(n) {
                                let lum =
                                    LUME[0] * n[R] + LUME[1] * n[G] + LUME[2] * n[B];
                                if lum < min_lum {
                                    min_lum = lum;
                                    darkest = *n;
                                    found = true;
                                }
                            }
                        }
                    }
                    if found {
                        out.push(darkest);
                        changed += 1;
                        done = true;
                    }
                }
                if !done {
                    out.push(*p);
                }
            }
        }
        (
            Img {
                w: src.w,
                h: src.h,
                px: out,
            },
            changed,
        )
    }

    /// 元の FinalFillIterate (最終パス)。
    fn finish(&self, p: &mut Px) {
        if !(is_marker(p) && p[A] != 0.0) {
            return;
        }
        if !self.keep_pixel {
            if self.make_white_transparent {
                p[A] = 0.0;
            } else {
                let w = self.depth.white();
                p[R] = w;
                p[G] = w;
                p[B] = w;
            }
        } else if self.fill_unknown {
            *p = self.give_up;
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Count,
    Color1,
    Color2,
    Color3,
    Color4,
    Color5,
    Color6,
    Color7,
    Color8,
    Color9,
    Color10,
    KeepPixels,
    FillUnknown,
    FillColor,
    MakeWhiteTransparent,
}

const COLORS: [Params; 10] = [
    Params::Color1,
    Params::Color2,
    Params::Color3,
    Params::Color4,
    Params::Color5,
    Params::Color6,
    Params::Color7,
    Params::Color8,
    Params::Color9,
    Params::Color10,
];

/// 元の PF_ADD_COLOR の既定色。
const DEFAULTS: [[u8; 3]; 10] = [
    [0x00, 0x00, 0x00],
    [0xFF, 0x00, 0x00],
    [0x00, 0xFF, 0x00],
    [0x00, 0x00, 0xFF],
    [0x80, 0x00, 0x00],
    [0x00, 0x80, 0x00],
    [0x00, 0x00, 0x80],
    [0x40, 0x00, 0x00],
    [0x00, 0x40, 0x00],
    [0x00, 0x00, 0x40],
];

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
        // 元: PF_ADD_SLIDER(STR_Use, 0, 10, 0, 10, 0, ..)
        params.add(
            Params::Count,
            "TargetColorCount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(10);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;

        for (i, p) in COLORS.iter().enumerate() {
            let rgb = DEFAULTS[i];
            params.add(
                *p,
                &format!("color{}", i + 1),
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
            Params::KeepPixels,
            "KeepPixels",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::FillUnknown,
            "Fill unremoved pixels",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::FillColor,
            "FillColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0x00,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::MakeWhiteTransparent,
            "Make White Transparent",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
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
                out_data.set_return_msg("fs-rs celllineeraser");
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
        _in_data: &ae::InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let count = params.get(Params::Count)?.as_slider()?.value().clamp(0, 10) as usize;
        let mut targets = Vec::new();
        for p in COLORS.iter().take(count) {
            let c = params.get(*p)?.as_color()?.value();
            targets.push([c.red, c.green, c.blue]);
        }
        let keep_pixel = params.get(Params::KeepPixels)?.as_checkbox()?.value();
        let fill_unknown = params.get(Params::FillUnknown)?.as_checkbox()?.value();
        let give_up8 = params.get(Params::FillColor)?.as_color()?.value();
        let make_white_transparent = params
            .get(Params::MakeWhiteTransparent)?
            .as_checkbox()?
            .value();

        let depth = match out_layer.bit_depth() {
            8 => Depth::Eight,
            16 => Depth::Sixteen,
            _ => Depth::ThirtyTwo,
        };

        // 元: GiveUpColor16 = CONV8TO16(c), GiveUpColor32 = CONV8TO32(c)。
        let give_up: Px = match depth {
            Depth::Eight => [
                give_up8.alpha as f64,
                give_up8.red as f64,
                give_up8.green as f64,
                give_up8.blue as f64,
            ],
            Depth::Sixteen => {
                let c = libs::utils::conv_8_to_16(&give_up8);
                [
                    c.alpha as f64,
                    c.red as f64,
                    c.green as f64,
                    c.blue as f64,
                ]
            }
            Depth::ThirtyTwo => {
                let c = libs::utils::conv_8_to_32(&give_up8);
                [
                    c.alpha as f64,
                    c.red as f64,
                    c.green as f64,
                    c.blue as f64,
                ]
            }
        };

        let fill = Fill {
            depth,
            targets,
            keep_pixel,
            fill_unknown,
            give_up,
            make_white_transparent,
        };

        let (w, h) = (out_layer.width(), out_layer.height());

        // 入力を読み込みつつパス1 (元の UnifyToMarkerIterate)。
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let p: Px = match depth {
                    Depth::Eight => {
                        let q = in_layer.as_pixel8(x, y);
                        [
                            q.alpha as f64,
                            q.red as f64,
                            q.green as f64,
                            q.blue as f64,
                        ]
                    }
                    Depth::Sixteen => {
                        let q = in_layer.as_pixel16(x, y);
                        [
                            q.alpha as f64,
                            q.red as f64,
                            q.green as f64,
                            q.blue as f64,
                        ]
                    }
                    Depth::ThirtyTwo => {
                        let q = in_layer.as_pixel32(x, y);
                        [
                            q.alpha as f64,
                            q.red as f64,
                            q.green as f64,
                            q.blue as f64,
                        ]
                    }
                };
                px.push(fill.unify(&p));
            }
        }
        let mut cur = Img { w, h, px };

        // 元の STEP 2: 変化が止まるまで、最大100回。
        for _ in 0..100 {
            let (next, changed) = fill.darkest(&cur);
            if changed == 0 {
                break;
            }
            cur = next;
        }

        // 元の STEP 3。
        for p in cur.px.iter_mut() {
            fill.finish(p);
        }

        for y in 0..h {
            for x in 0..w {
                let p = cur.at(x, y);
                match depth {
                    Depth::Eight => {
                        *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                            alpha: p[A] as u8,
                            red: p[R] as u8,
                            green: p[G] as u8,
                            blue: p[B] as u8,
                        }
                    }
                    Depth::Sixteen => {
                        *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                            alpha: p[A] as u16,
                            red: p[R] as u16,
                            green: p[G] as u16,
                            blue: p[B] as u16,
                        }
                    }
                    Depth::ThirtyTwo => {
                        *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                            alpha: p[A] as f32,
                            red: p[R] as f32,
                            green: p[G] as f32,
                            blue: p[B] as f32,
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fill8(targets: Vec<[u8; 3]>) -> Fill {
        Fill {
            depth: Depth::Eight,
            targets,
            keep_pixel: false,
            fill_unknown: false,
            give_up: [255.0, 255.0, 0.0, 255.0],
            make_white_transparent: false,
        }
    }

    #[test]
    fn unify_turns_targets_into_markers() {
        let f = fill8(vec![[255, 0, 0]]);
        assert_eq!(f.unify(&[255.0, 255.0, 0.0, 0.0]), [255.0, 0.0, 0.0, 0.0]);
        // 対象でなければそのまま。
        assert_eq!(f.unify(&[255.0, 10.0, 20.0, 30.0]), [255.0, 10.0, 20.0, 30.0]);
    }

    #[test]
    fn unify_nudges_pixels_that_are_already_black() {
        let f = fill8(vec![]);
        assert_eq!(f.unify(&[255.0, 0.0, 0.0, 0.0]), [255.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn unify_skips_transparent_pixels_when_matching() {
        // 元: alpha == 0 なら対象色でも対象にしない。
        let f = fill8(vec![[255, 0, 0]]);
        assert_eq!(f.unify(&[0.0, 255.0, 0.0, 0.0]), [0.0, 255.0, 0.0, 0.0]);
    }

    #[test]
    fn darkest_fill_takes_the_darkest_neighbour() {
        let f = fill8(vec![]);
        // 中央が目印、周りに明るい色と暗い色。
        let px = vec![
            [255.0, 200.0, 200.0, 200.0],
            [255.0, 10.0, 20.0, 30.0],
            [255.0, 180.0, 180.0, 180.0],
            [255.0, 0.0, 0.0, 0.0],
        ];
        let img = Img { w: 2, h: 2, px };
        let (out, changed) = f.darkest(&img);
        assert_eq!(changed, 1);
        assert_eq!(*out.at(1, 1), [255.0, 10.0, 20.0, 30.0]);
    }

    #[test]
    fn darkest_fill_leaves_a_marker_with_no_usable_neighbour() {
        let f = fill8(vec![]);
        let px = vec![[255.0, 0.0, 0.0, 0.0], [255.0, 255.0, 255.0, 255.0]];
        let img = Img { w: 2, h: 1, px };
        let (out, changed) = f.darkest(&img);
        // 白は候補にならないので変わらない。
        assert_eq!(changed, 0);
        assert_eq!(*out.at(0, 0), [255.0, 0.0, 0.0, 0.0]);
    }

    #[test]
    fn finish_whitens_leftover_markers() {
        let f = fill8(vec![]);
        let mut p = [255.0, 0.0, 0.0, 0.0];
        f.finish(&mut p);
        assert_eq!(p, [255.0, 255.0, 255.0, 255.0]);
    }

    #[test]
    fn finish_keeps_markers_when_asked() {
        let mut f = fill8(vec![]);
        f.keep_pixel = true;
        let mut p = [255.0, 0.0, 0.0, 0.0];
        f.finish(&mut p);
        assert_eq!(p, [255.0, 0.0, 0.0, 0.0]);
        // 埋める指定があれば指定色。
        f.fill_unknown = true;
        let mut p = [255.0, 0.0, 0.0, 0.0];
        f.finish(&mut p);
        assert_eq!(p, [255.0, 255.0, 0.0, 255.0]);
    }

    #[test]
    fn sixteen_bit_colour_match_goes_through_eight_bit() {
        // 元: (v * 255 + 16384) / 32768。
        assert_eq!(Depth::Sixteen.to8(&[0.0, 32768.0, 16384.0, 0.0]), [255, 128, 0]);
    }
}
