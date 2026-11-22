//! F's RandomMosaic2nd の移植。
//!
//! 画面を桝目に区切り、桝ごとに決めた確率でモザイクにする。`RandomMosaic` が
//! 大きさも位置も乱数でばらまくのに対して、こちらは並びは固定で「どの桝を
//! つぶすか」だけが乱数。塗りつぶしは同じ `libs::fs_mosaic::flat`
//! (元の flat8/16/32)。
//!
//! 元のパラメータ名はすべて日本語で、出荷 .aex では文字化けしているので
//! 英語名にしてある。
//!
//! 元から写した細かいところ:
//!
//! - 種はフレーム番号 (`current_time / time_step`)。毎フレーム別の絵になる
//! - 桝の数は切り上げ (`幅 / 横サイズ`、余りがあれば +1)。最後の桝は
//!   画面からはみ出すが、はみ出したぶんは `flat` が詰める
//! - 量 (%) は固定小数のまま 100 で割り、`FM_RAND(1<<16)` と比べる
//!   (`乱数 <= 量` ならつぶす)。整数の割り算なので 100% で必ずつぶれる
//! - **つぶさない桝でも乱数を1回捨てて進める**。ここを飛ばすと以降の
//!   並びがずれる
//! - 横サイズ・縦サイズは `downScale` (+0.5 で丸め)
//! - 強さは 16/32bit では 8bit から 16bit に直した値で引く。32bit は
//!   それを PF_MAX_CHAN16 で割って実数にする
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/RandomMosaic2nd

use after_effects::{self as ae, MAX_CHANNEL16};
use libs::fs_graphics::XorShift;
use libs::fs_mosaic::flat;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Value,
    Strength,
    SizeX,
    SizeY,
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
        // 元: PF_ADD_FIXED(量(%), 0, 100, 0, 100, 0, 1, 0, 0, ..)
        // 元は FIX_SLIDER だがリポジトリの他と揃えて float スライダーにしている。
        params.add(
            Params::Value,
            "Amount(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(強さ, 0, 255, 0, 128, 10, ..)
        params.add(
            Params::Strength,
            "Strength",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(255);
                f.set_slider_min(0);
                f.set_slider_max(128);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER(横/縦サイズ, 1, 256, 1, 256, 8, ..)
        for (p, name) in [(Params::SizeX, "SizeX"), (Params::SizeY, "SizeY")] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(1);
                    f.set_valid_max(256);
                    f.set_slider_min(1);
                    f.set_slider_max(256);
                    f.set_default(8);
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
                out_data.set_return_msg("fs-rs randommosaic2nd");
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
        // 元の GetParams: GetFIXED してから 100 で割る (整数の割り算)。
        let value =
            libs::utils::flt2fix(params.get(Params::Value)?.as_float_slider()?.value()) as i64 / 100;
        let strength = params.get(Params::Strength)?.as_slider()?.value() as i64;
        let ds = in_data.downsample_x();
        // 元の downScale: 0 はそのまま、他は +0.5 で丸め。
        let down = |v: i64| -> i64 {
            if v == 0 {
                0
            } else {
                (v as f64 * ds.num as f64 / ds.den as f64 + 0.5) as i64
            }
        };
        let size_x = down(params.get(Params::SizeX)?.as_slider()?.value() as i64);
        let size_y = down(params.get(Params::SizeY)?.as_slider()?.value() as i64);

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

        if value <= 0 || size_x <= 0 || size_y <= 0 {
            return Ok(());
        }

        // 元: 種はフレーム番号。
        let (time, step) = (in_data.current_time(), in_data.time_step());
        let frame = if step != 0 { (time / step) as u32 } else { 0 };
        let mut rng = XorShift::new(frame);

        // 元: 8bit はそのまま、16/32bit は 8bit から 16bit に直した値。
        let pr_value = if depth == 8 {
            strength
        } else {
            // 元の FS_CONVERT8TO16: (v * 32768 + 128) / 255。
            (strength * MAX_CHANNEL16 as i64 + 128) / 255
        };
        let k = if depth == 32 {
            MAX_CHANNEL16 as f64
        } else {
            1.0
        };

        // 元: 桝の数は切り上げ。
        let (iw, ih) = (w as i64, h as i64);
        let mut cnt_x = iw / size_x;
        if iw % size_x != 0 {
            cnt_x += 1;
        }
        let mut cnt_y = ih / size_y;
        if ih % size_y != 0 {
            cnt_y += 1;
        }

        for y in 0..cnt_y {
            for x in 0..cnt_x {
                // 元: FM_RAND(1<<16) <= 量 ならつぶす。
                if rng.fm_rand(1 << 16) <= value {
                    let r = rng.rand2(-pr_value, pr_value);
                    let o = r as f64 / k;
                    flat(
                        &out_layer,
                        depth,
                        x * size_x,
                        y * size_y,
                        size_x,
                        size_y,
                        [o, o, o, o],
                    );
                } else {
                    // 元: つぶさなくても1回引いて進める。
                    rng.next();
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
    fn the_block_count_rounds_up() {
        let count = |n: i64, s: i64| {
            let mut c = n / s;
            if n % s != 0 {
                c += 1;
            }
            c
        };
        assert_eq!(count(256, 8), 32);
        assert_eq!(count(144, 8), 18);
        // 割り切れないときは1つ増える (最後の桝はみ出す)。
        assert_eq!(count(144, 10), 15);
        assert_eq!(count(10, 100), 1);
    }

    #[test]
    fn the_percentage_goes_through_an_integer_division() {
        // 元: GetFIXED してから /100。100% なら 65536 で、
        // FM_RAND(1<<16) は 0..65536 なので必ずつぶれる。
        assert_eq!(libs::utils::flt2fix(100.0) as i64 / 100, 65536);
        assert_eq!(libs::utils::flt2fix(50.0) as i64 / 100, 32768);
        assert_eq!(libs::utils::flt2fix(0.0) as i64 / 100, 0);
    }

    #[test]
    fn skipping_the_unused_draw_keeps_the_sequence_aligned() {
        // つぶす桝とつぶさない桝で、進む回数が違うと並びがずれる。
        // 元はつぶさないときも1回引くので、どちらでも2回ぶん進む。
        let mut a = XorShift::new(1);
        let mut b = XorShift::new(1);
        let _ = a.fm_rand(1 << 16);
        let _ = a.rand2(-10, 10);
        let _ = b.fm_rand(1 << 16);
        b.next();
        assert_eq!(a.fm_rand(1 << 16), b.fm_rand(1 << 16));
    }
}
