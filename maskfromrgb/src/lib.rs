//! F's MaskFromRGB の移植。
//!
//! RGB から1チャンネル分を抜き出してアルファにし、RGB は白で塗りつぶす。
//! Yellow/Magenta/Cyan は RGB を反転してから対応する原色の式を使う。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MaskFromRGB

use after_effects::{self as ae, MAX_CHANNEL16, MAX_CHANNEL8};

use libs::utils::{round_byte_fp_long, round_short_fp_long};

/// 元の M_MODE。NONE = 0 で RED から始まる。
const RED: i32 = 1;
const GREEN: i32 = 2;
const BLUE: i32 = 3;
const YELLOW: i32 = 4;
const MAGENTA: i32 = 5;
const CYAN: i32 = 6;
/// 元の enum には MAX = 7 もあるが、ポップアップの項目数が 6 なので AE からは
/// 選べない。到達不能な分岐として元のまま残してある。
const MAX_MODE: i32 = 7;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Mask,
    Level,
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
        // 元: PF_ADD_POPUP(STR_POPUP, 6, 1, "Red|Green|Blue|Yellow|Magenta|Cyan|Max", ID_POPUP)
        //
        // 項目文字列は7個あるのに個数が 6 なので、AE は先頭6個しか出さず Max は
        // 選べない。数値範囲 (1..6) を元と合わせる方を優先して 6 個で宣言する。
        params.add(
            Params::Mask,
            "Mask",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Red", "Green", "Blue", "Yellow", "Magenta", "Cyan"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;

        // 元: PF_ADD_FLOAT_SLIDER(STR_LEVEL, 0, 5000, 50, 300, 1, 100, 1, 0, 0, ID_LEVEL)
        params.add(
            Params::Level,
            "Level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(5000.0);
                f.set_slider_min(50.0);
                f.set_slider_max(300.0);
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
                self.about(&mut out_data);
            }
            ae::Command::GlobalSetup => {
                self.global_setup(&in_data)?;
            }
            ae::Command::Render {
                in_layer,
                out_layer,
            } => {
                self.legacy_render(&in_data, in_layer, out_layer, params)?;
            }
            ae::Command::SmartPreRender { extra } => {
                self.smart_pre_render(&in_data, extra, params)?;
            }
            ae::Command::SmartRender { extra } => {
                self.smart_render(&in_data, extra, params)?;
            }
            _ => {}
        }
        Ok(())
    }
}

impl Plugin {
    fn about(&mut self, out_data: &mut OutData) {
        out_data.set_return_msg("fs-rs maskfromrgb");
    }

    fn global_setup(&mut self, in_data: &InData) -> Result<(), ae::Error> {
        win_dbg_logger::DEBUGGER_LOGGER.set_force_log_without_debugger(true);
        log::info!("GlobalSetup");
        // For Premiere - declare supported pixel formats
        if in_data.is_premiere() {
            let suite = ae::pf::suites::PixelFormat::new()?;

            suite.clear_supported_pixel_formats(in_data.effect_ref())?;
            let formats = [
                ae::pr::PixelFormat::Bgra4444_8u,
                ae::pr::PixelFormat::Bgra4444_16u,
                ae::pr::PixelFormat::Bgra4444_32f,
            ];
            for x in formats {
                suite.add_supported_pixel_format(in_data.effect_ref(), x)?;
            }
        }
        Ok(())
    }

    fn legacy_render(
        &mut self,
        in_data: &InData,
        in_layer: ae::Layer,
        out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        if !in_data.is_premiere() {
            // We don't support non-SmartFX unless it's Premiere
            return Err(Error::BadCallbackParameter);
        }

        self.do_render(in_data, in_layer, out_layer, params)?;

        Ok(())
    }

    fn smart_pre_render(
        &mut self,
        in_data: &InData,
        mut extra: ae::PreRenderExtra,
        _params: &mut ae::Parameters<Params>,
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

    fn smart_render(
        &mut self,
        in_data: &InData,
        extra: ae::SmartRenderExtra,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        let cb = extra.callbacks();
        let Some(input_world) = cb.checkout_layer_pixels(0)? else {
            return Ok(());
        };

        if let Ok(Some(output_world)) = cb.checkout_output() {
            self.do_render(in_data, input_world, output_world, params)?;
        }

        cb.checkin_layer_pixels(0)?;
        Ok(())
    }

    fn do_render(
        &self,
        in_data: &ae::InData,
        in_layer: ae::Layer,
        mut out_layer: ae::Layer,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), Error> {
        let mode = params.get(Params::Mask)?.as_popup()?.value();
        // 元の GetParams: Level は /100 される。
        let level = params.get(Params::Level)?.as_float_slider()?.value() as f32 / 100.0;

        // 元の switch。Yellow/Magenta/Cyan は反転済みの RGB に対して
        // 対応する原色と同じ式を使う。
        let pick = move |r: f32, g: f32, b: f32| -> f32 {
            match mode {
                RED | CYAN => r - g.max(b),
                GREEN | MAGENTA => g - b.max(r),
                BLUE | YELLOW => b - r.max(g),
                MAX_MODE => b.max(r.max(g)),
                _ => 0.0,
            }
        };

        let progress_final = out_layer.height() as _;
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        in_layer.iterate_with(
            &mut out_layer,
            0,
            progress_final,
            None,
            |_x: i32,
             _y: i32,
             pixel: ae::GenericPixel,
             out_pixel: ae::GenericPixelMut|
             -> Result<(), Error> {
                match (pixel, out_pixel) {
                    (ae::GenericPixel::Pixel8(_), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        let m = MAX_CHANNEL8 as f32;
                        let (mut r, mut g, mut b) = (
                            out_pixel.red as f32,
                            out_pixel.green as f32,
                            out_pixel.blue as f32,
                        );
                        let a = out_pixel.alpha as f32;
                        let v = if a == 0.0 {
                            0
                        } else {
                            if (YELLOW..MAX_MODE).contains(&mode) {
                                r = round_byte_fp_long(m - r) as f32;
                                g = round_byte_fp_long(m - g) as f32;
                                b = round_byte_fp_long(m - b) as f32;
                            }
                            if a < m {
                                let af = a / m;
                                r = round_byte_fp_long(r * af) as f32;
                                g = round_byte_fp_long(g * af) as f32;
                                b = round_byte_fp_long(b * af) as f32;
                            }
                            // 元は差を一度 RoundByteFpLong で 0..255 に丸めてから
                            // level を掛け、もう一度丸める。
                            let picked = round_byte_fp_long(pick(r, g, b)) as f32;
                            round_byte_fp_long(picked * level)
                        };
                        out_pixel.alpha = v;
                        out_pixel.red = MAX_CHANNEL8 as u8;
                        out_pixel.green = MAX_CHANNEL8 as u8;
                        out_pixel.blue = MAX_CHANNEL8 as u8;
                    }
                    (ae::GenericPixel::Pixel16(_), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        let m = MAX_CHANNEL16 as f32;
                        let (mut r, mut g, mut b) = (
                            out_pixel.red as f32,
                            out_pixel.green as f32,
                            out_pixel.blue as f32,
                        );
                        let a = out_pixel.alpha as f32;
                        let v = if a == 0.0 {
                            0
                        } else {
                            if (YELLOW..MAX_MODE).contains(&mode) {
                                r = round_short_fp_long(m - r) as f32;
                                g = round_short_fp_long(m - g) as f32;
                                b = round_short_fp_long(m - b) as f32;
                            }
                            if a < m {
                                let af = a / m;
                                r = round_short_fp_long(r * af) as f32;
                                g = round_short_fp_long(g * af) as f32;
                                b = round_short_fp_long(b * af) as f32;
                            }
                            let picked = round_short_fp_long(pick(r, g, b)) as f32;
                            round_short_fp_long(picked * level)
                        };
                        out_pixel.alpha = v;
                        out_pixel.red = MAX_CHANNEL16 as u16;
                        out_pixel.green = MAX_CHANNEL16 as u16;
                        out_pixel.blue = MAX_CHANNEL16 as u16;
                    }
                    (ae::GenericPixel::PixelF32(_), ae::GenericPixelMut::PixelF32(out_pixel)) => {
                        let (mut r, mut g, mut b) =
                            (out_pixel.red, out_pixel.green, out_pixel.blue);
                        let a = out_pixel.alpha;
                        let v = if a == 0.0 {
                            0.0
                        } else {
                            if (YELLOW..MAX_MODE).contains(&mode) {
                                r = 1.0 - r;
                                g = 1.0 - g;
                                b = 1.0 - b;
                            }
                            if a < 1.0 {
                                r *= a;
                                g *= a;
                                b *= a;
                            }
                            pick(r, g, b) * level
                        };
                        out_pixel.alpha = v;
                        out_pixel.red = 1.0;
                        out_pixel.green = 1.0;
                        out_pixel.blue = 1.0;
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;
        Ok(())
    }
}
