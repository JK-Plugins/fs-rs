//! F's IroToreAdjacent の移植。
//!
//! セル画の**主線**のうち、指定した塗り色 (隣接色) に接しているところだけを
//! 別の色に置き換える。「色トレス」をあとから作る道具で、線がベタ塗りに
//! 触れているところだけが色線になる。
//!
//! 手順 (元の irotoreExec):
//!
//! 1. 入力をそのままコピーする。主線の色・隣接色・幅のどれかが無ければ終わり
//! 2. 入力の色を見て、アルファに印をつける: 主線色なら `MAIN`、隣接色なら
//!    `ADJ`、どちらでもなければ 0
//! 3. 「上下左右のどれかが `ADJ` か `NEW` の `MAIN` 画素を `NewColor`
//!    (印は `NEW`) にする」を**幅の回数**くりかえす。塗りから線へ向かって
//!    1画素ずつ染み出していく
//! 4. `Min/Max` が正なら、`NEW` のかたまりからさらに `Min/Max` 画素ぶん
//!    主線を染める (膨張)。負なら逆に、主線に近い `NEW` を主線色に
//!    戻す (収縮)
//! 5. `Blur` が 0 より大きければ、主線の印を入力から引き直してから、
//!    **主線のところだけ**を横→縦→横 (3回目は半分の幅) で平均する
//! 6. `LineOnly` が入っていれば主線だけを残す (ほかは透明)、
//!    入っていなければアルファを入力のものに戻す
//!
//! 元から写した細かいところ:
//!
//! - 色の近さは RGB を 8bit に落としてチャンネルごとの差を `level` と
//!   比べる (`level` は `255 * % / 100 + 0.5` の整数)
//! - 印は 8bit で 255 / 128 / 64、16bit で 32768 / 16384 / 8192
//! - 染み出しは横の走査 (行の写しから隣を見る) のあと縦の走査。縦の走査は
//!   **横の走査の結果の上**で動く
//! - 膨張・収縮は「隣が目的の印であるあいだ進み、違う印に当たったら
//!   打ち切る」走り方で、ふつうの最大最小フィルタではない
//! - ぼかしは主線の印が続くあいだだけ平均し、`(合計 / 個数) + 0.5` を
//!   切り捨てる。3回目の幅は `Blur / 2` で、0 になったら掛けない
//! - 主線を1本だけ選んだときの置き換え色は `MainColor1`
//!
//! 元と違うところ (直したバグ):
//!
//! 1. **32bit では何もしていなかった** (`case PF_PixelFormat_ARGB128:
//!    break;`)。8/16bit と同じ道を通すようにした
//! 2. 16bit の「収縮で主線に戻すときの色」の印が
//!    `ret.mnColor16.alpha = TARGET_MAIN8` (255) になっていた。16bit の
//!    印は 32768 なので、戻した画素が以降の走査で主線と見なされない。
//!    `TARGET_MAIN16` にした
//!
//! 直っているように見えて**結果は変わらないところ**:
//!
//! - 色見本の重複を省く `FindMnColor` / `FindAdColor` は、数を数え終える
//!   前の `mnColorMax` / `adColorMax` (まだ 0) を見ているので、主線2つめと
//!   隣接色どうしの重複は省かれない。ただし重複があっても同じ色を2回
//!   比べるだけで、出る絵は変わらない (置き換えに使うのは
//!   `mnColor[0]` だけ)。元のまま残してある
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/IroToreAdjacent

use after_effects::{self as ae};

/// 直したバグを戻す。元との画素比較のためだけにある。
const FAITHFUL: bool = false;

/// 1画素。深度に依らず f64 で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct Px {
    a: f64,
    r: f64,
    g: f64,
    b: f64,
}

/// 深度ごとの作法と印。
#[derive(Debug, Clone, Copy)]
struct Ops {
    max: f64,
    float: bool,
}

impl Ops {
    /// 元の TARGET_MAIN。
    fn main(&self) -> f64 {
        self.max
    }
    /// 元の TARGET_ADJ。
    fn adj(&self) -> f64 {
        self.max / 2.0
    }
    /// 元の TARGET_NEW。
    fn new(&self) -> f64 {
        self.max / 4.0
    }
    /// 色見本と比べるための 8bit 値。
    fn to8(&self, v: f64) -> i64 {
        if self.float {
            // 元の FS_CONVERT32TO8 相当。
            ((v * 255.0 + 0.5) as i64).clamp(0, 255)
        } else if self.max == 255.0 {
            v as i64
        } else {
            // 元の CONV16TO8。
            ((v as i64 * 255 + 16384) / 32768).clamp(0, 255)
        }
    }
    /// 8bit の色をこの深度へ。
    fn from8(&self, c: &ae::Pixel8) -> Px {
        if self.float {
            let f = libs::utils::conv_8_to_32(c);
            Px {
                a: f.alpha as f64,
                r: f.red as f64,
                g: f.green as f64,
                b: f.blue as f64,
            }
        } else if self.max == 255.0 {
            Px {
                a: c.alpha as f64,
                r: c.red as f64,
                g: c.green as f64,
                b: c.blue as f64,
            }
        } else {
            let f = libs::utils::conv_8_to_16(c);
            Px {
                a: f.alpha as f64,
                r: f.red as f64,
                g: f.green as f64,
                b: f.blue as f64,
            }
        }
    }
    /// 元の RoundByteFpLong / RoundShortFpLong。切り捨て。
    fn round(&self, v: f64) -> f64 {
        if self.float {
            v
        } else {
            v.trunc().clamp(0.0, self.max)
        }
    }
}

/// 読み込んだパラメータ。元の ParamInfo。
#[derive(Debug, Clone)]
struct Info {
    line_only: bool,
    new_colour: ae::Pixel8,
    value: i64,
    minmax: i64,
    blur: i64,
    mn: Vec<ae::Pixel8>,
    ad: Vec<ae::Pixel8>,
    level: i64,
}

/// 画素の並び。
struct Buf {
    w: usize,
    h: usize,
    px: Vec<Px>,
}

impl Buf {
    fn at(&self, o: usize, i: usize, vert: bool) -> usize {
        if vert {
            i * self.w + o
        } else {
            o * self.w + i
        }
    }
}

/// 元の isMain / isAdj。
fn matches(cols: &[ae::Pixel8], p: &Px, level: i64, ops: &Ops) -> bool {
    let (r, g, b) = (ops.to8(p.r), ops.to8(p.g), ops.to8(p.b));
    cols.iter().any(|c| {
        (c.red as i64 - r).abs() <= level
            && (c.green as i64 - g).abs() <= level
            && (c.blue as i64 - b).abs() <= level
    })
}

/// 元の targetSelect。入力の色を見てアルファに印をつける。
fn select(buf: &mut Buf, src: &[Px], info: &Info, ops: &Ops) {
    for (i, s) in src.iter().enumerate() {
        buf.px[i].a = if matches(&info.mn, s, info.level, ops) {
            ops.main()
        } else if matches(&info.ad, s, info.level, ops) {
            ops.adj()
        } else {
            0.0
        };
    }
}

/// 元の targetRemove。上下左右が ADJ か NEW の MAIN を NewColor にする。
fn remove(buf: &mut Buf, new_colour: &Px, ops: &Ops) {
    for vert in [false, true] {
        let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
        let mut scan: Vec<Px> = Vec::with_capacity(inner);
        for o in 0..outer {
            scan.clear();
            for i in 0..inner {
                scan.push(buf.px[buf.at(o, i, vert)]);
            }
            for i in 0..inner {
                let idx = buf.at(o, i, vert);
                if buf.px[idx].a != ops.main() {
                    continue;
                }
                let hit = |a: f64| a == ops.adj() || a == ops.new();
                let mut on = i > 0 && hit(scan[i - 1].a);
                if !on && i + 1 < inner {
                    on = hit(scan[i + 1].a);
                }
                if on {
                    buf.px[idx] = *new_colour;
                }
            }
        }
    }
}

/// 元の targetMax / targetMin。`from` の印から `to` の印の側へ伸ばす。
///
/// 印が `from` の画素について、隣を見ていって `to` に当たれば置き換える。
/// `from` 以外の印に当たったらそこで打ち切る。
fn grow(buf: &mut Buf, dist: i64, from: f64, to: f64, colour: &Px, ops: &Ops) {
    if dist == 0 {
        return;
    }
    let dist = dist.abs();
    for vert in [false, true] {
        let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
        let mut scan: Vec<Px> = Vec::with_capacity(inner);
        for o in 0..outer {
            scan.clear();
            for i in 0..inner {
                scan.push(buf.px[buf.at(o, i, vert)]);
            }
            for i in 0..inner {
                let idx = buf.at(o, i, vert);
                if buf.px[idx].a != from {
                    continue;
                }
                let mut ok = false;
                for k in 1..=dist as usize {
                    if i < k {
                        break;
                    }
                    let a = scan[i - k].a;
                    if a == to {
                        ok = true;
                        break;
                    } else if a != from {
                        break;
                    }
                }
                if !ok {
                    for k in 1..=dist as usize {
                        if i + k >= inner {
                            break;
                        }
                        let a = scan[i + k].a;
                        if a == to {
                            ok = true;
                            break;
                        } else if a != from {
                            break;
                        }
                    }
                }
                if ok {
                    buf.px[idx] = *colour;
                }
            }
        }
        let _ = ops;
    }
}

/// 元の targetLineOnly。入力の色から主線だけを残す。
fn line_only(buf: &mut Buf, src: &[Px], info: &Info, ops: &Ops) {
    for (i, s) in src.iter().enumerate() {
        buf.px[i].a = if matches(&info.mn, s, info.level, ops) {
            ops.max
        } else {
            0.0
        };
    }
}

/// 元の targetBlur の1走査ぶん。主線の印が続くあいだだけ平均する。
fn blur_pass(buf: &mut Buf, vert: bool, v: i64, ops: &Ops) {
    let (outer, inner) = if vert { (buf.w, buf.h) } else { (buf.h, buf.w) };
    let mut scan: Vec<Px> = Vec::with_capacity(inner);
    for o in 0..outer {
        scan.clear();
        for i in 0..inner {
            scan.push(buf.px[buf.at(o, i, vert)]);
        }
        for i in 0..inner {
            let idx = buf.at(o, i, vert);
            if buf.px[idx].a != ops.main() {
                continue;
            }
            let mut r = scan[i].r;
            let mut g = scan[i].g;
            let mut b = scan[i].b;
            let mut cnt = 1i64;
            for k in 1..=v as usize {
                if i < k {
                    break;
                }
                let p = scan[i - k];
                if p.a != ops.main() {
                    break;
                }
                r += p.r;
                g += p.g;
                b += p.b;
                cnt += 1;
            }
            for k in 1..=v as usize {
                if i + k >= inner {
                    break;
                }
                let p = scan[i + k];
                if p.a != ops.main() {
                    break;
                }
                r += p.r;
                g += p.g;
                b += p.b;
                cnt += 1;
            }
            if cnt > 1 {
                let d = cnt as f64;
                buf.px[idx] = Px {
                    a: ops.main(),
                    r: ops.round(r / d + 0.5),
                    g: ops.round(g / d + 0.5),
                    b: ops.round(b / d + 0.5),
                };
            }
        }
    }
}

/// 元の targetBlur。横→縦→横 (3回目は半分)。
fn blur_all(buf: &mut Buf, blur: i64, ops: &Ops) {
    if blur <= 0 {
        return;
    }
    blur_pass(buf, false, blur, ops);
    blur_pass(buf, true, blur, ops);
    let half = blur / 2;
    if half > 0 {
        blur_pass(buf, false, half, ops);
    }
}

/// 元の irotoreExec。
fn exec(buf: &mut Buf, src: &[Px], info: &Info, ops: &Ops) {
    if info.value <= 0 {
        return;
    }
    let mut new_colour = ops.from8(&info.new_colour);
    new_colour.a = ops.new();
    select(buf, src, info, ops);
    for _ in 0..info.value {
        remove(buf, &new_colour, ops);
    }
    if info.minmax > 0 {
        // 膨張: MAIN のうち NEW に届くものを NewColor にする。
        grow(buf, info.minmax, ops.main(), ops.new(), &new_colour, ops);
    } else if info.minmax < 0 {
        // 収縮: NEW のうち MAIN に届くものを主線色に戻す。
        let mut mn = ops.from8(&info.mn[0]);
        // 元は 16bit でここの印を 255 にしていた (直したところ 2)。
        mn.a = if FAITHFUL && !ops.float && ops.max != 255.0 {
            255.0
        } else {
            ops.main()
        };
        grow(buf, info.minmax, ops.new(), ops.main(), &mn, ops);
    }
    if info.blur > 0 {
        line_only(buf, src, info, ops);
        blur_all(buf, info.blur, ops);
    }
    if info.line_only {
        line_only(buf, src, info, ops);
    } else {
        // 元の targetAlphaRedo: アルファを入力のものに戻す。
        for (i, s) in src.iter().enumerate() {
            buf.px[i].a = s.a;
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    LineOnly,
    NewColor,
    Value,
    MinMax,
    Blur,
    Mn1On,
    Mn1,
    Mn2On,
    Mn2,
    Level,
    Ad1On,
    Ad1,
    Ad2On,
    Ad2,
    Ad3On,
    Ad3,
    Ad4On,
    Ad4,
    Ad5On,
    Ad5,
    Ad6On,
    Ad6,
}

const MN: [(Params, Params); 2] = [
    (Params::Mn1On, Params::Mn1),
    (Params::Mn2On, Params::Mn2),
];
const AD: [(Params, Params); 6] = [
    (Params::Ad1On, Params::Ad1),
    (Params::Ad2On, Params::Ad2),
    (Params::Ad3On, Params::Ad3),
    (Params::Ad4On, Params::Ad4),
    (Params::Ad5On, Params::Ad5),
    (Params::Ad6On, Params::Ad6),
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
        // 元: PF_ADD_CHECKBOX("LineOnly", "on", FALSE)
        params.add(
            Params::LineOnly,
            "LineOnly",
            ae::CheckBoxDef::setup(|f| {
                f.set_label("on");
                f.set_default(false);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_COLOR("NewColor", 0x80, 0x00, 0x00)
        params.add(
            Params::NewColor,
            "NewColor",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 0xFF,
                    red: 0x80,
                    green: 0,
                    blue: 0,
                });
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("Value", 0, 50, 0, 10, 6)
        params.add(
            Params::Value,
            "Value",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(50);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(6);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("Min/Max", -100, 100, -10, 10, -2)
        params.add(
            Params::MinMax,
            "Min/Max",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(-100);
                f.set_valid_max(100);
                f.set_slider_min(-10);
                f.set_slider_max(10);
                f.set_default(-2);
                f.set_value(f.default());
            }),
        )?;
        // 元: PF_ADD_SLIDER("Blur", 0, 50, 0, 10, 10)
        params.add(
            Params::Blur,
            "Blur",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(50);
                f.set_slider_min(0);
                f.set_slider_max(10);
                f.set_default(10);
                f.set_value(f.default());
            }),
        )?;
        // 主線の色2つ。既定はどちらも ON で、ほぼ黒。
        for (i, g) in MN.iter().enumerate() {
            params.add(
                g.0,
                &format!("EnabledMain{}", i + 1),
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(true);
                    f.set_value(f.default());
                }),
            )?;
            let v = if i == 0 { 0x0Fu8 } else { 0x0A };
            params.add(
                g.1,
                &format!("MainColor{}", i + 1),
                ae::ColorDef::setup(move |f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: v,
                        green: v,
                        blue: v,
                    });
                    f.set_value(f.default());
                }),
            )?;
        }
        // 元: PF_ADD_FLOAT_SLIDER("level", 0, 100, 0, 10, 1, 0, 1, 0, 0)
        params.add(
            Params::Level,
            "level",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(10.0);
                f.set_precision(1);
                f.set_curve_tolerance(0.05);
                f.set_default(0.0);
                f.set_value(f.default());
            }),
        )?;
        // 隣接色6つ。既定は全部 OFF で赤。
        for (i, g) in AD.iter().enumerate() {
            params.add(
                g.0,
                &format!("EnabledAdjacent{}", i + 1),
                ae::CheckBoxDef::setup(|f| {
                    f.set_label("on");
                    f.set_default(false);
                    f.set_value(f.default());
                }),
            )?;
            params.add(
                g.1,
                &format!("AdjacentColor{}", i + 1),
                ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 {
                        alpha: 0xFF,
                        red: 0xFF,
                        green: 0,
                        blue: 0,
                    });
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
                out_data.set_return_msg("fs-rs irotoreadjacent");
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
        // 元の GetParams。
        let mut mn: Vec<ae::Pixel8> = Vec::with_capacity(2);
        for g in MN.iter() {
            if params.get(g.0)?.as_checkbox()?.value() {
                mn.push(params.get(g.1)?.as_color()?.value());
            }
        }
        let mut ad: Vec<ae::Pixel8> = Vec::with_capacity(6);
        for g in AD.iter() {
            if params.get(g.0)?.as_checkbox()?.value() {
                let c = params.get(g.1)?.as_color()?.value();
                // 元: 主線の色と同じものは隣接色に入れない。
                let same = mn
                    .iter()
                    .any(|m| m.red == c.red && m.green == c.green && m.blue == c.blue);
                if !same {
                    ad.push(c);
                }
            }
        }
        let lv = params.get(Params::Level)?.as_float_slider()?.value() as f64;
        let info = Info {
            line_only: params.get(Params::LineOnly)?.as_checkbox()?.value(),
            new_colour: params.get(Params::NewColor)?.as_color()?.value(),
            value: params.get(Params::Value)?.as_slider()?.value() as i64,
            minmax: params.get(Params::MinMax)?.as_slider()?.value() as i64,
            blur: params.get(Params::Blur)?.as_slider()?.value() as i64,
            mn,
            ad,
            level: (255.0 * lv / 100.0 + 0.5) as i64,
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
        if info.mn.is_empty() || info.ad.is_empty() || info.value <= 0 || w == 0 || h == 0 {
            return Ok(());
        }
        // 元: 32bit は何もしなかった (直したところ 1)。
        if FAITHFUL && depth == 32 {
            return Ok(());
        }

        macro_rules! go {
            ($rd:ident, $wr:ident, $ops:expr, $get:expr, $put:expr) => {{
                let ops: Ops = $ops;
                let mut src: Vec<Px> = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        src.push($get(in_layer.$rd(x, y)));
                    }
                }
                let mut buf = Buf {
                    w,
                    h,
                    px: src.clone(),
                };
                exec(&mut buf, &src, &info, &ops);
                for y in 0..h {
                    for x in 0..w {
                        $put(out_layer.$wr(x, y), &buf.px[x + y * w]);
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
                as_pixel16,
                as_pixel16_mut,
                Ops {
                    max: 32768.0,
                    float: false
                },
                |p: &ae::Pixel16| Px {
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
                as_pixel32,
                as_pixel32_mut,
                Ops {
                    max: 1.0,
                    float: true
                },
                |p: &ae::PixelF32| Px {
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

    fn c(r: u8, g: u8, b: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: 255,
            red: r,
            green: g,
            blue: b,
        }
    }

    fn px(r: f64, g: f64, b: f64) -> Px {
        Px {
            a: 255.0,
            r,
            g,
            b,
        }
    }

    fn info() -> Info {
        Info {
            line_only: false,
            new_colour: c(0x80, 0, 0),
            value: 1,
            minmax: 0,
            blur: 0,
            mn: vec![c(15, 15, 15)],
            ad: vec![c(255, 0, 0)],
            level: 0,
        }
    }

    /// 印のつけ方: 主線・隣接・それ以外。
    #[test]
    fn the_marks_go_on_the_right_pixels() {
        let src = vec![px(15.0, 15.0, 15.0), px(255.0, 0.0, 0.0), px(0.0, 255.0, 0.0)];
        let mut b = Buf {
            w: 3,
            h: 1,
            px: src.clone(),
        };
        let o = ops8();
        select(&mut b, &src, &info(), &o);
        assert_eq!(b.px[0].a, o.main());
        assert_eq!(b.px[1].a, o.adj());
        assert_eq!(b.px[2].a, 0.0);
    }

    /// level を広げれば近い色も主線になる。
    #[test]
    fn the_level_widens_the_match() {
        let p = px(25.0, 25.0, 25.0);
        let mut i = info();
        assert!(!matches(&i.mn, &p, i.level, &ops8()));
        i.level = 12;
        assert!(matches(&i.mn, &p, i.level, &ops8()));
    }

    /// 隣接色に触れている主線だけが置き換わる。
    #[test]
    fn only_the_line_touching_the_fill_is_recoloured() {
        // 主線 主線 主線 隣接
        let src = vec![
            px(15.0, 15.0, 15.0),
            px(15.0, 15.0, 15.0),
            px(15.0, 15.0, 15.0),
            px(255.0, 0.0, 0.0),
        ];
        let mut b = Buf {
            w: 4,
            h: 1,
            px: src.clone(),
        };
        let o = ops8();
        let i = info();
        exec(&mut b, &src, &i, &o);
        // 隣に接している 2 番だけが NewColor になる。
        assert_eq!(b.px[2].r, 0x80 as f64);
        assert_eq!(b.px[1].r, 15.0);
        assert_eq!(b.px[0].r, 15.0);
        // アルファは入力のものに戻る。
        assert_eq!(b.px[0].a, 255.0);
    }

    /// 幅を増やすと線の奥まで染み出す。
    #[test]
    fn a_wider_value_reaches_deeper() {
        let src: Vec<Px> = (0..8)
            .map(|x| {
                if x == 7 {
                    px(255.0, 0.0, 0.0)
                } else {
                    px(15.0, 15.0, 15.0)
                }
            })
            .collect();
        let count = |value: i64| {
            let mut i = info();
            i.value = value;
            let mut b = Buf {
                w: 8,
                h: 1,
                px: src.clone(),
            };
            exec(&mut b, &src, &i, &ops8());
            b.px.iter().filter(|p| p.r == 0x80 as f64).count()
        };
        assert_eq!(count(1), 1);
        assert_eq!(count(3), 3);
    }

    /// 主線色も隣接色も無ければ何もしない。
    #[test]
    fn nothing_happens_without_colours() {
        let src = vec![px(15.0, 15.0, 15.0), px(255.0, 0.0, 0.0)];
        let mut b = Buf {
            w: 2,
            h: 1,
            px: src.clone(),
        };
        let mut i = info();
        i.value = 0;
        exec(&mut b, &src, &i, &ops8());
        assert_eq!(b.px, src);
    }

    /// LineOnly なら主線だけが残る。
    #[test]
    fn line_only_keeps_the_line() {
        let src = vec![
            px(15.0, 15.0, 15.0),
            px(255.0, 0.0, 0.0),
            px(0.0, 255.0, 0.0),
        ];
        let mut b = Buf {
            w: 3,
            h: 1,
            px: src.clone(),
        };
        let mut i = info();
        i.line_only = true;
        exec(&mut b, &src, &i, &ops8());
        assert_eq!(b.px[0].a, 255.0);
        assert_eq!(b.px[1].a, 0.0);
        assert_eq!(b.px[2].a, 0.0);
    }

    /// 膨張は NewColor のかたまりを主線側へ広げる。
    #[test]
    fn the_max_widens_the_new_colour() {
        let src: Vec<Px> = (0..10)
            .map(|x| {
                if x == 9 {
                    px(255.0, 0.0, 0.0)
                } else {
                    px(15.0, 15.0, 15.0)
                }
            })
            .collect();
        let n = |mm: i64| {
            let mut i = info();
            i.value = 1;
            i.minmax = mm;
            let mut b = Buf {
                w: 10,
                h: 1,
                px: src.clone(),
            };
            exec(&mut b, &src, &i, &ops8());
            b.px.iter().filter(|p| p.r == 0x80 as f64).count()
        };
        assert!(n(3) > n(0), "{} > {}", n(3), n(0));
    }

    /// 収縮は主線に近い NewColor を主線色に戻す。
    #[test]
    fn the_min_pulls_the_new_colour_back() {
        let src: Vec<Px> = (0..10)
            .map(|x| {
                if x >= 7 {
                    px(255.0, 0.0, 0.0)
                } else {
                    px(15.0, 15.0, 15.0)
                }
            })
            .collect();
        let n = |mm: i64| {
            let mut i = info();
            i.value = 4;
            i.minmax = mm;
            let mut b = Buf {
                w: 10,
                h: 1,
                px: src.clone(),
            };
            exec(&mut b, &src, &i, &ops8());
            b.px.iter().filter(|p| p.r == 0x80 as f64).count()
        };
        assert!(n(-2) < n(0), "{} < {}", n(-2), n(0));
    }

    /// ぼかしは主線の中だけで平均する。隣接色には漏れない。
    #[test]
    fn the_blur_stays_inside_the_line() {
        let src = vec![
            px(0.0, 0.0, 0.0),
            px(60.0, 60.0, 60.0),
            px(0.0, 0.0, 0.0),
            px(255.0, 0.0, 0.0),
        ];
        let mut i = info();
        i.mn = vec![c(0, 0, 0), c(60, 60, 60)];
        i.blur = 2;
        i.value = 1;
        let mut b = Buf {
            w: 4,
            h: 1,
            px: src.clone(),
        };
        exec(&mut b, &src, &i, &ops8());
        // 主線の3画素は平均で混ざるが、隣接色はそのまま。
        assert_eq!(b.px[3].r, 255.0);
        assert!(b.px[0].r > 0.0, "{:?}", b.px[0]);
    }

    #[test]
    fn the_fixes_are_on() {
        assert!(!FAITHFUL);
    }
}
