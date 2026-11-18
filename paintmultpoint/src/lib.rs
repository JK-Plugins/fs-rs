//! F's PaintMultPoint の移植。
//!
//! `Paint` の多点版で、位置と色の組を8つまで置いて順に塗りつぶす。
//! 塗りつぶし本体は `libs::paint` (`Paint` と共有)。
//!
//! 手順 (元の Exec):
//!
//! 1. 入力をそのままコピーする
//! 2. チェックの入っている組を**上から詰めた順**に、位置の画素から
//!    地続きで同じ色のところを指定色で塗る
//! 3. 「ガイドをすべて非表示にする」が入っていなければ、組ごとの
//!    `ガイド表示` が入っているところに**色を反転した**十字を引く
//!    (太さ 2、画面いっぱい)
//!
//! 元から写した細かいところ:
//!
//! - 塗る順番は組の番号順。あとの組が前の塗りの上に乗る
//! - 位置は `PF_FixedPoint` を 16 ビット右シフトしたもの
//! - ガイドの色は `255 - 各チャンネル` でアルファは最大。十字は
//!   `XLine` / `YLine` と同じく下・右へ太る
//! - 地続きの判定は RGB だけ
//! - 色のアルファはパラメータのまま (`Paint` と違って不透明度の
//!   スライダーが無い)
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **classic の `Render` が `Exec` を呼んでいない**。しかも `Paint` と
//!    違って入力のコピーもしないので、classic 経路では**出力に一度も
//!    書かずに返る** (AEXCompat も「書かれていない画素がある」と弾く)。
//!    classic でも塗るようにした
//! 2. 元の塗りつぶしはワールドが漏れる (`libs::paint` の注)。移植では
//!    持ち方そのものが無い
//!
//! なお**出荷された .aex の matchName は `F's PaintMalutPaint`** という
//! 打ち間違いになっている。既存の .aep から値を引き継ぐため、そのまま
//! 合わせてある。
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/PaintMultPoint

use after_effects::{self as ae};
use libs::paint::{bucket, Px};

const COUNT: usize = 8;

/// 1点ぶんの設定。元の PointColorInfo。
#[derive(Debug, Clone, Copy)]
struct Point {
    x: i64,
    y: i64,
    colour: ae::Pixel8,
    guide: bool,
}

/// 元の XLine / YLine (太さ 2 固定)。画面の縁で切る。
fn cross(data: &mut [Px], w: i64, h: i64, x: i64, y: i64, col: Px) {
    let thickness = 2;
    for j in 0..thickness {
        let yy = y + j;
        if yy >= 0 && yy < h {
            for xx in 0..w {
                data[(xx + yy * w) as usize] = col;
            }
        }
    }
    for j in 0..thickness {
        let xx = x + j;
        if xx >= 0 && xx < w {
            for yy in 0..h {
                data[(xx + yy * w) as usize] = col;
            }
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Topic0,
    Cb0,
    Pos0,
    Col0,
    Gid0,
    TopicEnd0,
    Topic1,
    Cb1,
    Pos1,
    Col1,
    Gid1,
    TopicEnd1,
    Topic2,
    Cb2,
    Pos2,
    Col2,
    Gid2,
    TopicEnd2,
    Topic3,
    Cb3,
    Pos3,
    Col3,
    Gid3,
    TopicEnd3,
    Topic4,
    Cb4,
    Pos4,
    Col4,
    Gid4,
    TopicEnd4,
    Topic5,
    Cb5,
    Pos5,
    Col5,
    Gid5,
    TopicEnd5,
    Topic6,
    Cb6,
    Pos6,
    Col6,
    Gid6,
    TopicEnd6,
    Topic7,
    Cb7,
    Pos7,
    Col7,
    Gid7,
    TopicEnd7,
    GuideHideAll,
}

/// 組ごとのスロット (topic, cb, pos, col, gid, topic_end)。
const GROUPS: [(Params, Params, Params, Params, Params, Params); COUNT] = [
    (
        Params::Topic0,
        Params::Cb0,
        Params::Pos0,
        Params::Col0,
        Params::Gid0,
        Params::TopicEnd0,
    ),
    (
        Params::Topic1,
        Params::Cb1,
        Params::Pos1,
        Params::Col1,
        Params::Gid1,
        Params::TopicEnd1,
    ),
    (
        Params::Topic2,
        Params::Cb2,
        Params::Pos2,
        Params::Col2,
        Params::Gid2,
        Params::TopicEnd2,
    ),
    (
        Params::Topic3,
        Params::Cb3,
        Params::Pos3,
        Params::Col3,
        Params::Gid3,
        Params::TopicEnd3,
    ),
    (
        Params::Topic4,
        Params::Cb4,
        Params::Pos4,
        Params::Col4,
        Params::Gid4,
        Params::TopicEnd4,
    ),
    (
        Params::Topic5,
        Params::Cb5,
        Params::Pos5,
        Params::Col5,
        Params::Gid5,
        Params::TopicEnd5,
    ),
    (
        Params::Topic6,
        Params::Cb6,
        Params::Pos6,
        Params::Col6,
        Params::Gid6,
        Params::TopicEnd6,
    ),
    (
        Params::Topic7,
        Params::Cb7,
        Params::Pos7,
        Params::Col7,
        Params::Gid7,
        Params::TopicEnd7,
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
        // 元の既定色: 赤・緑・青・黄・マゼンタ・水色・暗い赤・暗い緑。
        let cols = [
            (0xFFu8, 0x00u8, 0x00u8),
            (0x00, 0xFF, 0x00),
            (0x00, 0x00, 0xFF),
            (0xFF, 0xFF, 0x00),
            (0xFF, 0x00, 0xFF),
            (0x00, 0xFF, 0xFF),
            (0x7F, 0x00, 0x00),
            (0x00, 0x7F, 0x00),
        ];
        for (i, g) in GROUPS.iter().enumerate() {
            params.add_group(g.0, g.5, &format!("Point_{i}"), true, |p| {
                // 元: PF_ADD_CHECKBOX("塗りつぶす_%d", "ON", FALSE)。
                p.add(
                    g.1,
                    &format!("Paint_{i}"),
                    ae::CheckBoxDef::setup(|f| {
                        f.set_label("ON");
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_POINT("位置_%d", 5+10*i, 5+10*i)。
                let v = (5 + 10 * i) as f32;
                p.add(
                    g.2,
                    &format!("Position_{i}"),
                    ae::PointDef::setup(move |f| {
                        f.set_default((v, v));
                        f.set_value(f.default());
                    }),
                )?;
                let (r, gr, b) = cols[i];
                p.add(
                    g.3,
                    &format!("Paint_Color_{i}"),
                    ae::ColorDef::setup(move |f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 0xFF,
                            red: r,
                            green: gr,
                            blue: b,
                        });
                        f.set_value(f.default());
                    }),
                )?;
                // 元: PF_ADD_CHECKBOX("ガイド表示_%d", "ON", FALSE)。
                p.add(
                    g.4,
                    &format!("Guide_{i}"),
                    ae::CheckBoxDef::setup(|f| {
                        f.set_label("ON");
                        f.set_default(false);
                        f.set_value(f.default());
                    }),
                )?;
                Ok(())
            })?;
        }
        // 元: PF_ADD_CHECKBOX("ガイドをすべて非表示にする", "非表示", FALSE)。
        params.add(
            Params::GuideHideAll,
            "Hide_all_guides",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("hide");
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
                out_data.set_return_msg("fs-rs paintmultpoint");
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
        // 元の GetParams: チェックの入っている組を上から詰める。
        let mut pts: Vec<Point> = Vec::with_capacity(COUNT);
        for g in GROUPS.iter() {
            if params.get(g.1)?.as_checkbox()?.value() {
                let (px, py) = params.get(g.2)?.as_point()?.value();
                pts.push(Point {
                    x: (libs::utils::flt2fix(px as f64) as i64) >> 16,
                    y: (libs::utils::flt2fix(py as f64) as i64) >> 16,
                    colour: params.get(g.3)?.as_color()?.value(),
                    guide: params.get(g.4)?.as_checkbox()?.value(),
                });
            }
        }
        let hide_all = params.get(Params::GuideHideAll)?.as_checkbox()?.value();

        let (w, h) = (out_layer.width() as i64, out_layer.height() as i64);
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if pts.is_empty() || w == 0 || h == 0 {
            return Ok(());
        }

        macro_rules! go {
            ($wr:ident, $conv:expr, $get:expr, $put:expr) => {{
                let mut data: Vec<Px> = Vec::with_capacity((w * h) as usize);
                for yy in 0..h {
                    for xx in 0..w {
                        data.push($get(out_layer.$wr(xx as usize, yy as usize)));
                    }
                }
                for p in &pts {
                    bucket(&mut data, w, h, p.x, p.y, $conv(&p.colour));
                }
                if !hide_all {
                    for p in &pts {
                        if p.guide {
                            // 元: 色を反転してアルファは最大。
                            let inv = ae::Pixel8 {
                                alpha: 255,
                                red: 255 - p.colour.red,
                                green: 255 - p.colour.green,
                                blue: 255 - p.colour.blue,
                            };
                            cross(&mut data, w, h, p.x, p.y, $conv(&inv));
                        }
                    }
                }
                for yy in 0..h {
                    for xx in 0..w {
                        $put(
                            out_layer.$wr(xx as usize, yy as usize),
                            &data[(xx + yy * w) as usize],
                        );
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8_mut,
                |c: &ae::Pixel8| Px {
                    a: c.alpha as f64,
                    r: c.red as f64,
                    g: c.green as f64,
                    b: c.blue as f64
                },
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
                |c: &ae::Pixel8| {
                    let p = libs::utils::conv_8_to_16(c);
                    Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64,
                    }
                },
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
                |c: &ae::Pixel8| {
                    let p = libs::utils::conv_8_to_32(c);
                    Px {
                        a: p.alpha as f64,
                        r: p.red as f64,
                        g: p.green as f64,
                        b: p.blue as f64,
                    }
                },
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

    fn data(w: i64, h: i64) -> Vec<Px> {
        vec![
            Px {
                a: 255.0,
                r: 255.0,
                g: 255.0,
                b: 255.0,
            };
            (w * h) as usize
        ]
    }

    /// 十字は画面いっぱいで、太さ 2。
    #[test]
    fn the_guide_cross_spans_the_screen() {
        let (w, h) = (8i64, 6i64);
        let mut d = data(w, h);
        let red = Px {
            a: 255.0,
            r: 255.0,
            g: 0.0,
            b: 0.0,
        };
        cross(&mut d, w, h, 3, 2, red);
        // 横線は y = 2, 3。
        for x in 0..w {
            assert_eq!(d[(x + 2 * w) as usize], red, "x = {x}");
            assert_eq!(d[(x + 3 * w) as usize], red, "x = {x}");
        }
        // 縦線は x = 3, 4。
        for y in 0..h {
            assert_eq!(d[(3 + y * w) as usize], red, "y = {y}");
            assert_eq!(d[(4 + y * w) as usize], red, "y = {y}");
        }
        // それ以外は白のまま。
        assert_eq!(d[0].r, 255.0);
        assert_eq!(d[0].g, 255.0);
    }

    /// 画面の外に出るぶんは切る。
    #[test]
    fn the_guide_cross_is_clipped() {
        let (w, h) = (4i64, 4i64);
        let mut d = data(w, h);
        let red = Px {
            a: 255.0,
            r: 255.0,
            g: 0.0,
            b: 0.0,
        };
        cross(&mut d, w, h, 3, 3, red);
        // x = 4 と y = 4 は無いので落ちないこと。
        assert_eq!(d[(3 + 3 * w) as usize], red);
    }
}
