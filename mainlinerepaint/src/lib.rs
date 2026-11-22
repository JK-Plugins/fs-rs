//! F's MainLineRepaint の移植。
//!
//! セル画の「主線」(指定色で引かれた線) を消して、線の両側の塗りで
//! 埋め直す。横一列ずつ・縦一列ずつ見て、主線の画素から左右 (上下) に
//! 最大3つまで探し、見つかった塗りのうち**暗いほう**の色を持ってくる。
//!
//! 線の太さ1本ぶんでは埋まりきらないので、横→縦を1巡として最大10巡
//! くり返す。決められなかった画素の数が減らなくなったら止める。
//!
//! 元から写した細かいところ:
//!
//! - 画素の見分けは `pV` が返す1つの数。0..255 が明るさ (ふつうの塗り)、
//!   256 が透明 (`PPTRANS`)、257 が主線 (`PPMAIN`)。アルファ 0 は
//!   色を見ずに透明、次に主線の色と `許容値` 以内なら主線、それ以外は
//!   明るさ (`0.29891 R + 0.58661 G + 0.11448 B`)
//! - 主線かどうかの判定は 8bit に落としてから。明るさはその深度の値から
//!   直に (16bit は `/PF_MAX_CHAN16`、32bit は 1.0 で頭を抑えるだけで
//!   下は抑えない)
//! - 左右で見つかったものの組で決める。塗り/塗りは暗いほう、
//!   主線/塗り・透明/塗りはその塗り、塗り/主線・塗り/透明はその塗り、
//!   透明/透明は完全な透明。**主線/主線・主線/透明・透明/主線は
//!   決められない**として次の巡に回す
//! - 探す向きの条件は「主線でないもの」。透明も止まる先になる
//! - `許容値` は `255 * % / 100` を切り捨てたもの
//! - 退避はその線を**見る前**の値。同じ線の中で書いた色が次の判定に
//!   回り込むことはない
//! - 縦のパスは、横のパスで決められなかった画素が1つでもあったときだけ
//!   走る
//!
//! 元と違えたところ (どちらも端1列ぶんの写し間違い):
//!
//! - **右端の列が貰う色が自分自身だった**。左端は `scanline[1]` から
//!   貰うのに、右端は `data[... + w - 1] = scanline[w - 1]` で、退避した
//!   自分の色をそのまま書き戻している。つまり右端の主線は消えない。
//!   `scanline[w - 2]` にした
//! - **縦のパスの下端が、線の長さではなく幅の番号で見ていた**。
//!   `scanlineV[w - 1]` / `scanlineV[w - 2]` / `scanline[w - 1]` と
//!   書いてあり、縦の線は `h` 個しかないので、幅が高さより大きいと
//!   **直前の横のパスが退避した値を読んで**、下端の行にその色を書く。
//!   `h - 1` / `h - 2` にした
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MainLineRepaint

use after_effects::{self as ae};
use libs::utils::{comp_pix8_lv, conv_16_to_8, conv_32_to_8};

/// 透明にするときの画素。元の `dst = {0,0,0,0}`。
const ZERO8: ae::Pixel8 = ae::Pixel8 {
    alpha: 0,
    red: 0,
    green: 0,
    blue: 0,
};
const ZERO16: ae::Pixel16 = ae::Pixel16 {
    alpha: 0,
    red: 0,
    green: 0,
    blue: 0,
};
const ZERO32: ae::PixelF32 = ae::PixelF32 {
    alpha: 0.0,
    red: 0.0,
    green: 0.0,
    blue: 0.0,
};

/// 元の PPTRANS / PPMAIN。明るさ 0..255 の上に積んだ目印。
const PPTRANS: i32 = 256;
const PPMAIN: i32 = 257;

/// 元の PS_NORMAL / PS_TRANS / PS_MAIN。
const PS_NORMAL: i32 = 0;
const PS_TRANS: i32 = 1;
const PS_MAIN: i32 = 2;

/// 元の GetParams で固定されている値。
const SCAN_COUNT: usize = 10;
const SCAN_LENGTH: usize = 4;

/// 元の PxStatus。
fn px_status(v: i32) -> i32 {
    if v == PPMAIN {
        PS_MAIN
    } else if v == PPTRANS {
        PS_TRANS
    } else {
        PS_NORMAL
    }
}

/// この線のこの画素をどうするか。元はどの枝も「同じ線の別の画素の色を
/// 持ってくる」か「完全な透明にする」かのどちらかなので、深度に関係なく
/// 番号だけで表せる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Act {
    /// その番号の退避した色を貰う。
    Copy(usize),
    /// 元の `dst = {0,0,0,0}`。
    Clear,
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    MainColor,
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
        // 元: PF_ADD_COLOR("主線の色", 0, 0, 0)
        params.add(
            Params::MainColor,
            "Main_Color",
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
        // 元: PF_ADD_FLOAT_SLIDER("許容値(%)", 0, 100, 0, 20, 1, 0, 1, 0, 0, ..)
        params.add(
            Params::Level,
            "Tolerance(%)",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(20.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
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
                out_data.set_return_msg("fs-rs mainlinerepaint");
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
        let main = params.get(Params::MainColor)?.as_color()?.value();
        // 元: lv = (A_u_char)(PF_MAX_CHAN8 * % / 100)。切り捨て。
        let level = params.get(Params::Level)?.as_float_slider()?.value();
        let lv = (ae::MAX_CHANNEL8 as f64 * level / 100.0) as u8;

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をそのままコピーしてから、その上で消していく。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if w < 2 || h < 2 {
            return Ok(());
        }

        // 元の pV8/16/32。深度ごとに明るさの出し方が違う。
        let pv = |x: usize, y: usize| -> i32 {
            let (p8, luma) = match depth {
                8 => {
                    let p = *out_layer.as_pixel8(x, y);
                    if p.alpha == 0 {
                        return PPTRANS;
                    }
                    let m = ae::MAX_CHANNEL8 as f64;
                    (
                        p,
                        0.29891 * (p.red as f64 / m)
                            + 0.58661 * (p.green as f64 / m)
                            + 0.11448 * (p.blue as f64 / m),
                    )
                }
                16 => {
                    let p = out_layer.as_pixel16(x, y);
                    if p.alpha == 0 {
                        return PPTRANS;
                    }
                    let m = ae::MAX_CHANNEL16 as f64;
                    (
                        conv_16_to_8(p),
                        0.29891 * (p.red as f64 / m)
                            + 0.58661 * (p.green as f64 / m)
                            + 0.11448 * (p.blue as f64 / m),
                    )
                }
                _ => {
                    let p = out_layer.as_pixel32(x, y);
                    if p.alpha == 0.0 {
                        return PPTRANS;
                    }
                    // 元: 1.0 で頭だけ抑える (下は抑えない)。
                    let c = |v: f32| -> f64 {
                        let v = v as f64;
                        if v > 1.0 {
                            1.0
                        } else {
                            v
                        }
                    };
                    (
                        conv_32_to_8(p),
                        0.29891 * c(p.red) + 0.58661 * c(p.green) + 0.11448 * c(p.blue),
                    )
                }
            };
            if comp_pix8_lv(&p8, &main, lv) {
                return PPMAIN;
            }
            let ret = (luma * ae::MAX_CHANNEL8 as f64 + 0.5) as i32;
            ret.clamp(0, ae::MAX_CHANNEL8 as i32)
        };

        // 元: 退避用に max(w, h) * 2 の幅のワールドを取る。縦のパスの端が
        // 幅のほうの番号を読むので、長さを持ち回す配列にしてある。
        let span = w.max(h) * 2;
        let mut snap8 = vec![ZERO8; if depth == 8 { span } else { 0 }];
        let mut snap16 = vec![ZERO16; if depth == 16 { span } else { 0 }];
        let mut snap32 = vec![ZERO32; if depth == 32 { span } else { 0 }];
        let mut v = vec![0i32; span];
        let mut acts: Vec<(usize, Act)> = Vec::with_capacity(span);

        // 元の Exec を囲むループ。決められなかった数が減らなくなったら止める。
        let mut count: i64 = 0;
        let mut prev: i64 = -1;
        for _ in 0..SCAN_COUNT {
            // --- 横のパス ---
            let mut cnt = 0i64;
            for y in 0..h {
                for x in 0..w {
                    match depth {
                        8 => snap8[x] = *out_layer.as_pixel8(x, y),
                        16 => snap16[x] = *out_layer.as_pixel16(x, y),
                        _ => snap32[x] = *out_layer.as_pixel32(x, y),
                    }
                    v[x] = pv(x, y);
                }
                cnt += decide(&v, w, w - 1, &mut acts);
                for (i, act) in acts.iter() {
                    match depth {
                        8 => {
                            *out_layer.as_pixel8_mut(*i, y) = match act {
                                Act::Copy(s) => snap8[*s],
                                Act::Clear => ZERO8,
                            }
                        }
                        16 => {
                            *out_layer.as_pixel16_mut(*i, y) = match act {
                                Act::Copy(s) => snap16[*s],
                                Act::Clear => ZERO16,
                            }
                        }
                        _ => {
                            *out_layer.as_pixel32_mut(*i, y) = match act {
                                Act::Copy(s) => snap32[*s],
                                Act::Clear => ZERO32,
                            }
                        }
                    }
                }
            }
            // 元: 横のパスで決められなかったものが1つもなければここで戻る
            // (count は前の巡の値のまま)。
            if cnt != 0 {
                // --- 縦のパス ---
                cnt = 0;
                for x in 0..w {
                    for y in 0..h {
                        match depth {
                            8 => snap8[y] = *out_layer.as_pixel8(x, y),
                            16 => snap16[y] = *out_layer.as_pixel16(x, y),
                            _ => snap32[y] = *out_layer.as_pixel32(x, y),
                        }
                        v[y] = pv(x, y);
                    }
                    cnt += decide(&v, h, w - 1, &mut acts);
                    for (i, act) in acts.iter() {
                        match depth {
                            8 => {
                                *out_layer.as_pixel8_mut(x, *i) = match act {
                                    Act::Copy(s) => snap8[*s],
                                    Act::Clear => ZERO8,
                                }
                            }
                            16 => {
                                *out_layer.as_pixel16_mut(x, *i) = match act {
                                    Act::Copy(s) => snap16[*s],
                                    Act::Clear => ZERO16,
                                }
                            }
                            _ => {
                                *out_layer.as_pixel32_mut(x, *i) = match act {
                                    Act::Copy(s) => snap32[*s],
                                    Act::Clear => ZERO32,
                                }
                            }
                        }
                    }
                }
                count = cnt;
            }
            if prev == count || count <= 0 {
                break;
            }
            prev = count;
        }
        Ok(())
    }
}

/// 1本の線を見て、どの画素がどこから色を貰うかを決める。返すのは
/// 「決められなかった主線の画素の数」。
///
/// `v` は持ち回しの配列で、この線の分は `0..len` に入っている。
/// `width_hi` は元が端の処理に使っている `w - 1`。縦のパスでも幅のほうを
/// 使うのが元のままの振る舞い (上の説明を参照)。
fn decide(v: &[i32], len: usize, width_hi: usize, acts: &mut Vec<(usize, Act)>) -> i64 {
    acts.clear();
    let mut cnt = 0i64;

    for i in 1..len - 1 {
        if v[i] != PPMAIN {
            continue;
        }
        // 元: 現在の位置を初期値に。見つからなければ自分 (= 主線) のまま。
        let mut lc = i;
        let mut rc = i;
        for k in 1..SCAN_LENGTH {
            let Some(k2) = i.checked_sub(k) else { break };
            // 元: `<= PPTRANS`、つまり主線でないもの。透明も止まる先になる。
            if v[k2] <= PPTRANS {
                lc = k2;
                break;
            }
        }
        for k in 1..SCAN_LENGTH {
            let k2 = i + k;
            if k2 >= len {
                break;
            }
            if v[k2] <= PPTRANS {
                rc = k2;
                break;
            }
        }
        let pat = (px_status(v[lc]) << 8) | px_status(v[rc]);
        let act = if pat == (PS_NORMAL << 8) | PS_NORMAL {
            // 元: 明るさが小さいほう (暗いほう) を採る。同じなら左。
            Act::Copy(if v[lc] <= v[rc] { lc } else { rc })
        } else if pat == (PS_MAIN << 8) | PS_NORMAL || pat == (PS_TRANS << 8) | PS_NORMAL {
            Act::Copy(rc)
        } else if pat == (PS_NORMAL << 8) | PS_MAIN || pat == (PS_NORMAL << 8) | PS_TRANS {
            Act::Copy(lc)
        } else if pat == (PS_TRANS << 8) | PS_TRANS {
            Act::Clear
        } else {
            // 主線/主線・主線/透明・透明/主線は決められない。次の巡に回す。
            cnt += 1;
            continue;
        };
        acts.push((i, act));
    }

    // 端の1列は上の輪に入らないので別に見る。隣が主線でなければ貰う。
    if v[0] >= PPMAIN {
        if v[1] < PPMAIN {
            acts.push((0, Act::Copy(1)));
        } else {
            cnt += 1;
        }
    }
        // 直したところ: 端の番号はこの線の長さで見て、隣から貰う。
        // (元は縦のパスでも幅のほうの番号を使っていた。上の説明を参照)
        let _ = width_hi;
        let cond = len - 1;
        let src = cond - 1;
    if v[cond] >= PPMAIN {
        if v[cond - 1] < PPMAIN {
            acts.push((len - 1, Act::Copy(src)));
        } else {
            cnt += 1;
        }
    }
    cnt
}

#[cfg(test)]
mod tests {
    use super::*;

    const N: i32 = 100; // ふつうの塗り (明るさ 100)
    const D: i32 = 20; // 暗い塗り
    const M: i32 = PPMAIN;
    const T: i32 = PPTRANS;

    fn run(v: &[i32]) -> Vec<(usize, Act)> {
        let mut acts = Vec::new();
        decide(v, v.len(), v.len() - 1, &mut acts);
        acts.sort_by_key(|(i, _)| *i);
        acts
    }

    #[test]
    fn the_status_only_knows_three_kinds() {
        assert_eq!(px_status(0), PS_NORMAL);
        assert_eq!(px_status(255), PS_NORMAL);
        assert_eq!(px_status(PPTRANS), PS_TRANS);
        assert_eq!(px_status(PPMAIN), PS_MAIN);
    }

    #[test]
    fn a_line_between_two_fills_takes_the_darker_side() {
        // 塗り/塗りは暗いほう。
        assert_eq!(run(&[N, N, M, D, N]), vec![(2, Act::Copy(3))]);
        assert_eq!(run(&[N, D, M, N, N]), vec![(2, Act::Copy(1))]);
        // 同じ明るさなら左。
        assert_eq!(run(&[N, N, M, N, N]), vec![(2, Act::Copy(1))]);
    }

    #[test]
    fn a_line_against_transparency_takes_the_fill() {
        assert_eq!(run(&[N, T, M, N, N]), vec![(2, Act::Copy(3))]);
        assert_eq!(run(&[N, N, M, T, N]), vec![(2, Act::Copy(1))]);
        // 両側が透明なら完全な透明。
        assert_eq!(run(&[N, T, M, T, N]), vec![(2, Act::Clear)]);
    }

    #[test]
    fn a_thick_line_is_left_for_the_next_round() {
        // 主線が探す長さ (3つ) より太いと、中の画素は決められない。
        let v = [N, M, M, M, M, M, M, M, N];
        let mut acts = Vec::new();
        let cnt = decide(&v, v.len(), v.len() - 1, &mut acts);
        assert!(cnt > 0, "太い線は次の巡に回る");
        // 両端寄りは隣の塗りが届くので決まる。
        assert!(acts.iter().any(|(i, _)| *i == 1));
        assert!(acts.iter().any(|(i, _)| *i == 7));
    }

    #[test]
    fn the_search_stops_at_the_first_non_line() {
        // 元は最初に見つかった「主線でないもの」で止まる。2つ先に暗い塗りが
        // あっても、1つ先の明るい塗りが勝つ。
        assert_eq!(run(&[D, N, M, N, D]), vec![(2, Act::Copy(1))]);
    }

    #[test]
    fn the_search_reaches_three_pixels() {
        // SCAN_LENGTH は 4 なので k は 1..3、つまり3つ先まで。
        assert_eq!(run(&[N, M, M, M, N, M, M, M, M, N]).len(), 7);
    }

    #[test]
    fn both_ends_are_handled_separately() {
        // 端の列は内側の輪 (1..len-1) に入らない。
        let acts = run(&[M, N, N, N, M]);
        assert_eq!(acts, vec![(0, Act::Copy(1)), (4, Act::Copy(3))]);
    }

    #[test]
    fn the_tolerance_truncates() {
        // 元: (A_u_char)(255 * % / 100)。
        let lv = |pct: f64| (ae::MAX_CHANNEL8 as f64 * pct / 100.0) as u8;
        assert_eq!(lv(0.0), 0);
        assert_eq!(lv(20.0), 51);
        assert_eq!(lv(100.0), 255);
        // 10% は 25.5 なので 25 に落ちる。
        assert_eq!(lv(10.0), 25);
    }
}
