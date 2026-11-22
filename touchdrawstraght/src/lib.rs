//! F's TouchDrawStraght の移植。
//!
//! 絵の中の「対象」(指定色と同じところ、または隣と差の大きいところ) の
//! **縁**を探して、そこから `Rot` の向きへ短い線 (タッチ) をたくさん引く。
//! 鉛筆で輪郭を掃いたような絵になる。双子の `touchdraw` は同じ道筋で、
//! 線の向きが `Center` へ向かうところだけが違う。
//!
//! 手順 (元の Exec):
//!
//! 1. 出力を消す
//! 2. `Mode` で対象を探す
//!    - `Color`: 指定色と `Color_Range` のうちで同じ色のところに旗を立て、
//!      そのうち隣に旗の無い画素がある「縁」だけを残す
//!    - `BrightnessDelta` / `AlphaDelta`: 右と下の隣との差が
//!      `Delta_Range` を超えたところに旗を立てる
//! 3. `blockDraw`: `Block_Size` の四角を `Block_Value` の割合だけ
//!    ランダムに置いて、そこの旗を消す (タッチを間引く)
//! 4. `drawTouch`: 旗の立っている画素ごとに `Value` の確からしさで
//!    `Point_Count + 1` 本のタッチを引く
//! 5. `greenBlur`: 貯めた濃さを 3×3 でぼかしてアルファに入れる
//! 6. `copyAlpha`: そのアルファを `Color` に付け、`Original_Blend` が
//!    入っていれば入力の上に重ねる
//!
//! タッチ1本 (元の drawTouchSub):
//!
//! - 始点は旗の画素のまわり `±Point_Length` にばらついた位置
//! - 始点から `Rot` の向きへ `Inside_Length` 画素ぶん進む。同じ向きの
//!   逆へ `Outside_Length` 画素ぶんも引く
//! - 歩数は長さそのもの (画素数)。不透明度は歩数ぶんで 0 まで落ちる
//! - 1歩ごとに小数の位置へ4点に分けて濃さを足す (`libs::touchdraw`)
//!
//! 元から写した細かいところ:
//!
//! - 位置は固定小数 (16.16)、長さは画素数。1歩ぶんのずれは
//!   `向きのベクトル / 長さ` を整数で割ったもの
//! - 長さのばらつきは `上限 - (上限*ばらつき>>16)` から上限までの乱数。
//!   不透明度だけ `(不透明度>>8)*(ばらつき>>8)` を引いた値から
//!   (`>>8` のせいで 100% でもきっちり 0 までは落ちない)
//! - 外向きは `向き / -長さ` を足していくので、割り算の端の落ち方が
//!   内向きと揃わない
//! - 旗の立っていない画素でも乱数を1つ捨てる (元の `else { F_RAND(); }`)
//! - `Point_Length` と `Block_Size` には downsample が掛かる
//! - 四角の数は `画面を Block_Size で割った枚数 × Block_Value`
//! - 乱数の種は四角とタッチで**別々に蒔き直す** (どちらも `RandomSeed`)
//!
//! 元と違うところ (直したバグ) は `libs::touchdraw` の注を参照
//! (`green_blur` の四隅と右の辺、`copy_alpha` がぼかす前の緑を使って
//! いたところ)。`FAITHFUL` で元に戻る。
//!
//! # 突き合わせられていないところ
//!
//! `blockDraw` (`Block_Value` の割合だけ四角を置いて旗を消すところ) だけ
//! は、出荷された .aex と合わせられなかった。四角は乱数4つ (幅・高さ・
//! x・y) で決まるだけなのに、出荷版が消す四角はこちらと違う。大きさ・
//! 枚数・乱数を先に捨てる数を総当たりしても、ソースのとおりの式では
//! 出荷版の消しかたにならない (いちばん近いのは「幅 64・高さ 56 を
//! (1, 7) から」で、これはソースの `bSize/2 .. bSize` の範囲に入らない)。
//!
//! 乱数そのものと、ほかの段はぜんぶ合っている: `Block_Value` を 0 に
//! した回は、種を4通り × ばらつきの入り切りの計8通りの乱数の流れで、
//! どれも画素まで一致する。出荷版の `blockDraw` は手元のソースと違う
//! ものだとみて、ここはソースのとおりに移植してある。突き合わせの回も
//! `Block_Value` は 0 のままにしてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/TouchDrawStraght

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;
use libs::touchdraw::{
    block_draw, copy_alpha, find_target, find_target_delta, green_blur, pset_alpha, Buf, Ops, Px,
};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Clone, Copy, Debug)]
pub struct Info {
    pub target_value: i64,
    pub target_mode: i32,
    pub target_colour: (i64, i64, i64),
    pub colour_range: i64,
    pub delta_range: i64,
    pub rot: i64,
    pub length_i_max: i64,
    pub length_i_rnd: i64,
    pub length_o_max: i64,
    pub length_o_rnd: i64,
    pub opacity: i64,
    pub opacity_rnd: i64,
    pub point_count: i64,
    pub point_length: i64,
}

/// 元の drawTouchSub。始点から `Rot` の向きへ (と、その逆へ) 1本ずつ引く。
fn draw_touch_sub(
    out: &mut Buf,
    start: (i64, i64),
    info: &Info,
    length_i: i64,
    length_o: i64,
    opacity: i64,
    ops: &Ops,
) {
    if opacity <= 0 {
        return;
    }
    for (len, inward) in [(length_i, true), (length_o, false)] {
        if len <= 0 {
            continue;
        }
        // 元の CRotCalc::SetRotLength。長さは画素数なので固定小数に直す。
        let (vx, vy) = libs::utils::rot_length(info.rot as i32, (len << 16) as i32);
        // 外向きは負の数で割るので、端の落ち方が内向きと揃わない (元のまま)。
        let step = if inward { len } else { -len };
        let dx = vx as i64 / step;
        let dy = vy as i64 / step;
        let mut op = opacity;
        let dl = op / len;
        let (mut px, mut py) = start;
        for _ in 0..len {
            if !pset_alpha(out, px, py, op, ops) {
                break;
            }
            px += dx;
            py += dy;
            op -= dl;
        }
    }
}

/// 元の drawTouch。旗の立っている画素ごとにタッチを引く。
pub fn draw_touch(out: &mut Buf, info: &Info, ops: &Ops, rng: &mut XorShift) {
    // 元: F_RAND_MAX * target_value >> 16 (target_value は固定小数)。
    let t0 = (XorShift::RAND_MAX as i64 * info.target_value) >> 16;
    let ir0 = (info.length_i_max - ((info.length_i_max * info.length_i_rnd) >> 16)).max(0);
    let or0 = (info.length_o_max - ((info.length_o_max * info.length_o_rnd) >> 16)).max(0);
    let p0 = (info.opacity - (info.opacity >> 8) * (info.opacity_rnd >> 8)).max(0);
    for y in 0..out.h {
        for x in 0..out.w {
            if out.at(x, y).b != ops.max {
                // 旗が無くても乱数は1つ進む。
                rng.next();
                continue;
            }
            if (rng.next() as i64) >= t0 {
                continue;
            }
            for _ in 0..=info.point_count {
                let sx = (x << 16) + rng.rand2(-info.point_length, info.point_length);
                let sy = (y << 16) + rng.rand2(-info.point_length, info.point_length);
                let li = rng.rand2(ir0, info.length_i_max);
                let lo = rng.rand2(or0, info.length_o_max);
                let op = rng.rand2(p0, info.opacity);
                draw_touch_sub(out, (sx, sy), info, li, lo, op, ops);
            }
        }
    }
}

/// 元の Exec。
pub fn exec(inp: &Buf, out: &mut Buf, info: &Info, seed: u32, colour: &Px, org: bool, block_size: i64, block_count: i64, ops: &Ops) {
    out.px.iter_mut().for_each(|p| *p = Px::default());
    match info.target_mode {
        2 => find_target_delta(inp, out, info.delta_range, false, ops),
        3 => find_target_delta(inp, out, info.delta_range, true, ops),
        _ => find_target(inp, out, info.target_colour, info.colour_range, ops),
    }
    let mut rng = XorShift::new(seed);
    block_draw(out, block_size, block_count, &mut rng);
    let mut rng = XorShift::new(seed);
    draw_touch(out, info, ops, &mut rng);
    green_blur(out, ops, FAITHFUL);
    copy_alpha(inp, out, colour, org, ops, FAITHFUL);
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Seed,
    TargetValue,
    TargetTopic,
    TargetMode,
    TargetColor,
    ColorRange,
    DeltaRange,
    TargetTopicEnd,
    Rot,
    LengthIMax,
    LengthIRnd,
    LengthOMax,
    LengthORnd,
    Color,
    Opacity,
    OpacityRnd,
    PointCount,
    PointLength,
    BlockValue,
    BlockSize,
    Org,
}

#[derive(Default)]
struct Plugin {}

ae::define_effect!(Plugin, (), Params);

/// 元の PF_ADD_FIXED (精度1桁)。
fn fixed_slider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    vmax: f32,
    smax: f32,
    dflt: f64,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(0.0);
            f.set_valid_max(vmax);
            f.set_slider_min(0.0);
            f.set_slider_max(smax);
            f.set_precision(1);
            f.set_curve_tolerance(0.05);
            f.set_default(dflt);
            f.set_value(f.default());
        }),
    )
}

/// 元の PF_ADD_SLIDER (長さ、画素数)。
fn px_slider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    dflt: i32,
) -> Result<(), Error> {
    params.add(
        id,
        name,
        ae::SliderDef::setup(move |f| {
            f.set_valid_min(0);
            f.set_valid_max(32000);
            f.set_slider_min(0);
            f.set_slider_max(200);
            f.set_default(dflt);
            f.set_value(f.default());
        }),
    )
}

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
        // 元: PF_ADD_SLIDER("RandomSeed", 0, 32000, 0, 144, 0)
        params.add(
            Params::Seed,
            "RandomSeed",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(32000);
                f.set_slider_min(0);
                f.set_slider_max(144);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        fixed_slider(params, Params::TargetValue, "Value", 100.0, 25.0, 5.0)?;
        params.add_group(
            Params::TargetTopic,
            Params::TargetTopicEnd,
            "Target",
            true,
            |p| {
                // 元: PF_ADD_POPUP("Mode", 3, 2, "Color|BrightnessDelta|AlphaDelta")
                p.add(
                    Params::TargetMode,
                    "Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Color", "BrightnessDelta", "AlphaDelta"]);
                        f.set_default(2);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_COLOR("Color", 0x00, 0x00, 0x00)
                p.add(
                    Params::TargetColor,
                    "Color",
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
                fixed_slider(p, Params::ColorRange, "Color_Range", 100.0, 25.0, 0.0)?;
                fixed_slider(p, Params::DeltaRange, "Delta_Range", 100.0, 90.0, 50.0)?;
                Ok(())
            },
        )?;
        // 元: PF_ADD_ANGLE("Rot", 45)
        params.add(
            Params::Rot,
            "Rot",
            ae::AngleDef::setup(|f| {
                f.set_default(45.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("Inside_Length", 0, 32000, 0, 200, 30)
        px_slider(params, Params::LengthIMax, "Inside_Length", 30)?;
        fixed_slider(
            params,
            Params::LengthIRnd,
            "Inside_Length_Random",
            100.0,
            100.0,
            50.0,
        )?;
        // 元: PF_ADD_SLIDER("Outside_Length", 0, 32000, 0, 200, 15)
        px_slider(params, Params::LengthOMax, "Outside_Length", 15)?;
        fixed_slider(
            params,
            Params::LengthORnd,
            "Outside_Length_Random",
            100.0,
            100.0,
            50.0,
        )?;
        // 元: PF_ADD_COLOR("Color", 0x00, 0x00, 0x00)
        params.add(
            Params::Color,
            "Color",
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
        fixed_slider(params, Params::Opacity, "Opacity", 100.0, 100.0, 100.0)?;
        fixed_slider(params, Params::OpacityRnd, "Opacity_Random", 100.0, 100.0, 50.0)?;
        // 元: PF_ADD_SLIDER("Point_Count", 0, 100, 0, 10, 2)
        params.add(
            Params::PointCount,
            "Point_Count",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        fixed_slider(params, Params::PointLength, "Point_Length", 300.0, 10.0, 1.0)?;
        fixed_slider(params, Params::BlockValue, "Block_Value", 300.0, 100.0, 50.0)?;
        // 元: PF_ADD_SLIDER("Block_Size", 8, 128, 8, 64, 2)
        params.add(
            Params::BlockSize,
            "Block_Size",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(8);
                f.set_valid_max(128);
                f.set_slider_min(8);
                f.set_slider_max(64);
                f.set_default(2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("Original_Blend", "ON", TRUE)
        params.add(
            Params::Org,
            "Original_Blend",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
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
                out_data.set_return_msg("fs-rs touchdrawstraght");
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
        let ds = in_data.downsample_x();
        // 元の downScaleNoClip。
        let down = |v: i64| -> i64 { v * ds.num as i64 / ds.den as i64 };
        let fixed = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(libs::utils::flt2fix(params.get(p)?.as_float_slider()?.value() as f64) as i64)
        };
        // 元の GetFIXED_PER2ONE: 固定小数のまま 100 で割る (整数の割り算)。
        let per2one = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(fixed(p, params)? / 100)
        };

        // 元の GetParams。
        let seed = params.get(Params::Seed)?.as_slider()?.value() as u32;
        let target_value = per2one(Params::TargetValue, params)?;
        let target_mode = params.get(Params::TargetMode)?.as_popup()?.value();
        let tc = params.get(Params::TargetColor)?.as_color()?.value();
        let colour_range = per2one(Params::ColorRange, params)?;
        let delta_range = per2one(Params::DeltaRange, params)?;
        // 元の GetANGLE と、そのあと 0..360 に収めるところ。
        let rot = {
            let mut v = libs::utils::flt2fix(params.get(Params::Rot)?.as_angle()?.value() as f64)
                as i64
                % (360 << 16);
            if v < 0 {
                v += 360 << 16;
            }
            v
        };
        let length_i_max = params.get(Params::LengthIMax)?.as_slider()?.value() as i64;
        let length_i_rnd = per2one(Params::LengthIRnd, params)?;
        let length_o_max = params.get(Params::LengthOMax)?.as_slider()?.value() as i64;
        let length_o_rnd = per2one(Params::LengthORnd, params)?;
        let colour8 = params.get(Params::Color)?.as_color()?.value();
        let opacity = per2one(Params::Opacity, params)?;
        let opacity_rnd = per2one(Params::OpacityRnd, params)?;
        let point_count = params.get(Params::PointCount)?.as_slider()?.value() as i64;
        // 元: downScaleNoClip してから 1.0 を足す。
        let point_length = down(fixed(Params::PointLength, params)?) + (1 << 16);
        let block_value = per2one(Params::BlockValue, params)?;
        let block_size = down(params.get(Params::BlockSize)?.as_slider()?.value() as i64).max(2);
        // 元: 画面を四角で割った枚数 × Block_Value。
        let tiles = |n: i64| -> i64 {
            if n % block_size != 0 {
                n / block_size + 1
            } else {
                n / block_size
            }
        };
        let block_count =
            (tiles(in_data.width() as i64) * tiles(in_data.height() as i64) * block_value) >> 16;
        let org = params.get(Params::Org)?.as_checkbox()?.value();

        let depth = out_layer.bit_depth();
        let ops = Ops {
            max: match depth {
                8 => 255.0,
                16 => 32768.0,
                _ => 1.0,
            },
            float: depth > 16,
        };
        let colour = match depth {
            8 => Px {
                a: colour8.alpha as f64,
                r: colour8.red as f64,
                g: colour8.green as f64,
                b: colour8.blue as f64,
            },
            16 => {
                let c = libs::utils::conv_8_to_16(&colour8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
            _ => {
                let c = libs::utils::conv_8_to_32(&colour8);
                Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64,
                }
            }
        };

        let info = Info {
            target_value,
            target_mode,
            target_colour: (tc.red as i64, tc.green as i64, tc.blue as i64),
            colour_range,
            delta_range,
            rot,
            length_i_max,
            length_i_rnd,
            length_o_max,
            length_o_rnd,
            opacity,
            opacity_rnd,
            point_count,
            point_length,
        };

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        if w == 0 || h == 0 {
            return Ok(());
        }
        let mut inp = Buf::new(w, h);
        let mut out = Buf::new(w, h);
        macro_rules! go {
            ($wr:ident, $get:expr, $put:expr) => {{
                let src = &in_layer;
                for y in 0..h {
                    for x in 0..w {
                        inp.set(x, y, $get(src.$wr(x as usize, y as usize)));
                    }
                }
                exec(
                    &inp,
                    &mut out,
                    &info,
                    seed,
                    &colour,
                    org,
                    block_size,
                    block_count,
                    &ops,
                );
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x as usize, y as usize), &out.at(x, y));
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                |p: &mut ae::Pixel8| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::Pixel8, s: &Px| {
                    d.alpha = s.a as i64 as u8;
                    d.red = s.r as i64 as u8;
                    d.green = s.g as i64 as u8;
                    d.blue = s.b as i64 as u8;
                }
            ),
            16 => go!(
                as_pixel16_mut,
                |p: &mut ae::Pixel16| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::Pixel16, s: &Px| {
                    d.alpha = s.a as i64 as u16;
                    d.red = s.r as i64 as u16;
                    d.green = s.g as i64 as u16;
                    d.blue = s.b as i64 as u16;
                }
            ),
            _ => go!(
                as_pixel32_mut,
                |p: &mut ae::PixelF32| Px {
                    a: p.alpha as f64,
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64
                },
                |d: &mut ae::PixelF32, s: &Px| {
                    d.alpha = s.a as f32;
                    d.red = s.r as f32;
                    d.green = s.g as f32;
                    d.blue = s.b as f32;
                }
            ),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops8() -> Ops {
        Ops {
            max: 255.0,
            float: false,
        }
    }

    fn info() -> Info {
        Info {
            target_value: 1 << 16,
            target_mode: 1,
            target_colour: (0, 0, 0),
            colour_range: 0,
            delta_range: 0,
            rot: 90 << 16,
            length_i_max: 8,
            length_i_rnd: 0,
            length_o_max: 0,
            length_o_rnd: 0,
            opacity: 1 << 16,
            opacity_rnd: 0,
            point_count: 0,
            point_length: 0,
        }
    }

    /// 角度 90 は右向き。手前が濃くて先が薄い。
    #[test]
    fn one_touch_goes_right() {
        let o = ops8();
        let i = info();
        let mut out = Buf::new(21, 21);
        draw_touch_sub(&mut out, (2 << 16, 2 << 16), &i, 8, 0, 1 << 16, &o);
        assert!(out.at(2, 2).g > 0.0, "始点に乗る");
        assert!(out.at(8, 2).g > 0.0, "右へ伸びる");
        assert_eq!(out.at(2, 8).g, 0.0, "下へは伸びない");
        assert!(
            out.at(8, 2).g < out.at(3, 2).g,
            "先へ向かって薄くなる: {} {}",
            out.at(3, 2).g,
            out.at(8, 2).g
        );
    }

    /// 長さを縮めるとそこで止まる。
    #[test]
    fn a_short_length_stops_early() {
        let o = ops8();
        let i = info();
        let mut out = Buf::new(21, 21);
        draw_touch_sub(&mut out, (2 << 16, 2 << 16), &i, 3, 0, 1 << 16, &o);
        assert!(out.at(4, 2).g > 0.0);
        assert_eq!(out.at(8, 2).g, 0.0, "3 画素より先には行かない");
    }

    /// 外向きは逆 (左) へ伸びる。
    #[test]
    fn the_outside_length_goes_the_other_way() {
        let o = ops8();
        let i = info();
        let mut out = Buf::new(21, 21);
        draw_touch_sub(&mut out, (10 << 16, 10 << 16), &i, 0, 8, 1 << 16, &o);
        assert!(out.at(5, 10).g > 0.0, "左へ伸びる");
        assert_eq!(out.at(15, 10).g, 0.0, "右へは伸びない");
    }

    /// 不透明度 0 なら何も引かない。
    #[test]
    fn a_zero_opacity_draws_nothing() {
        let o = ops8();
        let i = info();
        let mut out = Buf::new(21, 21);
        draw_touch_sub(&mut out, (2 << 16, 2 << 16), &i, 8, 8, 0, &o);
        assert!(out.px.iter().all(|p| p.g == 0.0));
    }

    /// 旗の立っているところからだけタッチが出る。
    #[test]
    fn the_touches_start_from_the_flagged_pixels() {
        let o = ops8();
        let i = info();
        let mut out = Buf::new(21, 21);
        out.px[(3 + 3 * 21) as usize].b = 255.0;
        let mut rng = XorShift::new(0);
        draw_touch(&mut out, &i, &o, &mut rng);
        assert!(out.at(3, 3).g > 0.0, "旗のところから引かれる");
        assert_eq!(out.at(18, 18).g, 0.0, "旗の無いところからは出ない");
    }
}

