//! F's sputteringCircle の移植。
//!
//! 1点のまわりの円 (アスペクトで楕円にもなる) の中に乱数で位置を選び、
//! そこからさらにかたまりを散らす。かたまりの絵柄は飛び散り系で共通の
//! `libs::sputtering` の表から引く。
//!
//! 元から写した細かいところ:
//!
//! - 円の中の位置は「角度は乱数、半径は 0..Radius の乱数」なので、
//!   中心のほうが濃くなる (面積で均した分布ではない)
//! - アスペクトは Y 側だけにかける (`pos.y * aspect >> 16`)
//! - アンカーが ON なら、アンカーからの差に Length_Scale をかける。
//!   OFF なら散らしぶんだけに Length_Scale をかける
//! - Point_length_sysnc が ON なら、散る距離にも Length_Scale をかける
//! - ふだんは距離そのまま、1/6 の確率で2倍まで伸びる
//!   (sputteringRect は3倍、sputteringAlpha は5倍/3倍)
//! - 画面の外かどうかの判定は、かたまりを散らす前の点で、しかも
//!   Length_Scale をかける前の `Radius + PointLength` を余裕にして見る
//! - 量と不透明度の幅のどちらかが 0 なら何も描かない
//! - ずらし量は `SetRotLength` に渡した固定小数をそのまま画素座標に足す
//!   (飛び散り系で共通の、二重の目盛り違いが打ち消し合う形)
//!
//! 元と違えたところ:
//!
//! - **Color4 の既定色を直した**。元のソースは緑のところに
//!   `-PF_MAX_CHAN8` と書いてしまっていて (マイナスが1つ余計)、
//!   `A_u_char` に収まるときに 1 になる。出荷 .aex の既定も
//!   `[255, 0, 1, 255]`。他の3色が白・黄・紫なので水色のつもりだったとみて
//!   `(0, 255, 255)` にした
//! - **ColorMax の既定値が 20 なのに色は4つしかない** (飛び散り系で共通)。
//!   読み込むときに 1..4 に収めてある
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/sputteringCircle

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
use libs::sputtering::{setup_sput_data, sput_draw, SPD_RAND_MAX};
use libs::utils::rot_length;

/// 元の UI_COLOR_MAX_V。
const COLOR_COUNT: usize = 4;

/// 元の「ばらつきを作る」ところ。1/6 の確率で2倍まで。
fn pick_offset(rng: &mut XorShift, p_len: i64) -> (i32, i32) {
    let len_max = if rng.fm_rand(5) == 0 { p_len * 2 } else { p_len };
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
    OpacityRand,
    Pos,
    Radius,
    Aspect,
    LengthScale,
    AnchorEnabled,
    Anchor,
    PointValue,
    PointLength,
    PointLenSync,
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

/// 元の PF_ADD_COLOR の既定色 (Color2..4)。
const EXTRA_COLORS: [(Params, &str, [u8; 3]); 3] = [
    (Params::Color2, "Color2", [0xFF, 0xFF, 0x00]),
    (Params::Color3, "Color3", [0xFF, 0x00, 0xFF]),
    // 元のソースは緑のところに `-PF_MAX_CHAN8` と書いてしまっていて
    // (マイナスが1つ余計)、A_u_char に収まるときに 1 になる。出荷 .aex も
    // [255, 0, 1, 255]。他の3色が白・黄・紫なので水色のつもりだったとみて
    // 直してある。
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
        // 元: PF_ADD_SLIDER(UI_VALUE, 0, 32000, 0, 100, 20, ..)
        params.add(
            Params::Value,
            "value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(32000);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(20);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED(UI_OPA_RND, 0, 5, 0, 2, 1, 2, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
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
        // 元: PF_ADD_POINT(UI_POS, 50, 50, FALSE, ..)
        params.add(
            Params::Pos,
            "Position",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(UI_RADIUS, 1, 32000, 1, 800, 100, ..)
        params.add(
            Params::Radius,
            "Radius",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(32000);
                f.set_slider_min(1);
                f.set_slider_max(800);
                f.set_default(100);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED(UI_ASPECT, 0, 100, 0, 2, 1, 2, 0, 0, ..)
        params.add(
            Params::Aspect,
            "Aspect",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(2.0);
                f.set_precision(2);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED(UI_LENGTH_SCALE, 0, 10000, 0, 400, 100, 1, 0, 0, ..)
        params.add(
            Params::LengthScale,
            "Length_Scale",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(10000.0);
                f.set_slider_min(0.0);
                f.set_slider_max(400.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
                f.set_value(f.default());
            }),
        )?;
        params.add(
            Params::AnchorEnabled,
            "Anchor_Enabled",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POINT(UI_ANCHOR, 50, 60, FALSE, ..)
        params.add(
            Params::Anchor,
            "Anchor",
            ae::PointDef::setup(|f| {
                f.set_default((50.0, 60.0));
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
        // 元: PF_ADD_SLIDER(UI_POINT_LENGTH, 5, 1000, 5, 200, 100, ..)
        params.add(
            Params::PointLength,
            "PointLength",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(5);
                f.set_valid_max(1000);
                f.set_slider_min(5);
                f.set_slider_max(200);
                f.set_default(100);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX(UI_POINT_LEN_SYSNC1, .., TRUE, ..) ―― 既定で ON。
        params.add(
            Params::PointLenSync,
            "Point_length_sysnc",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
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
                out_data.set_return_msg("fs-rs sputteringcircle");
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
        let value = params.get(Params::Value)?.as_slider()?.value() as i64;
        let opa =
            libs::utils::flt2fix(params.get(Params::OpacityRand)?.as_float_slider()?.value()) as i64;

        // 元: GetFIXEDPOINT してから >>16 で画素にする。
        let pos = params.get(Params::Pos)?.as_point()?.value();
        let (pos_x, pos_y) = (pos.0.floor() as i64, pos.1.floor() as i64);
        let anchor = params.get(Params::Anchor)?.as_point()?.value();
        let (anc_x, anc_y) = (anchor.0.floor() as i64, anchor.1.floor() as i64);

        let ds = in_data.downsample_x();
        // 元の downScale: 0 はそのまま、他は +0.5 で丸め。
        let down = |v: i64| -> i64 {
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        let radius = down(params.get(Params::Radius)?.as_slider()?.value() as i64);
        let aspect =
            libs::utils::flt2fix(params.get(Params::Aspect)?.as_float_slider()?.value()) as i64;
        // 元: GetFIXED_PER2ONE なので固定小数を 100 で割る (整数の割り算)。
        let scale =
            libs::utils::flt2fix(params.get(Params::LengthScale)?.as_float_slider()?.value()) as i64
                / 100;
        let anchor_enabled = params.get(Params::AnchorEnabled)?.as_checkbox()?.value();
        let p_value = params.get(Params::PointValue)?.as_slider()?.value() as i64;
        // 元: こちらは 2 で下止めしない (Rect / Splash とは違う)。
        let point_length = down(params.get(Params::PointLength)?.as_slider()?.value() as i64);
        let len_sync = params.get(Params::PointLenSync)?.as_checkbox()?.value();
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

        // 元: 量と不透明度の幅のどちらかが 0 以下なら何も描かない。
        if !(value > 0 && opa > 0) {
            return Ok(());
        }

        let table = setup_sput_data(size);
        let mut rng = XorShift::new(seed as u32);

        // 元: 同期が ON なら散る距離にも Length_Scale をかける。
        let p_len = if len_sync {
            (point_length * scale) >> 16
        } else {
            point_length
        };
        // 元: 余裕は Length_Scale をかける前の Radius + PointLength で見る。
        let margin = radius + point_length;
        let (iw, ih) = (w as i64, h as i64);
        let xy0 = -margin;
        let xx1 = iw + margin;
        let yy1 = ih + margin;

        for _ in 0..value {
            // 元: 円の中の位置。半径は 0..Radius の乱数なので中心が濃くなる。
            let (sx0, sy0) = {
                let len = rng.fm_rand(radius) as i32;
                let angle = (rng.fm_rand((360 << 4) - 1) << 12) as i32;
                rot_length(angle, len)
            };
            let shift_x = sx0 as i64;
            // 元: アスペクトは Y 側だけ。
            let shift_y = (sy0 as i64 * aspect) >> 16;

            let (x, y) = if anchor_enabled {
                (
                    anc_x + (((pos_x + shift_x - anc_x) * scale) >> 16),
                    anc_y + (((pos_y + shift_y - anc_y) * scale) >> 16),
                )
            } else {
                (
                    pos_x + ((shift_x * scale) >> 16),
                    pos_y + ((shift_y * scale) >> 16),
                )
            };

            for _ in 0..p_value {
                let (ox, oy) = pick_offset(&mut rng, p_len);
                // 元: sdp.x = x + rp.pos().x。固定小数を画素にそのまま足す。
                let px = (x + ox as i64) as i32;
                let py = (y + oy as i64) as i32;
                // 元: (PF_FpShort)FM_RAND(opa)/65536 なので f32 の割り算。
                let op = rng.fm_rand(opa) as f32 / 65536.0;
                let idx = table[rng.fm_rand(SPD_RAND_MAX as i64 - 1) as usize] as i64;
                let col_r = rng.fm_rand(color_max - 1);
                if !(x >= xy0 && x < xx1 && y >= xy0 && y < yy1) {
                    continue;
                }
                let col = colors[(col_r.clamp(0, COLOR_COUNT as i64 - 1)) as usize];
                sput_draw(&out_layer, depth, idx, px, py, op, col);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_stay_within_twice_the_length() {
        let mut rng = XorShift::new(5);
        for _ in 0..200 {
            let (x, y) = pick_offset(&mut rng, 30);
            assert!(x.abs() <= 60 && y.abs() <= 60, "{x} {y}");
        }
    }

    #[test]
    fn the_scale_goes_through_an_integer_division() {
        // 元: GetFIXED_PER2ONE で固定小数を 100 で割る。100% なら 1.0。
        let scale = libs::utils::flt2fix(100.0) as i64 / 100;
        assert_eq!(scale, 1 << 16);
        // 50% なら 0.5。
        assert_eq!(libs::utils::flt2fix(50.0) as i64 / 100, 32768);
    }

    #[test]
    fn the_aspect_only_touches_y() {
        // アスペクト 0.5 なら Y が半分、X はそのまま。
        let aspect = libs::utils::flt2fix(0.5) as i64;
        assert_eq!((100i64 * aspect) >> 16, 50);
    }
}
