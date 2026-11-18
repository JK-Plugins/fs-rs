//! F's sputteringAlpha の移植。
//!
//! アルファのある画素を「縁」と「中身」に分け、それぞれ別の確率でかたまりを
//! 散らす。縁は近いところに、中身は遠くまで飛ぶ。かたまりの絵柄は
//! sputteringRect / sputteringSplash と同じ `libs::sputtering` の表から引く。
//!
//! 元から写した細かいところ:
//!
//! - 縁の判定は「自分のアルファが最大の 8/10 以上」かつ「8近傍のどれかが
//!   8/10 未満」。境目は整数の割り算なので 8bit は 204、16bit は 26214、
//!   32bit は 0.8。アルファがあって縁でなければ「中身」
//! - 画像のいちばん外側の1画素は縁とみなさない (「中身」になる)
//! - **確率を引く乱数は、透明な画素でも必ず1回引く**。描かない画素でも
//!   乱数列が進むので、ここを飛ばすと以降が全部ずれる
//! - 量 (%) は固定小数のまま 100 で割り、さらに 100 で割る (中身)。
//!   縁のほうは 100 で割ってから 10。整数の割り算を2回通す
//! - 散る距離の上限は、中身が距離の5倍、縁が3倍。さらに 1/6 の確率で
//!   その3倍まで伸びる
//! - 量・縁の量・不透明度の幅のどれかが 0 なら何も描かない
//! - ずらし量は `SetRotLength` に渡した固定小数をそのまま画素座標に足す
//!   (飛び散り系で共通の、二重の目盛り違いが打ち消し合う形)
//!
//! 元と違えたところ: **ColorMax の既定値が 20 なのに色は4つしかない**
//! (飛び散り系で共通)。読み込むときに 1..4 に収めてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/sputteringAlpha

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
use libs::sputtering::{setup_sput_data, sput_draw, SPD_RAND_MAX};
use libs::utils::rot_length;

/// 元の UI_COLOR_MAX_V。
const COLOR_COUNT: usize = 4;

/// 元の buf の3状態。
const TARGET_NONE: u8 = 0;
/// 元の TARGET_SOLID (PF_HALF_CHAN8)。
const TARGET_SOLID: u8 = 128;
/// 元の TARGET_EDGE (PF_MAX_CHAN8)。
const TARGET_EDGE: u8 = 255;

/// 元の「ばらつきを作る」ところ。`scale` は中身なら 5、縁なら 3。
fn pick_offset(rng: &mut XorShift, p_len: i64, scale: i64) -> (i32, i32) {
    // 元: 1/6 の確率で距離を scale 倍、ふだんは距離そのまま。
    let len_max = if rng.fm_rand(5) == 0 {
        p_len * scale
    } else {
        p_len
    };
    // 元: SetRotLength(FM_RAND((360<<4)-1) << 12, FM_RAND(len)) の引数なので、
    // 出荷ビルド (MSVC) の評価順に合わせて距離を先に引く。
    let len = rng.fm_rand(len_max) as i32;
    let angle = (rng.fm_rand((360 << 4) - 1) << 12) as i32;
    rot_length(angle, len)
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Seed,
    Value,
    EdgeValue,
    OpacityRand,
    PointValue,
    PointLength,
    Size,
    Color1,
    ColorTopic,
    ColorMax,
    Color2,
    Color3,
    Color4,
    ColorTopicEnd,
    Org,
}

const EXTRA_COLORS: [(Params, &str, [u8; 3]); 3] = [
    (Params::Color2, "Color2", [0xFF, 0xFF, 0x00]),
    (Params::Color3, "Color3", [0xFF, 0x00, 0xFF]),
    (Params::Color4, "Color4", [0x00, 0xFF, 0xFF]),
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
        // 元: PF_ADD_SLIDER(UI_SEED, 0, 32000, 0, 144, 0, ..)
        params.add(
            Params::Seed,
            "seed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(32000);
                f.set_slider_min(0);
                f.set_slider_max(144);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED(UI_VALUE, 0, 100, 0, 100, 10, 1, 0, 0, ..)
        // 元: PF_ADD_FIXED(UI_EDGE_VALUE, 0, 100, 0, 200, 15, 1, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        for (p, name, slider_max, dflt) in [
            (Params::Value, "value(%)", 100.0, 10.0),
            (Params::EdgeValue, "edge_value(%)", 200.0, 15.0),
        ] {
            params.add(
                p,
                name,
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0);
                    f.set_valid_max(100.0);
                    f.set_slider_min(0.0);
                    f.set_slider_max(slider_max);
                    f.set_precision(1);
                    f.set_curve_tolerance(0.05);
                    f.set_default(dflt);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_FIXED(UI_OPA_RND, 0, 5, 0, 2, 1, 2, 0, 0, ..)
        params.add(
            Params::OpacityRand,
            "opacity_rand",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(5.0);
                f.set_slider_min(0.0);
                f.set_slider_max(2.0);
                f.set_precision(2);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(UI_POINT_VALUE, 1, 100, 1, 50, 20, ..)
        params.add(
            Params::PointValue,
            "PointValue",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(100);
                f.set_slider_min(1);
                f.set_slider_max(50);
                f.set_default(20);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(UI_POINT_LENGTH, 1, 1000, 5, 200, 100, ..)
        params.add(
            Params::PointLength,
            "PointLength",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(1000);
                f.set_slider_min(5);
                f.set_slider_max(200);
                f.set_default(100);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP(UI_SIZE1, 5, 2, "極小|小|中|大|特大", ..)
        // 項目名は元が非 ASCII で出荷 .aex では文字化けしているので英語。
        params.add(
            Params::Size,
            "size",
            ae::PopupDef::setup(|f| {
                f.set_options(&["XS", "S", "M", "L", "XL"]);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::Color1,
            "Color1",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0xFF,
                    green: 0xFF,
                    blue: 0xFF,
                });
                f.set_value(f.default());
            }),
        )?;
        params.add_group(
            Params::ColorTopic,
            Params::ColorTopicEnd,
            "ExtraColors",
            true,
            |params: &mut ae::Parameters<Params>| -> Result<(), Error> {
                // 元: PF_ADD_SLIDER(UI_COLOR_MAX, 1, 4, 1, 4, 20, ..)
                // 既定値 20 は上限 4 を超えている (元のまま。上の説明を参照)。
                params.add(
                    Params::ColorMax,
                    "ColorMax",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(COLOR_COUNT as i32);
                        f.set_slider_min(1);
                        f.set_slider_max(COLOR_COUNT as i32);
                        f.set_default(20);
                        f.set_value(f.default());
                    }),
                )?;
                for (p, name, rgb) in EXTRA_COLORS {
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
                Ok(())
            },
        )?;
        params.add(
            Params::Org,
            "Blend with original",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
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
                out_data.set_return_msg("fs-rs sputteringalpha");
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
        let seed = params.get(Params::Seed)?.as_slider()?.value();
        // 元: GetFIXED_PER2ONE なので固定小数を 100 で割る (整数の割り算)。
        let per2one = |v: f64| -> i64 { libs::utils::flt2fix(v) as i64 / 100 };
        let value_p = per2one(params.get(Params::Value)?.as_float_slider()?.value());
        let edge_p = per2one(params.get(Params::EdgeValue)?.as_float_slider()?.value());
        let opa =
            libs::utils::flt2fix(params.get(Params::OpacityRand)?.as_float_slider()?.value()) as i64;
        let p_value = params.get(Params::PointValue)?.as_slider()?.value() as i64;
        // 元: downScale (0 はそのまま、他は +0.5 で丸め) してから 2 で下止め。
        let p_len = {
            let v = params.get(Params::PointLength)?.as_slider()?.value() as i64;
            let ds = in_data.downsample_x();
            let v = if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            };
            v.max(2)
        };
        let size = params.get(Params::Size)?.as_popup()?.value();
        // 元は既定値 20 のまま colors[] を読んで範囲外に出る。1..4 に収める。
        let color_max = params
            .get(Params::ColorMax)?
            .as_slider()?
            .value()
            .clamp(1, COLOR_COUNT as i32) as i64;

        let blank = ae::Pixel8 {
            alpha: 0,
            red: 0,
            green: 0,
            blue: 0,
        };
        let mut colors = [blank; COLOR_COUNT];
        for (i, p) in [
            Params::Color1,
            Params::Color2,
            Params::Color3,
            Params::Color4,
        ]
        .iter()
        .enumerate()
        {
            colors[i] = params.get(*p)?.as_color()?.value();
        }
        let org = params.get(Params::Org)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: 元画像に重ねるなら copy、そうでなければ消す。
        if org {
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
        } else {
            for y in 0..h {
                for x in 0..w {
                    match depth {
                        8 => {
                            *out_layer.as_pixel8_mut(x, y) = ae::Pixel8 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        }
                        16 => {
                            *out_layer.as_pixel16_mut(x, y) = ae::Pixel16 {
                                alpha: 0,
                                red: 0,
                                green: 0,
                                blue: 0,
                            }
                        }
                        _ => {
                            *out_layer.as_pixel32_mut(x, y) = ae::PixelF32 {
                                alpha: 0.0,
                                red: 0.0,
                                green: 0.0,
                                blue: 0.0,
                            }
                        }
                    }
                }
            }
        }

        // 元: 量・縁の量・不透明度の幅のどれかが 0 以下なら何も描かない。
        if !(value_p > 0 && edge_p > 0 && opa > 0) {
            return Ok(());
        }

        // 元: MainRender の頭で、中身は 100、縁は 10 でもう一度割る (整数)。
        let value = value_p / 100;
        let e_value = edge_p / 10;

        // 元の EDGE_BORDER8 / 16 / 32。どれも整数の割り算。
        let border = match depth {
            8 => (255 * 8 / 10) as f64,
            16 => (32768 * 8 / 10) as f64,
            _ => 0.8,
        };
        let alpha_at = |x: usize, y: usize| -> f64 {
            match depth {
                8 => in_layer.as_pixel8(x, y).alpha as f64,
                16 => in_layer.as_pixel16(x, y).alpha as f64,
                _ => in_layer.as_pixel32(x, y).alpha as f64,
            }
        };

        // 元の1回目の走査: 透明 / 縁 / 中身 に分ける。
        let (iw, ih) = (w as i32, h as i32);
        let mut flags = vec![TARGET_NONE; w * h];
        for y in 0..ih {
            for x in 0..iw {
                if alpha_at(x as usize, y as usize) < border {
                    continue;
                }
                let mut eg = TARGET_SOLID;
                // 元: いちばん外側の1画素は縁とみなさない (中身のまま)。
                if x >= 1 && x < iw - 1 && y >= 1 && y < ih - 1 {
                    for (dx, dy) in [
                        (-1, -1),
                        (0, -1),
                        (1, -1),
                        (-1, 0),
                        (1, 0),
                        (-1, 1),
                        (0, 1),
                        (1, 1),
                    ] {
                        if alpha_at((x + dx) as usize, (y + dy) as usize) < border {
                            eg = TARGET_EDGE;
                            break;
                        }
                    }
                }
                flags[(y * iw + x) as usize] = eg;
            }
        }

        let table = setup_sput_data(size);
        let mut rng = XorShift::new(seed as u32);

        // 元の2回目の走査。
        for y in 0..ih {
            for x in 0..iw {
                let flg = flags[(y * iw + x) as usize];
                // 元: 透明な画素でも必ず1回引く。ここを飛ばすと乱数列がずれる。
                let rnd = rng.fm_rand(1 << 16);
                let (draw, scale) = match flg {
                    TARGET_SOLID => (value >= rnd, 5),
                    TARGET_EDGE => (e_value >= rnd, 3),
                    _ => (false, 1),
                };
                if !draw {
                    continue;
                }
                for _ in 0..p_value {
                    let (ox, oy) = pick_offset(&mut rng, p_len, scale);
                    // 元: sdp.x = x + rc.pos().x。固定小数を画素にそのまま足す。
                    let sx = x + ox;
                    let sy = y + oy;
                    // 元: (PF_FpShort)FM_RAND(opa)/65536 なので f32 の割り算。
                    let op = rng.fm_rand(opa) as f32 / 65536.0;
                    let idx = table[rng.fm_rand(SPD_RAND_MAX as i64 - 1) as usize] as i64;
                    let col_r = rng.fm_rand(color_max - 1);
                    let col = colors[(col_r.clamp(0, COLOR_COUNT as i64 - 1)) as usize];
                    sput_draw(&out_layer, depth, idx, sx, sy, op, col);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_flag_values_match_the_original() {
        // 元: 0 / PF_HALF_CHAN8 / PF_MAX_CHAN8。
        assert_eq!(TARGET_NONE, 0);
        assert_eq!(TARGET_SOLID, 128);
        assert_eq!(TARGET_EDGE, 255);
    }

    #[test]
    fn the_percent_goes_through_two_integer_divisions() {
        // 中身は /100 のあと /100、縁は /100 のあと /10。
        let p = libs::utils::flt2fix(10.0) as i64 / 100;
        assert_eq!(p / 100, 65);
        let e = libs::utils::flt2fix(15.0) as i64 / 100;
        assert_eq!(e / 10, 983);
    }

    #[test]
    fn offsets_stay_within_the_scaled_length() {
        let mut rng = XorShift::new(11);
        for _ in 0..200 {
            let (x, y) = pick_offset(&mut rng, 20, 5);
            assert!(x.abs() <= 100 && y.abs() <= 100, "{x} {y}");
        }
    }

    #[test]
    fn the_border_is_eight_tenths() {
        // 元の EDGE_BORDER8 / 16 は整数の割り算。
        assert_eq!(255 * 8 / 10, 204);
        assert_eq!(32768 * 8 / 10, 26214);
    }
}
