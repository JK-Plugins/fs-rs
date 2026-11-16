//! F's colorThreshold の移植。
//!
//! 「アルファで切り出した絵の中を、あらかじめ決めた色のどれかに丸める」
//! セル画向けの効果。アルファを2値化して対象を決め、対象のうち指定色に
//! 近いものをその色で塗り、どの色にも当たらなかったものは**まわりの
//! 決まった画素から色を貰う**のを最大10回くり返して広げる。
//!
//! 元は判定の途中の状態をアルファチャンネルの下位2ビットに埋めて
//! (`CT_TARGET`=1 / `CT_SEARCH`=2 / `CT_DECISION`=3)、2ビット目から上を
//! 色表の番号にしている。移植では色と旗を別の面に分けたが、値の決まり方は
//! そのまま写した。
//!
//! 元から写した細かいところ:
//!
//! - **独自の RGB→HLS を持っている** (`libs::hls` とは別物)。入力は
//!   `FS_CONVERT8TO32`、つまり小数4桁に量子化した 0..1。`cMax` との
//!   一致は量子化後の値で見るので、ここを素直な `/255.0` にすると
//!   枝の選び方が変わる
//! - 色の近さは2種類ある。`colorCompare` は色相と彩度だけの距離 (`/√2`)
//!   を `hs` と比べ、通ったら明度の比 `1 - minL/maxL` を `l` と比べる。
//!   `colorDiatance` のほうは色相・彩度・明度の3つ (`/√3`) で、まわりから
//!   貰う色を選ぶときだけに使う
//! - 判定は3段。(1) 全体の colorMatch_level / colorLightness_level で
//!   メイン3色→サブ20色を順に当てる、(2) まだ残っている対象をメイン色
//!   ごとの `color#_hs` / `color#_level` でもう一度当てる、(3) 残りは
//!   上下左右の決まった画素のうち最も近い色から番号を貰う。(3) は
//!   10回まで
//! - (3) で貰った旗は `CT_SEARCH` なので、同じ周の中では広がらない。
//!   次の `use_searchSub` で色が入って初めて種になる
//! - 画面の外は `getPixel8` が透明の黒を返すので、種にならない
//! - 最後の `AlphaFix` は旗の下位2ビットが 0 以外なら不透明にする。
//!   **どの色にも当たらなかった対象は元の色のまま不透明になる**
//! - アルファのしきい値は `255 * level + 0.5` を切り捨てたもの。
//!   16/32bit は 8bit に直してから比べる
//! - メイン色は ON のものだけを前から詰める。サブ色は RGB が同じものを
//!   捨ててから詰める (`findColor8`)
//! - 色が1つもなければ何もしない
//!
//! 元と違えたところ:
//!
//! - **RGB→HLS の緑の式を直した**。元は緑が最大のとき
//!   `h = (r - g) / C + 2` になっていて、赤の枝が `(g - b)/C`、青の枝が
//!   `(r - g)/C + 4` と教科書どおりなので、緑だけ写し間違えている。
//!   正しくは `(b - r) / C + 2`。元のままだと緑系の色相が青成分を
//!   まったく見ないので、緑と黄の見分けがつかない。`libs::hls` の
//!   `HLSA::from_rgb` は正しい式になっている
//! - **32bit の `use_searchSub` が表の番号を左に戻していないのを直した**。
//!   8/16bit は `(idx << 2) | CT_DECISION` を書き戻すのに、32bit だけ
//!   `idx | CT_DECISION` で、1番目以降のメイン色・サブ色に決まった画素が
//!   番号 0 として次の周に伝わる。深度どうしで食い違っているほうを
//!   直した。**こちらは元と突き合わせて確かめていない** (AEXCompat の
//!   classic 経路は 8bit 専用なので 32bit を振れない)
//! - 色が1つもないとき、元は出力ワールドに一切書かないので中身が不定に
//!   なる。移植では入力をそのままコピーする
//! - パラメータ名は元も ASCII なのでそのまま
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/colorThreshold

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};
use libs::utils::{conv_32_to_8, conv_8_to_16, conv_8_to_32, conv_16_to_8, round_byte_fp_double};

/// 元の COUNT_OF_MAIN。
const COUNT_OF_MAIN: usize = 3;
/// 元の COUNT_OF_SUB。
const COUNT_OF_SUB: usize = 20;
/// 元の RETRY。まわりから色を貰う周の上限。
const RETRY: usize = 10;

const CT_TARGET: i32 = 1;
const CT_SEARCH: i32 = 2;
const CT_DECISION: i32 = 3;
/// 元の CT_FLOAT。32bit は旗をこれで割った実数としてアルファに入れる。
const CT_FLOAT: f32 = 1000.0;

const SQMAX_2: f64 = 1.4142135623730950488016887242097;
const SQMAX_3: f64 = 1.7320508075688772935274463415059;

/// 入力をそのまま通す画素。
const SLOT_INPUT: i32 = -1;
/// 対象外。元は白の透明 (`{0, MAX, MAX, MAX}`) を書く。
const SLOT_WHITE: i32 = -2;

#[derive(Debug, Clone, Copy, Default)]
struct Hls {
    h: f64,
    l: f64,
    s: f64,
}

/// 元の FS_CONVERT8TO32。小数4桁に量子化する。`libs` の
/// `conv_8_to_32_channel` は最後に f32 に落とすが、元の `rgbToHls` は
/// PF_FpLong (double) で受けるのでここでは f64 のままにする。
fn conv_8_to_32_double(c: u8) -> f64 {
    (c as f64 * 10000.0 / MAX_CHANNEL8 as f64 + 0.5) as i64 as f64 / 10000.0
}

/// 元の rgbToHls。h, l, s はいずれも 0..1。
fn rgb_to_hls(p: ae::Pixel8) -> Hls {
    let mut r = conv_8_to_32_double(p.red);
    let mut g = conv_8_to_32_double(p.green);
    let mut b = conv_8_to_32_double(p.blue);
    if r > 1.0 {
        r = 1.0;
    }
    if g > 1.0 {
        g = 1.0;
    }
    if b > 1.0 {
        b = 1.0;
    }

    let c_max = r.max(g).max(b);
    let c_min = r.min(g).min(b);
    let c = c_max - c_min;

    let mut h = 0.0;
    let mut s = 0.0;
    let l = (c_max + c_min) / 2.0;

    if c != 0.0 {
        s = if l <= 0.5 {
            c / (2.0 * l)
        } else {
            c / (2.0 - 2.0 * l)
        };
        if r == c_max {
            h = (g - b) / c;
            if h < 0.0 {
                h += 6.0;
            }
        } else if g == c_max {
            h = (b - r) / c + 2.0;
        } else if b == c_max {
            h = (r - g) / c + 4.0;
        }
        h /= 6.0;
    }
    Hls { h, l, s }
}

/// 元の hueDiff。色相は輪なので近いほうを取る。
fn hue_diff(sh: f64, dh: f64) -> f64 {
    if sh > dh {
        (sh - dh).min(dh - sh + 1.0)
    } else {
        (dh - sh).min(sh - dh + 1.0)
    }
}

/// 元の colorDiatance。色相・彩度・明度の3次元の距離を √3 で割る。
fn color_distance(s: &Hls, c: ae::Pixel8) -> f64 {
    let d = rgb_to_hls(c);
    let hd = hue_diff(s.h, d.h);
    (hd * hd + (s.s - d.s) * (s.s - d.s) + (s.l - d.l) * (s.l - d.l)).sqrt() / SQMAX_3
}

/// 元の colorCompare。色相と彩度の距離を `hs` と比べ、通ったら明度の比を
/// `l` と比べる。明度がぴったり同じなら比を見ずに通す。
fn color_compare(s: &Hls, c: ae::Pixel8, hs: f64, l: f64) -> bool {
    let d = rgb_to_hls(c);
    let hd = hue_diff(s.h, d.h);
    let hs_d = (hd * hd + (s.s - d.s) * (s.s - d.s)).sqrt() / SQMAX_2;
    if hs_d > hs {
        return false;
    }
    if s.l == d.l {
        return true;
    }
    let max_l = s.l.max(d.l);
    let min_l = s.l.min(d.l);
    (1.0 - min_l / max_l) <= l
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    AlphaLevel,
    HsLevel,
    LLevel,
    MainTopic,
    Main1On,
    Main1Col,
    Main1Hs,
    Main1Lv,
    Main2On,
    Main2Col,
    Main2Hs,
    Main2Lv,
    Main3On,
    Main3Col,
    Main3Hs,
    Main3Lv,
    MainTopicEnd,
    SubTopic,
    SubCount,
    Sub1,
    Sub2,
    Sub3,
    Sub4,
    Sub5,
    Sub6,
    Sub7,
    Sub8,
    Sub9,
    Sub10,
    Sub11,
    Sub12,
    Sub13,
    Sub14,
    Sub15,
    Sub16,
    Sub17,
    Sub18,
    Sub19,
    Sub20,
    SubTopicEnd,
}

const SUB_PARAMS: [Params; COUNT_OF_SUB] = [
    Params::Sub1,
    Params::Sub2,
    Params::Sub3,
    Params::Sub4,
    Params::Sub5,
    Params::Sub6,
    Params::Sub7,
    Params::Sub8,
    Params::Sub9,
    Params::Sub10,
    Params::Sub11,
    Params::Sub12,
    Params::Sub13,
    Params::Sub14,
    Params::Sub15,
    Params::Sub16,
    Params::Sub17,
    Params::Sub18,
    Params::Sub19,
    Params::Sub20,
];

/// 元の (ON, 色, 色相+彩度, 明度) の4つ組と、その既定値。
const MAIN_PARAMS: [(Params, Params, Params, Params); COUNT_OF_MAIN] = [
    (
        Params::Main1On,
        Params::Main1Col,
        Params::Main1Hs,
        Params::Main1Lv,
    ),
    (
        Params::Main2On,
        Params::Main2Col,
        Params::Main2Hs,
        Params::Main2Lv,
    ),
    (
        Params::Main3On,
        Params::Main3Col,
        Params::Main3Hs,
        Params::Main3Lv,
    ),
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
        // 元: PF_ADD_FLOAT_SLIDER(alpha_Level, 0, 100, 25, 75, 1, 50, 1, 0, 0, ..)
        // CURVE_TOLERANCE は SDK のマクロが引数を捨てて 0.05 を入れる。
        params.add(
            Params::AlphaLevel,
            "alpha_Level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(25.0);
                f.set_slider_max(75.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER(colorMatch_level, 0, 10, 0, 1, 0, 0, 1, 0, 0, ..)
        params.add(
            Params::HsLevel,
            "colorMatch_level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(10.0);
                f.set_slider_min(0.0);
                f.set_slider_max(1.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FLOAT_SLIDER(colorLightness_level, 0, 100, 25, 75, 1, 50, ..)
        params.add(
            Params::LLevel,
            "colorLightness_level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(25.0);
                f.set_slider_max(75.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(50.0);
                f.set_value(f.default());
            }),
        )?;
        params.add_group(
            Params::MainTopic,
            Params::MainTopicEnd,
            "LineColors",
            true,
            |params: &mut ae::Parameters<Params>| -> Result<(), Error> {
                // 元: 1色目だけ ON が既定。色は (10,10,10) / (254,254,254) /
                // (0,0,0)、色相の幅は 50 / 1 / 1、明度の幅はすべて 50。
                // スライダーの上限だけは色ごとに違う (50 / 10 / 10 と
                // 50 / 100 / 50)。
                const DEFAULTS: [(bool, [u8; 3], f32, f64, f32, f64); COUNT_OF_MAIN] = [
                    (true, [10, 10, 10], 50.0, 50.0, 50.0, 50.0),
                    (false, [254, 254, 254], 10.0, 1.0, 100.0, 50.0),
                    (false, [0, 0, 0], 10.0, 1.0, 50.0, 50.0),
                ];
                for (i, (on, col, hs, lv)) in MAIN_PARAMS.iter().enumerate() {
                    let (on_dflt, rgb, hs_max, hs_dflt, lv_max, lv_dflt) = DEFAULTS[i];
                    params.add(
                        *on,
                        ["Enabled_1", "Enabled_2", "Enabled_3"][i],
                        ae::CheckBoxDef::setup(|f| {
                            f.set_label("on");
                            f.set_default(on_dflt);
                            f.set_value(f.default());
                        }),
                    )?;
                    params.add(
                        *col,
                        ["color1", "color2", "color3"][i],
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
                    params.add(
                        *hs,
                        ["color1_hs", "color2_hs", "color3_hs"][i],
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(hs_max);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(hs_dflt);
                            f.set_value(f.default());
                        }),
                    )?;
                    params.add(
                        *lv,
                        ["color1_level", "color2_level", "color3_level"][i],
                        ae::FloatSliderDef::setup(|f| {
                            f.set_valid_min(0.0);
                            f.set_valid_max(100.0);
                            f.set_slider_min(0.0);
                            f.set_slider_max(lv_max);
                            f.set_precision(1);
                            f.set_curve_tolerance(0.05);
                            f.set_default(lv_dflt);
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
        )?;
        params.add_group(
            Params::SubTopic,
            Params::SubTopicEnd,
            "UseColor",
            true,
            |params: &mut ae::Parameters<Params>| -> Result<(), Error> {
                // 元: PF_ADD_SLIDER(UseColor_count, 0, 20, 0, 20, 0, ..)
                params.add(
                    Params::SubCount,
                    "UseColor_count",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(COUNT_OF_SUB as i32);
                        f.set_slider_min(0);
                        f.set_slider_max(COUNT_OF_SUB as i32);
                        f.set_default(0);
                        f.set_value(f.default());
                    }),
                )?;
                for (i, p) in SUB_PARAMS.iter().enumerate() {
                    // 元: sprintf_s(num, "UserColor_%d", i+1)、既定は黒。
                    let name = format!("UserColor_{}", i + 1);
                    params.add(
                        *p,
                        &name,
                        ae::ColorDef::setup(|f| {
                            f.set_default(ae::Pixel8 {
                                alpha: 0xFF,
                                red: 0,
                                green: 0,
                                blue: 0,
                            });
                            f.set_value(f.default());
                        }),
                    )?;
                }
                Ok(())
            },
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
                out_data.set_return_msg("fs-rs colorthreshold");
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
        // --- 元の GetParams ---
        let alpha_level = params.get(Params::AlphaLevel)?.as_float_slider()?.value() / 100.0;
        // 元: RoundByteFpLong(255 * level + 0.5)。切り捨て。
        let alpha_border = round_byte_fp_double(MAX_CHANNEL8 as f64 * alpha_level + 0.5);
        let hs_level = params.get(Params::HsLevel)?.as_float_slider()?.value() / 100.0;
        let l_level = params.get(Params::LLevel)?.as_float_slider()?.value() / 100.0;

        // 元: ON のメイン色だけを前から詰める。色相・明度の幅も一緒に動く。
        let mut main_col: Vec<ae::Pixel8> = Vec::new();
        let mut main_hs: Vec<f64> = Vec::new();
        let mut main_l: Vec<f64> = Vec::new();
        for (on, col, hs, lv) in MAIN_PARAMS.iter() {
            if params.get(*on)?.as_checkbox()?.value() {
                main_col.push(params.get(*col)?.as_color()?.value());
                main_hs.push(params.get(*hs)?.as_float_slider()?.value() / 100.0);
                main_l.push(params.get(*lv)?.as_float_slider()?.value() / 100.0);
            }
        }

        // 元: サブ色は RGB が同じものを捨てて詰める (findColor8)。
        let sub_want = (params.get(Params::SubCount)?.as_slider()?.value() as usize)
            .min(COUNT_OF_SUB);
        let mut sub_col: Vec<ae::Pixel8> = Vec::new();
        for p in SUB_PARAMS.iter().take(sub_want) {
            let c = params.get(*p)?.as_color()?.value();
            if !sub_col
                .iter()
                .any(|t| t.red == c.red && t.green == c.green && t.blue == c.blue)
            {
                sub_col.push(c);
            }
        }

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元: 色が1つもなければ Exec は何もしない (出力は不定)。
        // 移植では入力をそのままコピーする (上の説明を参照)。
        if main_col.is_empty() && sub_col.is_empty() {
            let mut out_layer = out_layer;
            ae::pf::suites::WorldTransform::new()?.copy_hq(
                in_data.effect_ref(),
                &in_layer,
                &mut out_layer,
                None,
                None,
            )?;
            return Ok(());
        }

        // 元は 8bit に落としてから色を見る (16/32bit も CONV*TO8 を通す)。
        let n = w * h;
        let mut cmp: Vec<ae::Pixel8> = Vec::with_capacity(n);
        for y in 0..h {
            for x in 0..w {
                cmp.push(match depth {
                    8 => *in_layer.as_pixel8(x, y),
                    16 => conv_16_to_8(in_layer.as_pixel16(x, y)),
                    _ => conv_32_to_8(in_layer.as_pixel32(x, y)),
                });
            }
        }

        // 32bit は旗を 1000 で割った f32 でアルファに入れ、読むときに
        // 1000 を掛けて切り捨てる。その往復をここで通しておく。
        let store = |v: i32| -> i32 {
            if depth == 32 {
                ((v as f32 / CT_FLOAT) * CT_FLOAT) as i32
            } else {
                v
            }
        };

        let mut flag = vec![0i32; n];
        let mut slot = vec![SLOT_INPUT; n];

        // --- AlphaThreshold ---
        for i in 0..n {
            if cmp[i].alpha >= alpha_border {
                flag[i] = store(CT_TARGET);
            } else {
                // 元: {0, MAX, MAX, MAX} を書く。
                slot[i] = SLOT_WHITE;
                cmp[i] = ae::Pixel8 {
                    alpha: 0,
                    red: MAX_CHANNEL8 as u8,
                    green: MAX_CHANNEL8 as u8,
                    blue: MAX_CHANNEL8 as u8,
                };
                flag[i] = 0;
            }
        }

        // 色表を1本に並べる。番号はメイン色が 0.., サブ色が 3.. で、
        // 元の `(i<<2)` / `((i+COUNT_OF_MAIN)<<2)` と同じ。
        let table_index = |kind_main: bool, j: usize| -> i32 {
            if kind_main {
                j as i32
            } else {
                (j + COUNT_OF_MAIN) as i32
            }
        };
        let color_of = |idx: i32| -> ae::Pixel8 {
            if (idx as usize) < COUNT_OF_MAIN {
                main_col[idx as usize]
            } else {
                sub_col[idx as usize - COUNT_OF_MAIN]
            }
        };

        // --- hslMatch: 全体の幅でメイン色→サブ色を当てる ---
        for i in 0..n {
            if flag[i] & CT_DECISION != CT_TARGET {
                continue;
            }
            let o = rgb_to_hls(cmp[i]);
            let mut hit = None;
            for (j, c) in main_col.iter().enumerate() {
                if color_compare(&o, *c, hs_level, l_level) {
                    hit = Some(table_index(true, j));
                    break;
                }
            }
            if hit.is_none() {
                for (j, c) in sub_col.iter().enumerate() {
                    if color_compare(&o, *c, hs_level, l_level) {
                        hit = Some(table_index(false, j));
                        break;
                    }
                }
            }
            if let Some(idx) = hit {
                slot[i] = idx;
                cmp[i] = color_of(idx);
                flag[i] = store((idx << 2) | CT_DECISION);
            }
        }

        // --- line_search: 残った対象をメイン色ごとの幅でもう一度当てる ---
        if !main_col.is_empty() {
            for i in 0..n {
                if flag[i] & CT_DECISION != CT_TARGET {
                    continue;
                }
                let o = rgb_to_hls(cmp[i]);
                for (j, c) in main_col.iter().enumerate() {
                    if color_compare(&o, *c, main_hs[j], main_l[j]) {
                        let idx = table_index(true, j);
                        slot[i] = idx;
                        cmp[i] = color_of(idx);
                        flag[i] = store((idx << 2) | CT_DECISION);
                        break;
                    }
                }
            }
        }

        // --- use_search / use_searchSub: まわりの決まった画素から貰う ---
        for _ in 0..RETRY {
            let mut fin = true;
            // use_search: 上下左右のうち、色が最も近い決まった画素の番号を
            // 旗に写して CT_SEARCH にする。
            for y in 0..h {
                for x in 0..w {
                    let i = y * w + x;
                    if flag[i] & CT_DECISION != CT_TARGET {
                        continue;
                    }
                    let o = rgb_to_hls(cmp[i]);
                    let mut found = None;
                    let mut dis = 100.0f64;
                    // 元の並び: 上, 左, 右, 下。範囲外は透明の黒なので種に
                    // ならない。
                    for (nx, ny) in [
                        (x as i64, y as i64 - 1),
                        (x as i64 - 1, y as i64),
                        (x as i64 + 1, y as i64),
                        (x as i64, y as i64 + 1),
                    ] {
                        if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                            continue;
                        }
                        let k = ny as usize * w + nx as usize;
                        if flag[k] & CT_DECISION != CT_DECISION {
                            continue;
                        }
                        let d2 = color_distance(&o, cmp[k]);
                        if dis > d2 {
                            dis = d2;
                            found = Some(flag[k]);
                        }
                    }
                    match found {
                        Some(v) => flag[i] = store((v & 0xFC) | CT_SEARCH),
                        None => fin = false,
                    }
                }
            }
            // use_searchSub: 貰った番号の色を入れて CT_DECISION にする。
            for i in 0..n {
                if flag[i] & CT_DECISION != CT_SEARCH {
                    continue;
                }
                let idx = flag[i] >> 2;
                let ok = if (idx as usize) < COUNT_OF_MAIN {
                    (idx as usize) < main_col.len()
                } else {
                    (idx as usize) < COUNT_OF_MAIN + COUNT_OF_SUB
                        && (idx as usize - COUNT_OF_MAIN) < sub_col.len()
                };
                if ok {
                        let f = (idx << 2) | CT_DECISION;
                    slot[i] = idx;
                    cmp[i] = color_of(idx);
                    flag[i] = store(f);
                }
            }
            if fin {
                break;
            }
        }

        // --- AlphaFix と書き出し ---
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                let opaque = flag[i] & CT_DECISION != 0;
                match depth {
                    8 => {
                        let mut p = match slot[i] {
                            SLOT_INPUT => *in_layer.as_pixel8(x, y),
                            SLOT_WHITE => ae::Pixel8 {
                                alpha: 0,
                                red: MAX_CHANNEL8 as u8,
                                green: MAX_CHANNEL8 as u8,
                                blue: MAX_CHANNEL8 as u8,
                            },
                            idx => color_of(idx),
                        };
                        p.alpha = if opaque { MAX_CHANNEL8 as u8 } else { 0 };
                        *out_layer.as_pixel8_mut(x, y) = p;
                    }
                    16 => {
                        let mut p = match slot[i] {
                            SLOT_INPUT => *in_layer.as_pixel16(x, y),
                            SLOT_WHITE => ae::Pixel16 {
                                alpha: 0,
                                red: MAX_CHANNEL16 as u16,
                                green: MAX_CHANNEL16 as u16,
                                blue: MAX_CHANNEL16 as u16,
                            },
                            idx => conv_8_to_16(&color_of(idx)),
                        };
                        p.alpha = if opaque { MAX_CHANNEL16 as u16 } else { 0 };
                        *out_layer.as_pixel16_mut(x, y) = p;
                    }
                    _ => {
                        let mut p = match slot[i] {
                            SLOT_INPUT => *in_layer.as_pixel32(x, y),
                            SLOT_WHITE => ae::PixelF32 {
                                alpha: 0.0,
                                red: 1.0,
                                green: 1.0,
                                blue: 1.0,
                            },
                            idx => conv_8_to_32(&color_of(idx)),
                        };
                        p.alpha = if opaque { 1.0 } else { 0.0 };
                        *out_layer.as_pixel32_mut(x, y) = p;
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

    fn px(r: u8, g: u8, b: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: 255,
            red: r,
            green: g,
            blue: b,
        }
    }

    #[test]
    fn the_input_is_quantised_to_four_decimals() {
        // 元の FS_CONVERT8TO32。ここを素直な /255 にすると cMax の一致の
        // 取り方が変わる。
        assert_eq!(conv_8_to_32_double(255), 1.0);
        assert_eq!(conv_8_to_32_double(0), 0.0);
        assert_eq!(conv_8_to_32_double(128), 0.502);
        assert_eq!(conv_8_to_32_double(10), 0.0392);
    }

    #[test]
    fn the_primaries_land_on_the_colour_wheel() {
        // 赤 0, 黄 1/6, 緑 2/6, 水 3/6, 青 4/6, 紫 5/6。
        let hue = |r, g, b| (rgb_to_hls(px(r, g, b)).h * 6.0 * 1000.0).round() / 1000.0;
        assert_eq!(hue(255, 0, 0), 0.0);
        assert_eq!(hue(255, 255, 0), 1.0);
        assert_eq!(hue(0, 255, 0), 2.0);
        assert_eq!(hue(0, 255, 255), 3.0);
        assert_eq!(hue(0, 0, 255), 4.0);
        assert_eq!(hue(255, 0, 255), 5.0);
    }

    #[test]
    fn green_and_yellow_are_told_apart() {
        // 直した緑の枝の効き目。元の式は緑が最大のとき青を見ないので、
        // 緑 (0,255,0) と黄 (255,255,0) の色相が同じになってしまう。
        let g = rgb_to_hls(px(0, 255, 0));
        let y = rgb_to_hls(px(255, 255, 0));
        assert_ne!(g.h, y.h);
    }

    #[test]
    fn the_lightness_test_is_a_ratio_not_a_difference() {
        // 元: 1 - minL/maxL <= l。差ではないので、暗いほうが厳しい。
        let dark = rgb_to_hls(px(10, 10, 10));
        // 同じ灰色どうしは明度が一致するので比を見ずに通る。
        assert!(color_compare(&dark, px(10, 10, 10), 0.0, 0.0));
        // 20/255 と 10/255 は比で 0.5、差なら 0.04 ほど。
        assert!(!color_compare(&dark, px(20, 20, 20), 1.0, 0.4));
        assert!(color_compare(&dark, px(20, 20, 20), 1.0, 0.6));
    }

    #[test]
    fn the_hue_distance_wraps_around() {
        // 色相 0 と 1 は同じ色なので距離 0。
        assert_eq!(hue_diff(0.0, 1.0), 0.0);
        assert_eq!(hue_diff(1.0, 0.0), 0.0);
        assert!((hue_diff(0.1, 0.9) - 0.2).abs() < 1e-12);
        assert!((hue_diff(0.9, 0.1) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn the_32bit_flag_survives_the_float_round_trip() {
        // 旗は 1000 で割った f32 に入れて、読むときに掛けて切り捨てる。
        // 実際に使う値 (0..3 と (番号<<2)|2, |3) で往復が壊れないこと。
        let round = |v: i32| -> i32 { ((v as f32 / CT_FLOAT) * CT_FLOAT) as i32 };
        for v in 0..=3 {
            assert_eq!(round(v), v, "flag {v}");
        }
        for i in 0..(COUNT_OF_MAIN + COUNT_OF_SUB) as i32 {
            for low in [CT_SEARCH, CT_DECISION] {
                let v = (i << 2) | low;
                assert_eq!(round(v), v, "flag {v}");
            }
        }
    }

    #[test]
    fn the_table_colour_survives_the_depth_round_trip() {
        // 16/32bit は色表を変換して書き、比べるときに 8bit に戻す。
        // その往復が元の 8bit と一致しないと深度で結果が変わる。
        for c in 0..=255u8 {
            let p = px(c, c, c);
            assert_eq!(conv_16_to_8(&conv_8_to_16(&p)).red, c, "16bit {c}");
            assert_eq!(conv_32_to_8(&conv_8_to_32(&p)).red, c, "32bit {c}");
        }
    }

    #[test]
    fn the_alpha_border_truncates() {
        // 元: RoundByteFpLong(255 * level + 0.5)。四捨五入ではなく
        // +0.5 してから切り捨て。
        let border = |pct: f64| round_byte_fp_double(MAX_CHANNEL8 as f64 * (pct / 100.0) + 0.5);
        assert_eq!(border(0.0), 0);
        assert_eq!(border(50.0), 128);
        assert_eq!(border(100.0), 255);
    }
}
