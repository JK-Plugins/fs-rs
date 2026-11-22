//! F's grayToCountourLine の移植。
//!
//! 画素の輝度を段階数だけの灰色に丸める (等高線のような縞を作る)。
//! 段階数は暗部 (0..84) / 中間 (85..169) / 明部 (170..255) で別々に指定する。
//! DrawCol を入れると、使った灰色の一覧を画面左上に 10 個ずつ並べて描く。
//!
//! 16/32bit も元と同じく画素を 8bit に落としてから引き、結果を戻すので、
//! 出力は 256 段。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/grayToCountourLine

use after_effects::{self as ae, MAX_CHANNEL8};

use libs::utils::{conv_16_to_8, conv_32_to_8, conv_8_to_16, conv_8_to_32};

/// 元の GBDR。暗部・中間・明部の境界。
const ZERO: usize = 0;
const LO: usize = 85;
const HI: usize = 170;
const MAX: usize = 256;

/// 元の col[30]。段階ごとの灰色を作る。
fn make_colors(total: i32) -> [u8; 30] {
    let mut col = [MAX_CHANNEL8 as u8; 30];
    for i in 0..(total as usize).min(30) {
        // 元: 255*i/total + 0.5 を A_u_char にキャスト (切り捨て)。
        let mut b = MAX_CHANNEL8 as f64 * i as f64 / total as f64 + 0.5;
        if b > MAX_CHANNEL8 as f64 {
            b = MAX_CHANNEL8 as f64;
        }
        col[i] = b as u8;
    }
    col
}

/// 元の GetParams のテーブル作成。3区間それぞれを borders[i] 段に割る。
fn make_table(borders: [i32; 3]) -> [u8; MAX] {
    let total: i32 = borders.iter().sum();
    let col = make_colors(total);
    let mut tbl = [0u8; MAX];
    let mut v: usize = 0;

    for (sec, &(from, to)) in [(ZERO, LO), (LO, HI), (HI, MAX)].iter().enumerate() {
        let n = borders[sec];
        if n <= 1 {
            for i in from..to {
                tbl[i] = col[v.min(29)];
            }
        } else {
            // 元: bdrValue は整数除算。
            let step = ((to - from) as i32 / n).max(1);
            let mut cnt = 0;
            let mut cnt_max = 0;
            for i in from..to {
                tbl[i] = col[v.min(29)];
                cnt += 1;
                if cnt >= step {
                    cnt_max += 1;
                    if cnt_max < n {
                        v += 1;
                    }
                    cnt = 0;
                }
            }
        }
        // 元は区間ごとに最後に v++ する。
        v += 1;
    }
    tbl
}

/// 元の輝度計算。アルファで減光したあと 255 で上だけクランプする。
fn luma(p: &ae::Pixel8) -> usize {
    let mut v = (0.29891 * p.red as f64) + (0.58661 * p.green as f64) + (0.11448 * p.blue as f64);
    if p.alpha < MAX_CHANNEL8 as u8 {
        v *= p.alpha as f64 / MAX_CHANNEL8 as f64;
    }
    if v > MAX_CHANNEL8 as f64 {
        v = MAX_CHANNEL8 as f64;
    }
    // 元は (A_long)v で切り捨て。
    v as usize
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    CountLo,
    CountMid,
    CountHi,
    DrawCol,
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
        // 元: PF_ADD_SLIDER(STR_LO/MID/HI, 1, 10, 1, 10, 1, ..)
        for (p, name) in [
            (Params::CountLo, "Count_Lo"),
            (Params::CountMid, "Count_Mid"),
            (Params::CountHi, "Count_Hi"),
        ] {
            params.add(
                p,
                name,
                ae::SliderDef::setup(|f| {
                    f.set_valid_min(1);
                    f.set_valid_max(10);
                    f.set_slider_min(1);
                    f.set_slider_max(10);
                    f.set_default(1);
                    f.set_value(f.default());
                }),
            )?;
        }

        // 元: PF_ADD_CHECKBOX(STR_DRAW_COL, STR_ON, FALSE, 0, ID_DRAW_COL)
        params.add(
            Params::DrawCol,
            "DrawCol",
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
        out_data.set_return_msg("fs-rs graytocountourline");
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
        let borders = [
            params.get(Params::CountLo)?.as_slider()?.value(),
            params.get(Params::CountMid)?.as_slider()?.value(),
            params.get(Params::CountHi)?.as_slider()?.value(),
        ];
        let draw_col = params.get(Params::DrawCol)?.as_checkbox()?.value();
        let tbl = make_table(borders);
        let total: i32 = borders.iter().sum();

        // 元の Exec は CopyInToOut をコメントアウトしていて、画素関数が
        // 入力 (inP) から全チャンネルを書くのでコピーは要らない。ここでは
        // iterate_with が入力を渡すので同じになる。

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
                    (ae::GenericPixel::Pixel8(pixel), ae::GenericPixelMut::Pixel8(out_pixel)) => {
                        let v = tbl[luma(pixel)];
                        out_pixel.alpha = MAX_CHANNEL8 as u8;
                        out_pixel.red = v;
                        out_pixel.green = v;
                        out_pixel.blue = v;
                    }
                    (ae::GenericPixel::Pixel16(pixel), ae::GenericPixelMut::Pixel16(out_pixel)) => {
                        // 元は CONV16TO8 -> 引く -> CONV8TO16 と往復する。
                        let mut c = conv_16_to_8(pixel);
                        let v = tbl[luma(&c)];
                        c.alpha = MAX_CHANNEL8 as u8;
                        c.red = v;
                        c.green = v;
                        c.blue = v;
                        *out_pixel = conv_8_to_16(&c);
                    }
                    (
                        ae::GenericPixel::PixelF32(pixel),
                        ae::GenericPixelMut::PixelF32(out_pixel),
                    ) => {
                        let mut c = conv_32_to_8(pixel);
                        let v = tbl[luma(&c)];
                        c.alpha = MAX_CHANNEL8 as u8;
                        c.red = v;
                        c.green = v;
                        c.blue = v;
                        *out_pixel = conv_8_to_32(&c);
                    }
                    _ => return Err(Error::BadCallbackParameter),
                }

                Ok(())
            },
        )?;

        if draw_col && total > 0 {
            // 元: colWidth / colHeight は出力の幅・高さの 1/10。
            let cw = (out_layer.width() / 10) as i64;
            let ch = (out_layer.height() / 10) as i64;
            let col = make_colors(total);
            let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
            let depth = out_layer.bit_depth();
            let (mut gx, mut gy) = (0i64, 0i64);
            for i in 0..(total as usize).min(30) {
                let x0 = gx * cw;
                let y0 = gy * ch;
                // 元の boxFill8 は x1/y1 も塗る (閉区間) ので 1 画素はみ出す。
                let x1 = x0 + cw;
                let y1 = y0 + ch;
                if x0 < w && y0 < h && x1 >= 0 && y1 >= 0 {
                    let g = col[i];
                    for y in y0.max(0)..=y1.min(h - 1) {
                        for x in x0.max(0)..=x1.min(w - 1) {
                            let (x, y) = (x as usize, y as usize);
                            match depth {
                                8 => {
                                    let p = out_layer.as_pixel8_mut(x, y);
                                    *p = ae::Pixel8 {
                                        alpha: MAX_CHANNEL8 as u8,
                                        red: g,
                                        green: g,
                                        blue: g,
                                    };
                                }
                                16 => {
                                    let c = ae::Pixel8 {
                                        alpha: MAX_CHANNEL8 as u8,
                                        red: g,
                                        green: g,
                                        blue: g,
                                    };
                                    *out_layer.as_pixel16_mut(x, y) = conv_8_to_16(&c);
                                }
                                _ => {
                                    let c = ae::Pixel8 {
                                        alpha: MAX_CHANNEL8 as u8,
                                        red: g,
                                        green: g,
                                        blue: g,
                                    };
                                    *out_layer.as_pixel32_mut(x, y) = conv_8_to_32(&c);
                                }
                            }
                        }
                    }
                }
                gx += 1;
                if gx >= 10 {
                    gx = 0;
                    gy += 1;
                }
            }
        }

        Ok(())
    }
}
