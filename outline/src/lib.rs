//! F's OutLine の移植。
//!
//! 入力のアルファを Value ぶん収縮させ、「元のアルファ - 収縮後」を
//! 輪郭の帯としてアルファにし、RGB は指定色で塗る。Level はその帯の
//! 濃さの倍率。
//!
//! 元の流れ (深度ごとに同じ):
//!   1. out.red = in.alpha、out.green = 0、out.alpha = 最大値
//!   2. 横方向の最小値フィルタ: red を見て green に書く
//!   3. 縦方向の最小値フィルタ: green を見て alpha に書く
//!   4. out = 色、out.alpha = clamp((in.alpha - out.alpha) * Level)
//!
//! 2 と 3 は元と同じく **窓の端を 0 で埋める**。左/上は Value 列、
//! 右/下は Value+1 列ぶん 0 になる (元の条件が `x < value` と
//! `x >= width-1-value` で非対称)。ここも元のまま。
//!
//! 元は専用の最小値フィルタを持っていて [[MinimaxA]] は通らないので、
//! AEXCompat でそのまま比較できる。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/OutLine

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{conv_8_to_16, conv_8_to_32};

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    Color,
    Level,
}

/// 元の MinHor* / MinVur* をまとめたもの。
///
/// `src` を見て `dst` に、各点を中心とする幅 2*value+1 の窓の最小値を書く。
/// `horizontal` なら行ごと、そうでなければ列ごとに走る。窓が端にかかる
/// ところは 0 で埋める (元のまま、右/下だけ1列ぶん広い)。
///
/// 値を f64 で持つのは深度をまとめるため。最小値を取るだけなので、
/// 8/16bit の整数も 32bit の実数も f64 にすれば比較結果は元と同じ。
fn min_pass(src: &[f64], dst: &mut [f64], w: usize, h: usize, value: usize, horizontal: bool) {
    let (n, outer) = if horizontal { (w, h) } else { (h, w) };
    let imax = n as i64 - 1 - value as i64;
    for o in 0..outer {
        let at = |i: usize| if horizontal { o * w + i } else { i * w + o };
        // 元は行 (列) ごとに minIndex を -1000 で始めるので、最初の点では
        // 必ず窓を全部なめる。
        let mut min_index: i64 = -1000;
        let mut min_value = 0.0f64;
        for i in 0..n {
            if (i as i64) < value as i64 || i as i64 >= imax {
                dst[at(i)] = 0.0;
                continue;
            }
            let k0 = i as i64 - value as i64;
            let k1 = i as i64 + value as i64;
            if k0 < min_index {
                // 窓が前回の最小値を含むので、入ってきた端だけ見る。
                let k1p = src[at(k1 as usize)];
                if min_value >= k1p {
                    min_index = k1;
                    min_value = k1p;
                }
            } else {
                min_index = k0;
                min_value = src[at(k0 as usize)];
                for k in (k0 + 1)..=k1 {
                    let kp = src[at(k as usize)];
                    if min_value >= kp {
                        min_index = k;
                        min_value = kp;
                    }
                }
            }
            dst[at(i)] = min_value;
        }
    }
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
        // 元: PF_ADD_SLIDER(STR_Y, 1, 100, 1, 10, 1, ID_Y)
        params.add(
            Params::Value,
            "Value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(1);
                f.set_valid_max(100);
                f.set_slider_min(1);
                f.set_slider_max(10);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_COLOR(STR_COLOR, 0xFF, 0xFF, 0xFF, ID_COLOR)
        params.add(
            Params::Color,
            "Color",
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

        // 元: PF_ADD_FLOAT_SLIDER(STR_LEVEL, 0, 400, 100, 200, 1, 100, 1, 0, 0, ID_LEVEL)
        params.add(
            Params::Level,
            "Level(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(400.0);
                f.set_slider_min(100.0);
                f.set_slider_max(200.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(100.0);
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
                out_data.set_return_msg("fs-rs outline");
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
        let value = params.get(Params::Value)?.as_slider()?.value().max(0) as usize;
        let col8 = params.get(Params::Color)?.as_color()?.value();
        // 元の GetParams: 負なら 0 にして /100。
        let level = {
            let v = params.get(Params::Level)?.as_float_slider()?.value();
            (if v < 0.0 { 0.0 } else { v }) / 100.0
        };

        let (w, h) = (out_layer.width(), out_layer.height());
        let (in_w, in_h) = (in_layer.width(), in_layer.height());
        let depth = out_layer.bit_depth();
        if in_layer.bit_depth() != depth {
            return Err(Error::BadCallbackParameter);
        }
        if w == 0 || h == 0 {
            return Ok(());
        }

        // 元は out.red に入力のアルファを入れてから最小値フィルタを
        // 2回かける (red -> green -> alpha)。ここではその3枚を配列で持つ。
        let mut src = vec![0.0f64; w * h];
        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                if y >= in_h || x >= in_w {
                    continue;
                }
                src[i] = match depth {
                    8 => in_layer.as_pixel8(x, y).alpha as f64,
                    16 => in_layer.as_pixel16(x, y).alpha as f64,
                    _ => in_layer.as_pixel32(x, y).alpha as f64,
                };
            }
        }
        let mut mid = vec![0.0f64; w * h];
        let mut ero = vec![0.0f64; w * h];
        min_pass(&src, &mut mid, w, h, value, true);
        min_pass(&mid, &mut ero, w, h, value, false);

        let col16 = conv_8_to_16(&col8);
        let col32 = conv_8_to_32(&col8);

        for y in 0..h {
            for x in 0..w {
                let i = y * w + x;
                match depth {
                    8 => {
                        // 元: v = in.alpha - 収縮後。Level が 1 以外なら
                        // 掛けて +0.5 で丸める。
                        let mut v = src[i] - ero[i];
                        if level != 1.0 {
                            v = v * level + 0.5;
                        }
                        let v = (v as i32).clamp(0, MAX_CHANNEL8 as i32) as u8;
                        let mut p = col8;
                        p.alpha = v;
                        *out_layer.as_pixel8_mut(x, y) = p;
                    }
                    16 => {
                        let mut v = src[i] - ero[i];
                        if level != 1.0 {
                            v = v * level + 0.5;
                        }
                        let v = (v as i32).clamp(0, MAX_CHANNEL16 as i32) as u16;
                        let mut p = col16;
                        p.alpha = v;
                        *out_layer.as_pixel16_mut(x, y) = p;
                    }
                    _ => {
                        // 元の 32bit 版は +0.5 しない。
                        let mut v = src[i] - ero[i];
                        if level != 1.0 {
                            v *= level;
                        }
                        let mut p = col32;
                        p.alpha = v.clamp(0.0, 1.0) as f32;
                        *out_layer.as_pixel32_mut(x, y) = p;
                    }
                }
            }
        }
        Ok(())
    }
}
