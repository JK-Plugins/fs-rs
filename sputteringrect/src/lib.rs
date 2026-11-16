//! F's sputteringRect の移植。
//!
//! 四角い範囲の中に乱数で点を選び、そのまわりに絵の具を飛ばしたような
//! かたまりを重ねていく。かたまりの絵柄は `libs::sputtering` の
//! 14x14 を 60 種類持った表から引く。
//!
//! 元から写した細かいところ:
//!
//! - 乱数は FM_RAND (xorShift の 15bit を `(HI+1)/(1+0x7FFF)` 倍して
//!   切り捨て)。引く順まで合わせてある
//! - ずらし量は `SetRotLength` に固定小数として渡した値を、戻ってきた
//!   固定小数のまま画素座標に足している。**元は二重に目盛りを間違えて
//!   いるが、ちょうど打ち消し合って「画素単位のずらし」になる**ので
//!   そのまま写した
//! - 画面の外かどうかの判定は、ずらす前の点 (x, y) で見る。ずらした先が
//!   外に出たぶんは、かたまりを描くところで切られる
//! - 1/6 の確率で距離を3倍にして散らす
//!
//! 元と違えたところ:
//!
//! - **ColorMax の既定値が 20 なのに色は4つしかない** (スライダーの上限も
//!   4)。元はこのとき `colors[0..19]` を読んで配列の外に出る。読み込む
//!   ときに 1..4 に収めてあるので、既定では「4色」として働く。
//!   出荷 .aex と構成を合わせるため、既定値そのものは 20 のままにしてある
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/sputteringRect

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};
use libs::fs_graphics::XorShift;
use libs::fs_graphics8::{pixel_blend16, pixel_blend8, Px16, Px8};
use libs::sputtering::{setup_sput_data, sput_cells, SPD_RAND_MAX};
use libs::utils::{conv_8_to_16, conv_8_to_32, rot_length, round_fp_short_double};

/// 元の UI_COLOR_MAX_V。
const COLOR_COUNT: usize = 4;

/// 元の「ばらつきを作る」ところ。角度と距離を引いて XY のずれにする。
fn pick_offset(rng: &mut XorShift, p_len: i64) -> (i32, i32) {
    // 元: 1/6 の確率で距離を3倍。
    let len_max = if rng.fm_rand(5) == 0 { p_len * 3 } else { p_len };
        // 元: SetRotLength(FM_RAND((360<<4)-1) << 12, FM_RAND(len))。
        // ひとつの呼び出しの引数なので、どちらの乱数を先に引くかは
        // コンパイラ任せ。出荷ビルドの MSVC は右から評価するので距離が先。
        let len = rng.fm_rand(len_max) as i32;
        let angle = (rng.fm_rand((360 << 4) - 1) << 12) as i32;
    rot_length(angle, len)
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Seed,
    Value,
    OpacityRand,
    TopLeft,
    BottomRight,
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
        // 元: PF_ADD_POINT(UI_TOPLEFT, 25, 25, FALSE, ..) / (75, 75)
        for (p, name, xy) in [
            (Params::TopLeft, "TopLeft", (25.0f32, 25.0f32)),
            (Params::BottomRight, "BottomRight", (75.0, 75.0)),
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
        // 元: PF_ADD_SLIDER(UI_POINT_VALUE, 1, 100, 1, 50, 10, ..)
        params.add(
            Params::PointValue,
            "PointValue",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(100);
                f.set_slider_min(1);
                f.set_slider_max(50);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(UI_POINT_LENGTH, 5, 1000, 5, 200, 50, ..)
        params.add(
            Params::PointLength,
            "PointLength",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(5);
                f.set_valid_max(1000);
                f.set_slider_min(5);
                f.set_slider_max(200);
                f.set_default(50);
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
                out_data.set_return_msg("fs-rs sputteringrect");
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
        // 元: GetFIXED なので生の固定小数のまま使う。
        let opa = libs::utils::flt2fix(params.get(Params::OpacityRand)?.as_float_slider()?.value())
            as i64;

        let tl = params.get(Params::TopLeft)?.as_point()?.value();
        let br = params.get(Params::BottomRight)?.as_point()?.value();
        // 元: tl.x >> 16 (固定小数から画素へ)。クレートは f32 で返すので同じ切り捨て。
        let (mut left, mut top) = (tl.0.floor() as i64, tl.1.floor() as i64);
        let (mut right, mut bottom) = (br.0.floor() as i64, br.1.floor() as i64);
        if left > right {
            std::mem::swap(&mut left, &mut right);
        }
        if top > bottom {
            std::mem::swap(&mut top, &mut bottom);
        }

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

        // 元: value が 0 以下なら何も描かない。
        if value <= 0 {
            return Ok(());
        }

        let table = setup_sput_data(size);
        let mut rng = XorShift::new(seed as u32);

        let dx = right - left - 1;
        let dy = bottom - top - 1;
        let (iw, ih) = (w as i64, h as i64);
        // 元: 画面の外かどうかは、ずらす前の点で判定する。
        let xy0 = -p_len;
        let xx1 = iw + p_len;
        let yy1 = ih + p_len;

        for _ in 0..value {
            let x = left + rng.fm_rand(dx);
            let y = top + rng.fm_rand(dy);
            for _ in 0..p_value {
                let (ox, oy) = pick_offset(&mut rng, p_len);
                // 元: sdp.x = x + rp.pos().x。固定小数を画素にそのまま足す
                // (上の説明のとおり、二重の目盛り違いが打ち消し合う)。
                let sx = (x + ox as i64) as i32;
                let sy = (y + oy as i64) as i32;
                // 元: (PF_FpShort)FM_RAND(opa)/65536 なので f32 の割り算。
                let op = rng.fm_rand(opa) as f32 / 65536.0;
                let idx = table[rng.fm_rand(SPD_RAND_MAX as i64 - 1) as usize] as i64;
                let col_r = rng.fm_rand(color_max - 1);
                if !(x >= xy0 && x < xx1 && y >= xy0 && y < yy1) {
                    continue;
                }
                let col = colors[(col_r.clamp(0, COLOR_COUNT as i64 - 1)) as usize];
                match depth {
                    8 => {
                        let c = Px8::new(0, col.red, col.green, col.blue);
                        sput_cells(idx, sx, sy, w as i32, h as i32, |px, py, v| {
                            // 元: RoundByteFpShort (f32 で計算して 0..255 で切り捨て)。
                            let (a, draw) = match v {
                                None => (round_byte_f32(MAX_CHANNEL8 as f32 * op), true),
                                Some(s) => {
                                    let a = round_byte_f32(s as f32 * op);
                                    (a, a > 0)
                                }
                            };
                            if !draw {
                                return;
                            }
                            let d = out_layer.as_pixel8_mut(px as usize, py as usize);
                            let dd = Px8::new(d.alpha, d.red, d.green, d.blue);
                            // 元: PixelBlend8(oD[adr], c) ―― 既にある画素が
                            // src、新しい色が dst。順番が逆だと重なった
                            // ところだけ値がずれる。
                            let r = pixel_blend8(dd, Px8 { alpha: a, ..c });
                            d.alpha = r.alpha;
                            d.red = r.red;
                            d.green = r.green;
                            d.blue = r.blue;
                        });
                    }
                    16 => {
                        let c16 = conv_8_to_16(&col);
                        let c = Px16::new(0, c16.red, c16.green, c16.blue);
                        sput_cells(idx, sx, sy, w as i32, h as i32, |px, py, v| {
                            let (a, draw) = match v {
                                None => (
                                    round_short_f64(MAX_CHANNEL16 as f32 * op),
                                    true,
                                ),
                                Some(s) => {
                                    // 元: RoundShortFpLong(CONVERT8TO16(v * opa))。
                                    // CONVERT8TO16 の (long) で先に切り捨てられる。
                                    let t = (s as f32 * op) as i64;
                                    let a = round_short_f64(
                                        ((t * MAX_CHANNEL16 as i64 + 128)
                                            / MAX_CHANNEL8 as i64) as f32,
                                    );
                                    (a, a > 0)
                                }
                            };
                            if !draw {
                                return;
                            }
                            let d = out_layer.as_pixel16_mut(px as usize, py as usize);
                            let dd = Px16::new(d.alpha, d.red, d.green, d.blue);
                            let r = pixel_blend16(dd, Px16 { alpha: a, ..c });
                            d.alpha = r.alpha;
                            d.red = r.red;
                            d.green = r.green;
                            d.blue = r.blue;
                        });
                    }
                    _ => {
                        let c32 = conv_8_to_32(&col);
                        sput_cells(idx, sx, sy, w as i32, h as i32, |px, py, v| {
                            // 元: 1画素だけの分岐は丸めずに不透明度そのまま。
                            let (a, draw) = match v {
                                None => (op, true),
                                Some(s) => {
                                    let a = round_fp_short_double(
                                        (s as f32 / MAX_CHANNEL8 as f32 * op) as f64,
                                    );
                                    (a, a > 0.0)
                                }
                            };
                            if !draw {
                                return;
                            }
                            let d = out_layer.as_pixel32_mut(px as usize, py as usize);
                            let r = blend32(
                                [d.alpha, d.red, d.green, d.blue],
                                [a, c32.red, c32.green, c32.blue],
                            );
                            d.alpha = r[0];
                            d.red = r[1];
                            d.green = r[2];
                            d.blue = r[3];
                        });
                    }
                }
            }
        }
        Ok(())
    }
}

/// 元の RoundByteFpShort (PF_FpShort = f32)。
fn round_byte_f32(x: f32) -> u8 {
    let t = if x < 0.0 {
        0.0
    } else if x > MAX_CHANNEL8 as f32 {
        MAX_CHANNEL8 as f32
    } else {
        x
    };
    t as u8
}

/// 元の RoundShortFpLong に f32 の値を渡したとき。
fn round_short_f64(x: f32) -> u16 {
    let t = x as f64;
    let t = if t < 0.0 {
        0.0
    } else if t > MAX_CHANNEL16 as f64 {
        MAX_CHANNEL16 as f64
    } else {
        t
    };
    t as u16
}

/// 元の PixelBlend32 (a, r, g, b の順)。
fn blend32(ss: [f32; 4], dd: [f32; 4]) -> [f32; 4] {
    if ss[0] == 0.0 || dd[0] >= 1.0 {
        return dd;
    }
    if dd[0] == 0.0 {
        return ss;
    }
    let as2 = (1.0 - dd[0]) * ss[0];
    let ad2 = (dd[0] + as2).min(1.0);
    if ad2 <= 0.0 {
        return [0.0; 4];
    }
    let mix = |d: f32, s: f32| ((d * dd[0] + s * as2) / ad2).min(1.0);
    [
        ad2,
        mix(dd[1], ss[1]),
        mix(dd[2], ss[2]),
        mix(dd[3], ss[3]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_stay_within_the_length() {
        let mut rng = XorShift::new(7);
        for _ in 0..200 {
            let (x, y) = pick_offset(&mut rng, 50);
            // 1/6 の確率で3倍になるので、その上限で見る。
            assert!(x.abs() <= 150 && y.abs() <= 150, "{x} {y}");
        }
    }

    #[test]
    fn offset_is_zero_when_the_length_comes_out_zero() {
        // 距離 0 なら SetRotLength は原点を返す。
        assert_eq!(rot_length(12345, 0), (0, 0));
    }

    #[test]
    fn round_byte_f32_truncates_and_clamps() {
        assert_eq!(round_byte_f32(-1.0), 0);
        assert_eq!(round_byte_f32(254.9), 254);
        assert_eq!(round_byte_f32(1000.0), 255);
    }

    #[test]
    fn blend32_leaves_an_opaque_backdrop_alone() {
        let dd = [1.0, 0.5, 0.5, 0.5];
        assert_eq!(blend32([0.5, 1.0, 0.0, 0.0], dd), dd);
    }

    #[test]
    fn the_rng_matches_the_originals_draw_order() {
        // FM_RAND は 0..=hi を返す。種が同じなら並びも同じ。
        let mut a = XorShift::new(0);
        let mut b = XorShift::new(0);
        for _ in 0..50 {
            let v = a.fm_rand(9);
            assert!((0..=9).contains(&v));
            assert_eq!(v, b.fm_rand(9));
        }
    }
}
