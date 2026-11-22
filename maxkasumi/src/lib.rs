//! F's Max_Kasumi の移植。
//!
//! 隣の画素が自分より明るければ、その明るさに**少しだけ**寄せる。それを
//! 指定回数くりかえすので、明るいところが霞のようににじんで広がる
//! (名前の「かすみ」)。寄せ方は
//!
//! ```text
//! p = 隣 / 4;   自分 = 自分 + p - 自分 * p / 最大値;   隣を超えない
//! ```
//!
//! で、暗いところでは大きく、明るいところではほとんど動かない。
//!
//! 手順 (元の Exec8/16/32):
//!
//! 1. 入力をそのままコピーする
//! 2. 回数に downsample を掛ける (整数割り算)。0 なら何もしない
//! 3. **乗算済みにする** (`toBlackMat`)
//! 4. 回数が負なら全チャンネルを反転する (明暗を入れ替えて収縮にする)
//! 5. RGB を対象にするなら、回数ぶん「横 → 縦」を回す。走査の向きは
//!    ScanLine の指定で横だけ・縦だけにもできる
//! 6. アルファを対象にするなら、同じことをアルファについて回す
//! 7. 負なら反転を戻し、乗算を戻す (`fromBlackMat`)
//!
//! 1回の走査は、その行 (列) の**走査前の写し**から隣を読んで書き込む。
//! 隣は左右 (上下) の明るいほうで、RGB では `r+g+b` の大きいほうを選ぶ。
//!
//! 元から写した細かいところ:
//!
//! - 8/16bit の計算は整数で、割り算は切り捨て。32bit は f32
//! - RGB の走査は RGB だけを書き、アルファはそのまま。アルファの走査は
//!   その逆
//! - 乗算と乗算戻しは `toBlackMat` / `fromBlackMat` そのままで、
//!   整数の切り捨て割り算。往復で半透明の値がわずかに下がる
//! - 回数ぶんのくりかえしは「RGB を全部 → アルファを全部」の順で、
//!   混ざらない
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **RGB の走査で隣の明るさを比べる変数が初期化されていない**。元は
//!    `r1 = r2 = r` までしか書いておらず、`v1` / `v2` は関数の先頭で
//!    宣言されたまま前のくりかえしの値を持っている。左端 (上端) と
//!    右端 (下端) で、隣を使うかどうかが前の行の残りかすで決まっていた。
//!    自分の値で初期化するようにした
//! 2. **縦のアルファの走査が行ではなく列の番号で端を判定している**
//!    (`if (x > 0)` / `if (x < width - 1)`、`y` のつもりのところ)。
//!    左端の列では上を一度も見ず、それ以外の列では `y = 0` で
//!    バッファの手前を読んでいた。`y` で判定するようにした
//! 3. **32bit の反転が `11.0 - g`** になっていた (`1.0` の打ち間違い。
//!    赤だけ正しい)。32bit で回数を負にすると絵が壊れる
//! 4. **解像度が 1/1 でないと何もしない**ガードがあった
//!    (`if (ae->downSaleValue() != 1) return;`)。回数には downsample を
//!    掛けているのだから効かせるつもりだったと見て外した。半解像度の
//!    プレビューで効果が消えなくなる
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/Max_Kasumi

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 1画素。深度に依らず f64 で持ち、計算の丸めは `Ops` が深度ごとに合わせる。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Px {
    r: f64,
    g: f64,
    b: f64,
    a: f64,
}

/// 深度ごとの計算の作法。`float` が立っていれば 32bit (f32 で計算)、
/// そうでなければ 8/16bit (整数で、割り算は切り捨て)。
#[derive(Debug, Clone, Copy)]
struct Ops {
    max: f64,
    float: bool,
}

impl Ops {
    /// 元の `v = r + g + b`。32bit では f32 で足す。
    fn sum(&self, p: &Px) -> f64 {
        if self.float {
            ((p.r as f32) + (p.g as f32) + (p.b as f32)) as f64
        } else {
            p.r + p.g + p.b
        }
    }

    /// 元の寄せ方。`best` (隣の値) を超えない。
    fn grow(&self, v: f64, best: f64) -> f64 {
        if self.float {
            let (v, best) = (v as f32, best as f32);
            let p = best / 4.0;
            let mut r = v + p - v * p;
            if r > best {
                r = best;
            }
            r as f64
        } else {
            let p = (best / 4.0).trunc();
            let mut r = v + p - (v * p / self.max).trunc();
            if r > best {
                r = best;
            }
            r
        }
    }

    /// 元の Rev。32bit は 1.0 で押さえてから引く。
    fn rev(&self, p: &Px) -> Px {
        let f = |v: f64| {
            let v = if self.float { v.min(self.max) } else { v };
            self.max - v
        };
        Px {
            r: f(p.r),
            g: f(p.g),
            b: f(p.b),
            a: f(p.a),
        }
    }

    /// 元の toBlackMat。
    fn to_black_mat(&self, p: &Px) -> Px {
        if p.a >= self.max {
            return *p;
        }
        if p.a <= 0.0 {
            return Px {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: p.a,
            };
        }
        let f = |v: f64| self.clamp(self.mul(v, p.a));
        Px {
            r: f(p.r),
            g: f(p.g),
            b: f(p.b),
            a: p.a,
        }
    }

    /// 元の fromBlackMat。
    fn from_black_mat(&self, p: &Px) -> Px {
        if p.a >= self.max {
            return *p;
        }
        if p.a <= 0.0 {
            return Px {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: p.a,
            };
        }
        let f = |v: f64| self.clamp(self.div(v, p.a));
        Px {
            r: f(p.r),
            g: f(p.g),
            b: f(p.b),
            a: p.a,
        }
    }

    /// 8/16bit は `v * a / max` の整数割り算、32bit は `v * a`。
    fn mul(&self, v: f64, a: f64) -> f64 {
        if self.float {
            ((v as f32) * (a as f32)) as f64
        } else {
            (v * a / self.max).trunc()
        }
    }

    /// 8/16bit は `v * max / a`、32bit は `v / a`。
    fn div(&self, v: f64, a: f64) -> f64 {
        if self.float {
            ((v as f32) / (a as f32)) as f64
        } else {
            (v * self.max / a).trunc()
        }
    }

    /// 元の RoundByteLong / RoundShort / RoundFpShortDouble。
    fn clamp(&self, v: f64) -> f64 {
        let hi = if self.float { 32.0 } else { self.max };
        v.clamp(0.0, hi)
    }
}

/// 走査の向き。元の ScanLine (1 = 横と縦、2 = 横、3 = 縦)。
/// 対象のチャンネル。元の Channel (1 = RGB とアルファ、2 = RGB、3 = アルファ)。
#[derive(Debug, Clone, Copy)]
struct Info {
    max: i64,
    minus: bool,
    dir: i64,
    ch: i64,
}

/// 元の MaxHorRGB / MaxVerRGB。`vert` が立っていれば列を走る。
fn pass_rgb(buf: &mut [Px], w: usize, h: usize, vert: bool, ops: &Ops) {
    let (outer, inner) = if vert { (w, h) } else { (h, w) };
    let at = |o: usize, i: usize| if vert { i * w + o } else { o * w + i };
    let mut scan: Vec<Px> = Vec::with_capacity(inner);
    // 元は v1 / v2 を関数の先頭で宣言したまま初期化しておらず、端の回で
    // 前の回の値が残る (直したところ 1)。FAITHFUL ではその持ち越しを
    // 真似る。最初の1回だけは元では本物のスタックのごみなので、ここでは
    // 0 から始める (再現しきれないところ)。
    let mut carry1 = 0.0f64;
    let mut carry2 = 0.0f64;
    for o in 0..outer {
        scan.clear();
        for i in 0..inner {
            scan.push(buf[at(o, i)]);
        }
        for i in 0..inner {
            let me = scan[i];
            let v = ops.sum(&me);
            let mut n1 = me;
            let mut n2 = me;
            let mut v1 = if FAITHFUL { carry1 } else { v };
            let mut v2 = if FAITHFUL { carry2 } else { v };
            if i > 0 {
                n1 = scan[i - 1];
                v1 = ops.sum(&n1);
            }
            if i + 1 < inner {
                n2 = scan[i + 1];
                v2 = ops.sum(&n2);
            }
            if v1 < v2 {
                n1 = n2;
                v1 = v2;
            }
            carry1 = v1;
            carry2 = v2;
            if v < v1 {
                buf[at(o, i)] = Px {
                    r: ops.grow(me.r, n1.r),
                    g: ops.grow(me.g, n1.g),
                    b: ops.grow(me.b, n1.b),
                    a: me.a,
                };
            }
        }
    }
}

/// 元の MaxHorA / MaxVerA。
fn pass_alpha(buf: &mut [Px], w: usize, h: usize, vert: bool, ops: &Ops) {
    let (outer, inner) = if vert { (w, h) } else { (h, w) };
    let at = |o: usize, i: usize| if vert { i * w + o } else { o * w + i };
    let mut scan: Vec<Px> = Vec::with_capacity(inner);
    for o in 0..outer {
        scan.clear();
        for i in 0..inner {
            scan.push(buf[at(o, i)]);
        }
        for i in 0..inner {
            let me = scan[i];
            let a = me.a;
            let mut a1 = a;
            let mut a2 = a;
            // 元の縦だけ、行ではなく列の番号で端を見ていた (直したところ 2)。
            let (lo, hi) = if FAITHFUL && vert {
                (o > 0, o + 1 < w)
            } else {
                (i > 0, i + 1 < inner)
            };
            if lo && i > 0 {
                a1 = scan[i - 1].a;
            }
            if hi && i + 1 < inner {
                a2 = scan[i + 1].a;
            }
            let best = if a1 < a2 { a2 } else { a1 };
            if a < best {
                buf[at(o, i)].a = ops.grow(a, best);
            }
        }
    }
}

/// 元の Exec8/16/32 の、走査のところだけ。
fn run(buf: &mut [Px], w: usize, h: usize, info: &Info, ops: &Ops) {
    for p in buf.iter_mut() {
        *p = ops.to_black_mat(p);
    }
    if info.minus {
        for p in buf.iter_mut() {
            *p = ops.rev(p);
        }
    }
    if info.ch == 1 || info.ch == 2 {
        for _ in 0..info.max {
            if info.dir == 1 || info.dir == 2 {
                pass_rgb(buf, w, h, false, ops);
            }
            if info.dir == 1 || info.dir == 3 {
                pass_rgb(buf, w, h, true, ops);
            }
        }
    }
    if info.ch == 1 || info.ch == 3 {
        for _ in 0..info.max {
            if info.dir == 1 || info.dir == 2 {
                pass_alpha(buf, w, h, false, ops);
            }
            if info.dir == 1 || info.dir == 3 {
                pass_alpha(buf, w, h, true, ops);
            }
        }
    }
    if info.minus {
        for p in buf.iter_mut() {
            *p = ops.rev(p);
        }
    }
    for p in buf.iter_mut() {
        *p = ops.from_black_mat(p);
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Max,
    Dir,
    Channel,
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
        // 元: PF_ADD_SLIDER("Value", -30, 30, -15, 15, 0)
        params.add(
            Params::Max,
            "Value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-30);
                f.set_valid_max(30);
                f.set_slider_min(-15);
                f.set_slider_max(15);
                f.set_default(0);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP("ScanLine", 3, 1, "Horizon+Vertical|Horizon|Vertical")
        params.add(
            Params::Dir,
            "ScanLine",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Horizon+Vertical", "Horizon", "Vertical"]);
                f.set_default(1);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_POPUP("Channel", 3, 1, "RGB+Alpha|RGB|alpha")
        params.add(
            Params::Channel,
            "Channel",
            ae::PopupDef::setup(|f| {
                f.set_options(&["RGB+Alpha", "RGB", "alpha"]);
                f.set_default(1);
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
                out_data.set_return_msg("fs-rs maxkasumi");
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
        // 元の GetParams: downsample を掛けてから符号を落とす。
        let ds = in_data.downsample_x();
        let v = params.get(Params::Max)?.as_slider()?.value() as i64 * ds.num as i64
            / ds.den as i64;
        let info = Info {
            max: v.abs(),
            minus: v < 0,
            dir: params.get(Params::Dir)?.as_popup()?.value() as i64,
            ch: params.get(Params::Channel)?.as_popup()?.value() as i64,
        };

        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Exec: まず入力をコピー。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        if info.max == 0 || w == 0 || h == 0 {
            return Ok(());
        }
        // 元: 解像度が 1/1 でなければ何もしなかった (直したところ 4)。
        if FAITHFUL && ds.den as i64 != ds.num as i64 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                let mut buf: Vec<Px> = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        buf.push($get(out_layer.$rd(x, y)));
                    }
                }
                run(&mut buf, w, h, &info, &ops);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), &buf[x + y * w]);
                    }
                }
            }};
        }
        match depth {
            8 => go!(
                as_pixel8,
                as_pixel8_mut,
                Ops {
                    max: 255.0,
                    float: false
                },
                |p: &ae::Pixel8| Px {
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64,
                    a: p.alpha as f64
                },
                |d: &mut ae::Pixel8, s: &Px| {
                    d.red = s.r as i64 as u8;
                    d.green = s.g as i64 as u8;
                    d.blue = s.b as i64 as u8;
                    d.alpha = s.a as i64 as u8;
                }
            ),
            16 => go!(
                as_pixel16,
                as_pixel16_mut,
                Ops {
                    max: 32768.0,
                    float: false
                },
                |p: &ae::Pixel16| Px {
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64,
                    a: p.alpha as f64
                },
                |d: &mut ae::Pixel16, s: &Px| {
                    d.red = s.r as i64 as u16;
                    d.green = s.g as i64 as u16;
                    d.blue = s.b as i64 as u16;
                    d.alpha = s.a as i64 as u16;
                }
            ),
            _ => go!(
                as_pixel32,
                as_pixel32_mut,
                Ops {
                    max: 1.0,
                    float: true
                },
                |p: &ae::PixelF32| Px {
                    r: p.red as f64,
                    g: p.green as f64,
                    b: p.blue as f64,
                    a: p.alpha as f64
                },
                |d: &mut ae::PixelF32, s: &Px| {
                    d.red = s.r as f32;
                    d.green = s.g as f32;
                    d.blue = s.b as f32;
                    d.alpha = s.a as f32;
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

    fn px(r: f64, a: f64) -> Px {
        Px { r, g: r, b: r, a }
    }

    fn line(vals: &[f64]) -> Vec<Px> {
        vals.iter().map(|v| px(*v, 255.0)).collect()
    }

    /// 明るいところは隣に少しだけ漏れる。隣の値は超えない。
    #[test]
    fn the_bright_spot_bleeds_a_little() {
        let mut buf = line(&[0.0, 0.0, 200.0, 0.0, 0.0]);
        pass_rgb(&mut buf, 5, 1, false, &ops8());
        // 200/4 = 50 ぶん明るくなる (自分が 0 なので引く項も 0)。
        assert_eq!(buf[1].r, 50.0);
        assert_eq!(buf[3].r, 50.0);
        assert_eq!(buf[2].r, 200.0);
        assert_eq!(buf[0].r, 0.0);
    }

    /// 明るいところの隣で、すでに明るい画素はほとんど動かない。
    #[test]
    fn a_bright_neighbour_barely_moves() {
        let mut buf = line(&[240.0, 255.0, 240.0]);
        pass_rgb(&mut buf, 3, 1, false, &ops8());
        // p = 63、240 + 63 - 240*63/255 = 240 + 63 - 59 = 244。
        assert_eq!(buf[0].r, 244.0);
    }

    /// 自分より暗い隣は無視される。
    #[test]
    fn a_darker_neighbour_does_nothing() {
        let mut buf = line(&[100.0, 10.0, 10.0]);
        pass_rgb(&mut buf, 3, 1, false, &ops8());
        assert_eq!(buf[0].r, 100.0);
    }

    /// 端も同じように扱われる (直したところ 1)。元は初期化されていない
    /// 変数で決まっていた。
    #[test]
    fn the_ends_see_their_only_neighbour() {
        let mut buf = line(&[0.0, 200.0, 0.0, 0.0]);
        pass_rgb(&mut buf, 4, 1, false, &ops8());
        assert_eq!(buf[0].r, 50.0, "左端が右の隣を見ていない");
    }

    /// RGB の走査はアルファを触らない。
    #[test]
    fn the_rgb_pass_leaves_alpha_alone() {
        let mut buf = vec![px(0.0, 10.0), px(200.0, 250.0), px(0.0, 10.0)];
        pass_rgb(&mut buf, 3, 1, false, &ops8());
        assert_eq!(buf[0].a, 10.0);
    }

    /// アルファの走査は RGB を触らない。
    #[test]
    fn the_alpha_pass_leaves_rgb_alone() {
        let mut buf = vec![px(7.0, 0.0), px(7.0, 200.0), px(7.0, 0.0)];
        pass_alpha(&mut buf, 3, 1, false, &ops8());
        assert_eq!(buf[0].a, 50.0);
        assert_eq!(buf[0].r, 7.0);
    }

    /// 縦のアルファの走査も上下の隣を見る (直したところ 2)。元は列の番号で
    /// 端を判定していて、左端の列では上を見なかった。
    #[test]
    fn the_vertical_alpha_pass_looks_up_and_down() {
        // 幅 2・高さ 3。左の列だけに明るいアルファを置く。
        let mut buf = vec![
            px(0.0, 0.0),
            px(0.0, 0.0),
            px(0.0, 200.0),
            px(0.0, 0.0),
            px(0.0, 0.0),
            px(0.0, 0.0),
        ];
        pass_alpha(&mut buf, 2, 3, true, &ops8());
        assert_eq!(buf[0].a, 50.0, "左端の列で上下の隣を見ていない");
        assert_eq!(buf[4].a, 50.0);
    }

    /// 反転は最大値からの引き算。32bit は 1.0 で押さえてから引く
    /// (直したところ 3: 元は 11.0 から引いていた)。
    #[test]
    fn the_invert_subtracts_from_the_maximum() {
        let o = ops8();
        assert_eq!(o.rev(&px(10.0, 20.0)).r, 245.0);
        let f = Ops {
            max: 1.0,
            float: true,
        };
        let r = f.rev(&Px {
            r: 0.25,
            g: 2.0,
            b: 0.0,
            a: 1.0,
        });
        assert_eq!(r.r, 0.75);
        assert_eq!(r.g, 0.0, "1.0 で押さえてから引く");
        assert_eq!(r.b, 1.0);
        assert_eq!(r.a, 0.0);
    }

    /// 乗算の往復。切り捨てなので半透明の値はわずかに下がる。
    #[test]
    fn the_premultiply_round_trip_loses_a_little() {
        let o = ops8();
        let p = px(200.0, 128.0);
        let back = o.from_black_mat(&o.to_black_mat(&p));
        assert!(back.r <= 200.0 && back.r >= 196.0, "{}", back.r);
        // 不透明ならそのまま。
        let q = px(200.0, 255.0);
        assert_eq!(o.from_black_mat(&o.to_black_mat(&q)), q);
    }

    /// 回数ぶんくりかえすと広がる幅が増える。
    #[test]
    fn more_rounds_spread_further() {
        let info = |n: i64| Info {
            max: n,
            minus: false,
            dir: 2,
            ch: 2,
        };
        let count = |n: i64| {
            let mut buf = line(&[0.0, 0.0, 0.0, 255.0, 0.0, 0.0, 0.0]);
            run(&mut buf, 7, 1, &info(n), &ops8());
            buf.iter().filter(|p| p.r > 0.0).count()
        };
        assert!(count(3) > count(1), "{} > {}", count(3), count(1));
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }
}
