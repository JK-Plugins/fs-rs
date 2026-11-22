//! F's AnimatedNoise の移植。
//!
//! アナログの映像ノイズを2段で乗せる。
//!
//! 1. **ブロックノイズ**。乱数で選んだ場所に長方形を置き、その中の平均色に
//!    乱数の明暗を足した1色で塗り潰す。数は「ノイズの量 × ブロックの量」
//! 2. **粒のノイズ**。内側の画素1つずつに、確率でノイズを足す。中心は
//!    まるごと、上下左右は半分、斜めは1/4ぶん。隣にも配るので粒が少し
//!    広がって見える
//!
//! 「毎フレームでノイズ変化」を入れると種はフレーム番号の2倍、切ると
//! 「ノイズの動き」のスライダーの値になる (スライダーは入っているあいだ
//! 灰色になる)。**どちらの段も同じ種から蒔き直す**ので、ブロックと粒は
//! 同じ乱数の並びを別々に使う。
//!
//! 元から写した細かいところ:
//!
//! - ノイズの強さとブロックの強さは `PF_Fixed / 100` の整数割り算
//!   (切り捨て)。掛けたあと `>>16` で戻す。負の値の `>>16` は下に丸める
//! - モノクロのときは乱数を1回だけ使い、**残り2回は捨てる**
//! - 粒の段は内側 (1..幅-2, 1..高さ-2) だけ。縁には乗らない
//! - 8/16bit では隣に配るぶんが整数割り算 (`ar/2`, `ar/4`) なので
//!   0 方向へ切り捨てる。32bit は実数のまま割る
//! - ブロックの大きさは `幅/2 + 1` から倍までの乱数。位置は画面の外にも
//!   出て、縁で切られる。切った結果が空なら何も塗らない
//! - ブロックの平均色は**出力ワールドのそのときの中身**から取るので、
//!   先に置いたブロックが次の平均に効く
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/AnimatedNoise

use after_effects::{self as ae};
use libs::fs_graphics::XorShift;

/// 読み込んだパラメータ。元の PrmTbl のうち UI から来るぶん。
#[derive(Debug, Clone, Copy)]
struct Info {
    anim_flag: bool,
    anim: i64,
    value: i64,
    /// 元の length (PF_Fixed を 100 で整数割り算したもの)
    length: i64,
    color_flag: bool,
    block_value: i64,
    block_length: i64,
    block_width: i64,
    block_height: i64,
    block_color_flag: bool,
}

/// 元の F_RAND2。
fn rand2(rng: &mut XorShift, lo: i64, hi: i64) -> i64 {
    rng.rand2(lo, hi)
}

/// 深度ごとの本体。元の NoiseBlock* / NoiseMain* をそのまま写す。
///
/// 引数で変わるのは画素の型と、ノイズの振幅の作り方・足し方・隣への
/// 配り方だけ。
macro_rules! depth_impl {
    (
        $name:ident, $px:ty, $amp:ty,
        // 粒のノイズ1つぶんの振幅
        $main_amp:expr,
        // ブロック1つぶんの振幅
        $block_amp:expr,
        // 画素に足す (画素, 振幅) -> 新しいチャンネル値
        $add:expr,
        // 平均と振幅から塗る色を作る
        $mean:expr,
        // 隣に配るぶん (半分・1/4)
        $half:expr, $quarter:expr,
        // 平均を取るための足し込み
        $acc_zero:expr, $acc_add:expr, $acc_div:expr
    ) => {
        fn $name(buf: &mut [$px], w: i64, h: i64, info: &Info, seed: u32) {
            // 元: v = (量 × ブロックの量) >> 16。
            let v = (info.value * info.block_value) >> 16;
            if v > 0 {
                let mut rng = XorShift::new(seed);
                for _ in 0..v {
                    let bx = rand2(&mut rng, -info.block_width, w);
                    let by = rand2(&mut rng, -info.block_height, h);
                    // ---- 元の NoiseBlock*
                    let x0 = bx.max(0);
                    let y0 = by.max(0);
                    if x0 >= w || y0 >= h {
                        continue;
                    }
                    let temp = info.block_width / 2 + 1;
                    let bw = temp + temp * rng.next() as i64 / XorShift::RAND_MAX as i64;
                    let temp = info.block_height / 2 + 1;
                    let bh = temp + temp * rng.next() as i64 / XorShift::RAND_MAX as i64;
                    let x1 = (bx + bw).min(w);
                    let y1 = (by + bh).min(h);
                    if x1 <= 0 || y1 <= 0 {
                        continue;
                    }
                    let (bw, bh) = (x1 - x0, y1 - y0);
                    if bw <= 0 || bh <= 0 {
                        continue;
                    }
                    let mut acc = $acc_zero;
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let p = &buf[(x + y * w) as usize];
                            $acc_add(&mut acc, p);
                        }
                    }
                    let n = bw * bh;
                    let mean = $acc_div(acc, n);
                    let ar: $amp = $block_amp(&mut rng, info.block_length);
                    let (ag, ab): ($amp, $amp) = if info.block_color_flag {
                        (
                            $block_amp(&mut rng, info.block_length),
                            $block_amp(&mut rng, info.block_length),
                        )
                    } else {
                        // 元: 使わないぶんも2回引いて進める。
                        rng.next();
                        rng.next();
                        (ar, ar)
                    };
                    let col = $mean(mean, ar, ag, ab);
                    for y in y0..y1 {
                        for x in x0..x1 {
                            let p = &mut buf[(x + y * w) as usize];
                            p.red = col.0;
                            p.green = col.1;
                            p.blue = col.2;
                        }
                    }
                }
            }
            // ---- 元の NoiseMain*
            if info.value > 0 && w >= 3 && h >= 3 {
                let mut rng = XorShift::new(seed);
                for y in 1..h - 1 {
                    for x in 1..w - 1 {
                        let target = x + y * w;
                        // 元: 確率の判定。外れた回はここで戻るので、
                        // 色の乱数は引かない。
                        let r = rng.fm_rand(100);
                        if r >= info.value {
                            continue;
                        }
                        let ar: $amp = $main_amp(&mut rng, info.length);
                        let (ag, ab): ($amp, $amp) = if info.color_flag {
                            (
                                $main_amp(&mut rng, info.length),
                                $main_amp(&mut rng, info.length),
                            )
                        } else {
                            rng.next();
                            rng.next();
                            (ar, ar)
                        };
                        let (ar2, ag2, ab2) = ($half(ar), $half(ag), $half(ab));
                        let (ar3, ag3, ab3) = ($quarter(ar), $quarter(ag), $quarter(ab));
                        for (t, a) in [
                            (target, (ar, ag, ab)),
                            (target - 1, (ar2, ag2, ab2)),
                            (target + 1, (ar2, ag2, ab2)),
                            (target - w, (ar2, ag2, ab2)),
                            (target + w, (ar2, ag2, ab2)),
                            (target - w - 1, (ar3, ag3, ab3)),
                            (target - w + 1, (ar3, ag3, ab3)),
                            (target + w - 1, (ar3, ag3, ab3)),
                            (target + w + 1, (ar3, ag3, ab3)),
                        ] {
                            let p = &mut buf[t as usize];
                            p.red = $add(p.red, a.0);
                            p.green = $add(p.green, a.1);
                            p.blue = $add(p.blue, a.2);
                        }
                    }
                }
            }
        }
    };
}

depth_impl!(
    noise8,
    ae::Pixel8,
    i64,
    // 元: -255 + (A_long)(F_RAND() * (255*2+1.0)/(1.0+RAND_MAX)) を強さで縮める
    |rng: &mut XorShift, len: i64| {
        let a = -255 + (rng.next() as f64 * (255.0 * 2.0 + 1.0) / (1.0 + 32767.0)) as i64;
        (a * len) >> 16
    },
    // 元: 255 - F_RAND()*255*2/RAND_MAX (整数)
    |rng: &mut XorShift, len: i64| {
        let a = 255 - rng.next() as i64 * 255 * 2 / 32767;
        (a * len) >> 16
    },
    |v: u8, a: i64| (v as i64 + a).clamp(0, 255) as u8,
    |mean: (i64, i64, i64), ar: i64, ag: i64, ab: i64| (
        (mean.0 + ar).clamp(0, 255) as u8,
        (mean.1 + ag).clamp(0, 255) as u8,
        (mean.2 + ab).clamp(0, 255) as u8
    ),
    |a: i64| a / 2,
    |a: i64| a / 4,
    (0i64, 0i64, 0i64),
    |acc: &mut (i64, i64, i64), p: &ae::Pixel8| {
        acc.0 += p.red as i64;
        acc.1 += p.green as i64;
        acc.2 += p.blue as i64;
    },
    |acc: (i64, i64, i64), n: i64| (acc.0 / n, acc.1 / n, acc.2 / n)
);

depth_impl!(
    noise16,
    ae::Pixel16,
    i64,
    |rng: &mut XorShift, len: i64| {
        let a = -32768 + (rng.next() as f64 * (32768.0 * 2.0 + 1.0) / (1.0 + 32767.0)) as i64;
        (a * len) >> 16
    },
    |rng: &mut XorShift, len: i64| {
        let a = 32768 - rng.next() as i64 * 32768 * 2 / 32767;
        (a * len) >> 16
    },
    |v: u16, a: i64| (v as i64 + a).clamp(0, 32768) as u16,
    |mean: (i64, i64, i64), ar: i64, ag: i64, ab: i64| (
        (mean.0 + ar).clamp(0, 32768) as u16,
        (mean.1 + ag).clamp(0, 32768) as u16,
        (mean.2 + ab).clamp(0, 32768) as u16
    ),
    |a: i64| a / 2,
    |a: i64| a / 4,
    (0i64, 0i64, 0i64),
    |acc: &mut (i64, i64, i64), p: &ae::Pixel16| {
        acc.0 += p.red as i64;
        acc.1 += p.green as i64;
        acc.2 += p.blue as i64;
    },
    |acc: (i64, i64, i64), n: i64| (acc.0 / n, acc.1 / n, acc.2 / n)
);

depth_impl!(
    noise32,
    ae::PixelF32,
    f32,
    // 元: (PF_FpShort)(-1 + (F_RAND()*2.0+1.0)/(1.0+RAND_MAX))
    |rng: &mut XorShift, len: i64| {
        let a = (-1.0 + (rng.next() as f64 * 2.0 + 1.0) / (1.0 + 32767.0)) as f32;
        a * len as f32 / 65536.0
    },
    // 元: (PF_FpShort)(1 - ((PF_FpShort)F_RAND()*2/RAND_MAX))
    |rng: &mut XorShift, len: i64| {
        let a = 1.0f32 - (rng.next() as f32 * 2.0 / 32767.0);
        a * len as f32 / 65536.0
    },
    |v: f32, a: f32| libs::utils::round_fp_short_double((v + a) as f64),
    |mean: (f32, f32, f32), ar: f32, ag: f32, ab: f32| (
        libs::utils::round_fp_short_double((mean.0 + ar) as f64),
        libs::utils::round_fp_short_double((mean.1 + ag) as f64),
        libs::utils::round_fp_short_double((mean.2 + ab) as f64)
    ),
    |a: f32| a / 2.0,
    |a: f32| a / 4.0,
    (0f32, 0f32, 0f32),
    |acc: &mut (f32, f32, f32), p: &ae::PixelF32| {
        acc.0 += p.red;
        acc.1 += p.green;
        acc.2 += p.blue;
    },
    |acc: (f32, f32, f32), n: i64| (acc.0 / n as f32, acc.1 / n as f32, acc.2 / n as f32)
);

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Animated,
    AnimatedAdd,
    Value,
    Length,
    Color,
    BlockValue,
    BlockLength,
    BlockWidth,
    BlockHeight,
    BlockColor,
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
        // 元: PF_ADD_CHECKBOX("毎フレームでノイズ変化", "ON", TRUE)。
        // 名前は元が非 ASCII なので英語。
        params.add_with_flags(
            Params::Animated,
            "Animate",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(true);
                f.set_value(f.default());
            }),
            ae::ParamFlag::SUPERVISE | ae::ParamFlag::CANNOT_TIME_VARY,
            ae::ParamUIFlags::empty(),
        )?;
        // 元: PF_ADD_SLIDER("ノイズの動き", 0, F_RAND_MAX, 0, F_RAND_MAX/10, 0)
        // で、最初から灰色 (PF_PUI_DISABLED)。
        params.add_with_flags(
            Params::AnimatedAdd,
            "Noise Phase",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(0x7FFF);
                f.set_slider_min(0);
                f.set_slider_max(0x7FFF / 10);
                f.set_default(0);
                f.set_value(f.default());
            }),
            ae::ParamFlag::empty(),
            ae::ParamUIFlags::DISABLED,
        )?;
        // 元: PF_ADD_SLIDER("ノイズの量", 0, 100, 0, 100, 10)
        params.add(
            Params::Value,
            "Noise Amount",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(100);
                f.set_slider_min(0);
                f.set_slider_max(100);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("ノイズの強さ", 0, 100, 0, 100, 20, 2, 0, 0)
        params.add(
            Params::Length,
            "Noise Strength",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(2);
                f.set_curve_tolerance(0.05);
                f.set_default(20.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_CHECKBOX("カラーノイズ", "ON", FALSE)
        params.add(
            Params::Color,
            "Colour Noise",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("ON");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("ブロックの量", 0, 300, 0, 5, 1, 1, 0, 0)
        params.add(
            Params::BlockValue,
            "Block Amount",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(300.0);
                f.set_slider_min(0.0);
                f.set_slider_max(5.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(1.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_FIXED("ブロックの強さ", 0, 100, 0, 50, 5, 1, 0, 0)
        params.add(
            Params::BlockLength,
            "Block Strength",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(50.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(5.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("ブロックの横幅(px)"/"縦幅(px)", 3, 512, 3, 100, 16)
        for (p, name) in [
            (Params::BlockWidth, "Block Width(px)"),
            (Params::BlockHeight, "Block Height(px)"),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(3);
                    f.set_valid_max(512);
                    f.set_slider_min(3);
                    f.set_slider_max(100);
                    f.set_default(16);
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_CHECKBOX("カラーブロック", "ON", FALSE)
        params.add(
            Params::BlockColor,
            "Colour Block",
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
                out_data.set_return_msg("fs-rs animatednoise");
            }
            ae::Command::GlobalSetup => {
                self.global_setup(&in_data)?;
            }
            ae::Command::UserChangedParam { .. } => {
                // 元の HandleChangedParam: 毎フレーム変化が入っていれば
                // 「ノイズの動き」を灰色にする。
                let on = params.get(Params::Animated)?.as_checkbox()?.value();
                params
                    .get_mut(Params::AnimatedAdd)?
                    .set_ui_flag(ae::ParamUIFlags::DISABLED, on);
                out_data.set_force_rerender();
                out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
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
        // 元の GetParams。強さは PF_Fixed を 100 で整数割り算 (切り捨て)。
        let fix100 = |p: Params, params: &mut ae::Parameters<Params>| -> Result<i64, Error> {
            Ok(libs::utils::flt2fix(params.get(p)?.as_float_slider()?.value()) as i64 / 100)
        };
        let info = Info {
            anim_flag: params.get(Params::Animated)?.as_checkbox()?.value(),
            anim: params.get(Params::AnimatedAdd)?.as_slider()?.value() as i64,
            value: params.get(Params::Value)?.as_slider()?.value() as i64,
            length: fix100(Params::Length, params)?,
            color_flag: params.get(Params::Color)?.as_checkbox()?.value(),
            // 元: ブロックの量は PF_Fixed のまま使う。
            block_value: libs::utils::flt2fix(
                params.get(Params::BlockValue)?.as_float_slider()?.value(),
            ) as i64,
            block_length: fix100(Params::BlockLength, params)?,
            block_width: params.get(Params::BlockWidth)?.as_slider()?.value() as i64,
            block_height: params.get(Params::BlockHeight)?.as_slider()?.value() as i64,
            block_color_flag: params.get(Params::BlockColor)?.as_checkbox()?.value(),
        };

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をそのままコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if w == 0 || h == 0 {
            return Ok(());
        }

        // 元: 毎フレーム変化ならフレーム番号の2倍、切っていればスライダー。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { time / step } else { 0 } as i64;
        let seed = if info.anim_flag {
            frame * 2
        } else {
            info.anim
        } as u32;

        macro_rules! run {
            ($rd:ident, $wr:ident, $f:ident) => {{
                let mut buf = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        buf.push(*out_layer.$rd(x, y));
                    }
                }
                $f(&mut buf, w as i64, h as i64, &info, seed);
                for y in 0..h {
                    for x in 0..w {
                        *out_layer.$wr(x, y) = buf[x + y * w];
                    }
                }
            }};
        }
        match depth {
            8 => run!(as_pixel8, as_pixel8_mut, noise8),
            16 => run!(as_pixel16, as_pixel16_mut, noise16),
            _ => run!(as_pixel32, as_pixel32_mut, noise32),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> Info {
        Info {
            anim_flag: true,
            anim: 0,
            value: 10,
            // 20% を PF_Fixed にして 100 で割ったもの。
            length: 20 * 65536 / 100,
            color_flag: false,
            block_value: 65536,
            block_length: 5 * 65536 / 100,
            block_width: 16,
            block_height: 16,
            block_color_flag: false,
        }
    }

    fn grey(v: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: 255,
            red: v,
            green: v,
            blue: v,
        }
    }

    /// ノイズが乗る。
    #[test]
    fn the_noise_changes_pixels() {
        let (w, h) = (64i64, 64i64);
        let mut buf = vec![grey(128); (w * h) as usize];
        noise8(&mut buf, w, h, &info(), 0);
        let n = buf.iter().filter(|p| p.red != 128).count();
        assert!(n > 100, "n={n}");
    }

    /// 量 0・ブロックの量 0 なら何も起きない。
    #[test]
    fn nothing_happens_without_the_amount() {
        let (w, h) = (32i64, 32i64);
        let mut i = info();
        i.value = 0;
        let mut buf = vec![grey(128); (w * h) as usize];
        noise8(&mut buf, w, h, &i, 0);
        assert!(buf.iter().all(|p| p.red == 128));
    }

    /// 種を変えると絵が変わる。
    #[test]
    fn the_seed_changes_the_picture() {
        let (w, h) = (48i64, 48i64);
        let mut a = vec![grey(100); (w * h) as usize];
        let mut b = a.clone();
        noise8(&mut a, w, h, &info(), 0);
        noise8(&mut b, w, h, &info(), 7);
        assert!(a.iter().zip(b.iter()).any(|(x, y)| x.red != y.red));
    }

    /// モノクロなら RGB が同じだけ動く。カラーならばらける。
    #[test]
    fn the_colour_flag_splits_the_channels() {
        let (w, h) = (48i64, 48i64);
        let mut i = info();
        // ブロックは混ぜないで粒だけ見る。
        i.block_value = 0;
        let mut mono = vec![grey(128); (w * h) as usize];
        noise8(&mut mono, w, h, &i, 3);
        assert!(mono.iter().all(|p| p.red == p.green && p.green == p.blue));
        i.color_flag = true;
        let mut col = vec![grey(128); (w * h) as usize];
        noise8(&mut col, w, h, &i, 3);
        assert!(col.iter().any(|p| p.red != p.green));
    }

    /// 粒は内側だけを回るが、隣に配るぶんで縁の画素も動く
    /// (いちばん外の輪は 1/4 と 1/2 ぶんだけ)。はみ出した読み書きは
    /// しない。
    #[test]
    fn the_grain_reaches_the_border_but_stays_inside() {
        let (w, h) = (40i64, 40i64);
        let mut i = info();
        i.block_value = 0;
        i.value = 100;
        let mut buf = vec![grey(128); (w * h) as usize];
        noise8(&mut buf, w, h, &i, 1);
        assert!(buf.iter().any(|p| p.red != 128));
        // 幅や高さが 3 未満なら粒の段には入らない (落ちないこと)。
        let mut tiny = vec![grey(128); 2];
        noise8(&mut tiny, 2, 1, &i, 1);
        assert!(tiny.iter().all(|p| p.red == 128));
    }

    /// ブロックは1色で塗り潰される。
    #[test]
    fn a_block_is_filled_with_one_colour() {
        let (w, h) = (64i64, 64i64);
        let mut i = info();
        // 粒は強さ 0 で止めて、ブロックを1つだけ置く
        // ((量 × ブロックの量) >> 16 が 1 になる値)。
        i.length = 0;
        i.value = 100;
        i.block_value = 700;
        i.block_width = 40;
        i.block_height = 40;
        let mut buf = vec![grey(128); (w * h) as usize];
        noise8(&mut buf, w, h, &i, 5);
        let mut v: Vec<u8> = buf.iter().map(|p| p.red).collect();
        v.sort_unstable();
        v.dedup();
        assert_eq!(v.len(), 2, "{:?}", v);
    }

    /// アルファは触らない。
    #[test]
    fn the_alpha_is_left_alone() {
        let (w, h) = (32i64, 32i64);
        let mut buf = vec![
            ae::Pixel8 {
                alpha: 77,
                red: 128,
                green: 128,
                blue: 128
            };
            (w * h) as usize
        ];
        noise8(&mut buf, w, h, &info(), 2);
        assert!(buf.iter().all(|p| p.alpha == 77));
    }
}
