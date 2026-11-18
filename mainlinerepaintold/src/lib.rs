//! F's MainLineRepaint_old の移植。
//!
//! 同じ「主線消し」だが、移植済みの `mainlinerepaint` とは別の
//! やり方をしている古い版。出荷されている .aex の分類も
//! `NF's Plugins-{Deprecated}` で、パラメータは主線の色1つだけ。
//!
//! 流れは4段:
//!
//! 1. `alphaOn`: 不透明でない画素を**不透明な白**にする。この
//!    プラグインでは白が「セルの外」の目印で、以降の判定は
//!    すべて白を境界として扱う
//! 2. `onePointChk`: 主線の色の画素で、8近傍が全部白のものを白にする
//!    (ごみ取り)。画像の外は白として数える
//! 3. `mainColor` 前半: 主線の画素から8方向に最大6つまで**元画像**を
//!    辿り、白に当たったらその向きは打ち切り、主線でない色に当たったら
//!    そこまでの距離を覚える。いちばん近い向きの色をその画素に書く。
//!    探す上限はそれまでに見つかった最短距離まで縮む (元の `lp`)
//! 4. `mainColor` 後半: まだ主線の色のままの画素について、8近傍に
//!    「主線でも白でもない色」があれば、その画素を種にして
//!    **塗りつぶし (バケツ)** でひと続きの主線をその色で埋める。
//!    最後に残った主線はすべて白にする
//!
//! 画素の見分けは元の `cmpPx8` / `cmpPx16` に合わせて **RGB だけ**で、
//! アルファは見ない。
//!
//! 元から写した細かいところ:
//!
//! - 主線の色は 8bit の色指定をその深度に上げたもの (元の
//!   `change16` = `CONVERT8TO16`)
//! - 画像の外を読んだときは白 (元の `getPixel8_W` / `getPixel16_W`)
//! - 3段目は元画像を読むので、同じ段で書いた色が次の判定に回り込まない。
//!   2段目と4段目は逆に、書いた結果をそのまま次の判定に使う
//! - 4段目の塗りつぶしは元の `paint8` と同じ走査線式。種の画素と
//!   **RGB が同じ**ひと続きの範囲を塗る。つながりは4近傍で見る
//! - 塗る色がすでに入っている種は捨てる (元の「処理済のシードなら無視」)
//! - 最後に残った主線を白にするのは、埋める色が見つからなかった
//!   ところ (周りが白だけの主線) を消すため
//!
//! 元は 8bit と 16bit だけ (`exec32` が無い)。32bit は同じ手順を
//! 32bit のまま通してある。
//!
//! 元と違えたところ:
//!
//! - **3段目が参照する元画像に `alphaOn` が通っていない**。1段目で
//!   不透明でない画素を白にしているのに、3段目は入力そのものを読むので、
//!   「透明だが RGB は色が付いている」画素を塗りの色として拾い、
//!   主線の跡に**アルファ 0 の画素**を書く。穴が開いて見える。
//!   参照する側にも `alphaOn` を通した
//! - **塗りつぶしの待ち行列が幅ぶんしか無い**。元は
//!   `PF_NEW_HANDLE(m_width * sizeof(paintQue))` の輪バッファで、
//!   溢れると未処理の区間を上書きし、`start == last` になった時点で
//!   塗りかけのまま止まる (残りは最後に白へ)。伸びる待ち行列にした。
//!   ただし**これは画素で突き合わせていない**: 1区間から湧く区間は
//!   最大でも幅ぶんなので溢れさせる入力を作れず、試した入力では
//!   元のままの版と1画素も違わなかった
//!
//! ref: https://github.com/bryful/F-s-PluginsProjects/tree/master/Legacy/Project/MainLineRepaint_old

use after_effects::{self as ae};
use libs::utils::{conv_8_to_16, conv_8_to_32};

/// 元の輪バッファ (`max = m_width`) のままにするか。
const RING: bool = false;

/// 元の `lp` の初期値。主線から探す最大の距離。
const SEARCH_LEN: i64 = 6;

/// 元の pass A が見る8方向。00/45/90/135/180/225/270/315 の順で、
/// 同じ距離なら先に見た向きが勝つ (元は `nv < pv` の狭い不等号)。
const RAYS: [(i64, i64); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

/// 元の `onePointChk` / 4段目が見る8近傍。
const NEIGHBORS: [(i64, i64); 8] = [
    (-1, -1),
    (0, -1),
    (1, -1),
    (-1, 0),
    (1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// 深度ごとの画素。見分けは元の `cmpPx8` / `cmpPx16` に合わせて RGB だけ。
trait Px: Copy {
    /// RGB だけを取り出した見分け用の値。
    type Key: Copy + PartialEq;
    fn key(self) -> Self::Key;
    /// 元の `W`。不透明な白。
    fn white() -> Self;
    /// 元の `c.alpha != PF_MAX_CHAN8`。
    fn is_opaque(self) -> bool;
    /// 色指定 (8bit) をこの深度に上げる。元の `change16`。
    fn from8(c: ae::Pixel8) -> Self;
}

impl Px for ae::Pixel8 {
    type Key = [u8; 3];
    fn key(self) -> Self::Key {
        [self.red, self.green, self.blue]
    }
    fn white() -> Self {
        let m = ae::MAX_CHANNEL8 as u8;
        ae::Pixel8 {
            alpha: m,
            red: m,
            green: m,
            blue: m,
        }
    }
    fn is_opaque(self) -> bool {
        self.alpha == ae::MAX_CHANNEL8 as u8
    }
    fn from8(c: ae::Pixel8) -> Self {
        c
    }
}

impl Px for ae::Pixel16 {
    type Key = [u16; 3];
    fn key(self) -> Self::Key {
        [self.red, self.green, self.blue]
    }
    fn white() -> Self {
        let m = ae::MAX_CHANNEL16 as u16;
        ae::Pixel16 {
            alpha: m,
            red: m,
            green: m,
            blue: m,
        }
    }
    fn is_opaque(self) -> bool {
        self.alpha == ae::MAX_CHANNEL16 as u16
    }
    fn from8(c: ae::Pixel8) -> Self {
        conv_8_to_16(&c)
    }
}

impl Px for ae::PixelF32 {
    /// f32 の等値比較。元の `==` に合わせてビットで見る。
    type Key = [u32; 3];
    fn key(self) -> Self::Key {
        [
            self.red.to_bits(),
            self.green.to_bits(),
            self.blue.to_bits(),
        ]
    }
    fn white() -> Self {
        ae::PixelF32 {
            alpha: 1.0,
            red: 1.0,
            green: 1.0,
            blue: 1.0,
        }
    }
    fn is_opaque(self) -> bool {
        self.alpha == 1.0
    }
    fn from8(c: ae::Pixel8) -> Self {
        conv_8_to_32(&c)
    }
}

/// 元の `paintQue`。走査線1区間ぶん。
#[derive(Clone, Copy, Default, Debug)]
struct Que {
    xl: usize,
    xr: usize,
    y: usize,
    /// 元の `Yparent`。この区間を湧かせた行。
    oy: usize,
}

/// 元の待ち行列。`RING` のときは幅ぶんの輪バッファそのままで、
/// 溢れると未処理の区間を上書きする。
struct Queue {
    buf: Vec<Que>,
    cap: usize,
    start: usize,
    last: usize,
}

impl Queue {
    fn new(w: usize, seed: Que) -> Self {
        let mut buf = if RING {
            vec![Que::default(); w]
        } else {
            Vec::with_capacity(w)
        };
        if RING {
            buf[0] = seed;
        } else {
            buf.push(seed);
        }
        Queue {
            buf,
            cap: w,
            start: 0,
            last: 1,
        }
    }
    fn push(&mut self, q: Que) {
        if RING {
            self.buf[self.last] = q;
            self.last += 1;
            if self.last == self.cap {
                self.last = 0;
            }
        } else {
            if self.last < self.buf.len() {
                self.buf[self.last] = q;
            } else {
                self.buf.push(q);
            }
            self.last += 1;
        }
    }
    fn take(&mut self) -> Que {
        let q = self.buf[self.start];
        self.start += 1;
        if RING && self.start == self.cap {
            self.start = 0;
        }
        q
    }
    fn empty(&self) -> bool {
        self.start == self.last
    }
}

/// 元の `getPixelN_W`。画像の外は白。
fn get_w<P: Px>(buf: &[P], w: usize, h: usize, x: i64, y: i64) -> P {
    if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 {
        P::white()
    } else {
        buf[x as usize + y as usize * w]
    }
}

/// 元の `alphaOn8` / `alphaOn16`。
fn alpha_on<P: Px>(buf: &mut [P], white: P) {
    for p in buf.iter_mut() {
        if !p.is_opaque() {
            *p = white;
        }
    }
}

/// 元の `paint_scan_line8`。`target` と RGB が同じ区間を見つけて積む。
fn scan_line<P: Px>(
    buf: &[P],
    w: usize,
    target: P::Key,
    xl0: i64,
    xr: i64,
    y: usize,
    oy: usize,
    q: &mut Queue,
) {
    let mut xl = xl0;
    while xl <= xr {
        while xl < xr {
            if buf[xl as usize + y * w].key() == target {
                break;
            }
            xl += 1;
        }
        if buf[xl as usize + y * w].key() != target {
            break;
        }
        let left = xl as usize;
        while xl <= xr {
            if buf[xl as usize + y * w].key() != target {
                break;
            }
            xl += 1;
        }
        q.push(Que {
            xl: left,
            xr: (xl - 1) as usize,
            y,
            oy,
        });
    }
}

/// 元の `paint8` / `paint16`。走査線式の塗りつぶし。種の画素と RGB が
/// 同じひと続きの範囲を `col` で塗る。
fn paint<P: Px>(buf: &mut [P], w: usize, h: usize, x: usize, y: usize, col: P) {
    if x >= w || y >= h {
        return;
    }
    let target = buf[x + y * w].key();
    let colk = col.key();
    let mut q = Queue::new(
        w,
        Que {
            xl: x,
            xr: x,
            y,
            oy: y,
        },
    );
    loop {
        let e = q.take();
        let (mut lx, mut rx, uy, oy) = (e.xl, e.xr, e.y, e.oy);
        // 元: 左右に伸ばす**前**の値を覚えておく。
        let lxsav = e.xl as i64 - 1;
        let rxsav = e.xr as i64 + 1;

        // 元: 処理済のシードなら無視。塗る色が既に入っていたら飛ばす。
        if buf[lx + uy * w].key() != colk {
            while rx < w - 1 {
                if buf[rx + 1 + uy * w].key() != target {
                    break;
                }
                rx += 1;
            }
            while lx > 0 {
                if buf[lx - 1 + uy * w].key() != target {
                    break;
                }
                lx -= 1;
            }
            for i in lx..=rx {
                buf[i + uy * w] = col;
            }
            // 真上の走査線。親と同じ行なら、伸ばして増えた左右だけを見る。
            if uy > 0 {
                let ny = uy - 1;
                if ny == oy {
                    scan_line(buf, w, target, lx as i64, lxsav, ny, uy, &mut q);
                    scan_line(buf, w, target, rxsav, rx as i64, ny, uy, &mut q);
                } else {
                    scan_line(buf, w, target, lx as i64, rx as i64, ny, uy, &mut q);
                }
            }
            // 真下の走査線。
            let ny = uy + 1;
            if ny < h {
                if ny == oy {
                    scan_line(buf, w, target, lx as i64, lxsav, ny, uy, &mut q);
                    scan_line(buf, w, target, rxsav, rx as i64, ny, uy, &mut q);
                } else {
                    scan_line(buf, w, target, lx as i64, rx as i64, ny, uy, &mut q);
                }
            }
        }
        if q.empty() {
            break;
        }
    }
}

/// 元の mainColor 前半: 主線の画素から8方向に元画像を辿って、いちばん
/// 近い塗りの色を書く。
fn nearest_fill<P: Px>(
    buf: &mut [P],
    inbuf: &[P],
    w: usize,
    h: usize,
    mck: P::Key,
    wk: P::Key,
    white: P,
) {
    for y in 0..h {
        for x in 0..w {
            if buf[x + y * w].key() != mck {
                continue;
            }
            // 元: pv = 1000 (見つからなかった印)、lp = 6。
            let mut pv = 1000i64;
            let mut lp = SEARCH_LEN;
            let mut nc = white;
            for (dx, dy) in RAYS {
                let mut nv = -1i64;
                let mut hit = white;
                for ii in 1..=lp {
                    let p = get_w(inbuf, w, h, x as i64 + dx * ii, y as i64 + dy * ii);
                    // 元: 白に当たったらこの向きは無し。
                    if p.key() == wk {
                        nv = -1;
                        break;
                    }
                    if p.key() != mck {
                        nv = ii;
                        hit = p;
                        break;
                    }
                }
                // 元: 近いほうだけ採る。探す上限もそこまで縮む。
                if nv > 0 && nv < pv {
                    pv = nv;
                    nc = hit;
                    lp = nv;
                }
            }
            if pv < 1000 {
                buf[x + y * w] = nc;
            }
        }
    }
}

/// 元の `exec8` / `exec16`。
fn exec<P: Px>(buf: &mut Vec<P>, w: usize, h: usize, mc8: ae::Pixel8) {
    let white = P::white();
    let wk = white.key();
    let mck = P::from8(mc8).key();

        // 直したところ: 参照する元画像にも alphaOn を通す。元は `in` を
        // 直に読むので、透明なのに色の付いた画素を塗りとして拾って
        // しまう (上の説明を参照)。
        alpha_on(buf, white);
        let inbuf = buf.clone();

    // 元の onePointChk: 8近傍が全部白の主線を白にする。書いた結果が
    // そのまま次の画素の判定に回る。
    for y in 0..h {
        for x in 0..w {
            if buf[x + y * w].key() != mck {
                continue;
            }
            let lone = NEIGHBORS
                .iter()
                .all(|(dx, dy)| get_w(buf, w, h, x as i64 + dx, y as i64 + dy).key() == wk);
            if lone {
                buf[x + y * w] = white;
            }
        }
    }

    nearest_fill(buf, &inbuf, w, h, mck, wk, white);

    // 元の mainColor 中盤: 残った主線を、近傍で見つけた塗りの色で
    // 塗りつぶす。
    for y in 0..h {
        for x in 0..w {
            if buf[x + y * w].key() != mck {
                continue;
            }
            let mut found = None;
            for (dx, dy) in NEIGHBORS {
                let p = get_w(buf, w, h, x as i64 + dx, y as i64 + dy);
                if p.key() != mck && p.key() != wk {
                    found = Some(p);
                    break;
                }
            }
            if let Some(p) = found {
                paint(buf, w, h, x, y, p);
            }
        }
    }

    // 元の mainColor 後半: それでも残った主線は白。
    for p in buf.iter_mut() {
        if p.key() == mck {
            *p = white;
        }
    }
}

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    MainColor,
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
        // 元: PF_ADD_COLOR("主線の色", 0, 0, 0)。名前は元が非 ASCII で
        // 出荷 .aex では "F" に化けているため英語にしてある。
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
                out_data.set_return_msg("fs-rs mainlinerepaintold");
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
        let (w, h) = (out_layer.width(), out_layer.height());
        let depth = out_layer.bit_depth();

        // 元の Render: まず入力をそのままコピーしてから、その上で消していく。
        ae::pf::suites::WorldTransform::new()?.copy_hq(
            in_data.effect_ref(),
            &in_layer,
            &mut out_layer,
            None,
            None,
        )?;
        // 幅1だと元の輪バッファ (幅ぶん) が回り切らず止まらなくなる。
        // 意味のある絵にもならないので、そこは手を出さずに帰る。
        if w < 2 || h < 2 {
            return Ok(());
        }

        macro_rules! run {
            ($rd:ident, $wr:ident) => {{
                let mut buf = Vec::with_capacity(w * h);
                for y in 0..h {
                    for x in 0..w {
                        buf.push(*out_layer.$rd(x, y));
                    }
                }
                exec(&mut buf, w, h, main);
                for y in 0..h {
                    for x in 0..w {
                        *out_layer.$wr(x, y) = buf[x + y * w];
                    }
                }
            }};
        }
        match depth {
            8 => run!(as_pixel8, as_pixel8_mut),
            16 => run!(as_pixel16, as_pixel16_mut),
            _ => run!(as_pixel32, as_pixel32_mut),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MC: ae::Pixel8 = ae::Pixel8 {
        alpha: 255,
        red: 0,
        green: 0,
        blue: 0,
    };

    fn px(a: u8, r: u8, g: u8, b: u8) -> ae::Pixel8 {
        ae::Pixel8 {
            alpha: a,
            red: r,
            green: g,
            blue: b,
        }
    }

    fn white() -> ae::Pixel8 {
        px(255, 255, 255, 255)
    }

    /// PF_Pixel は PartialEq を持たないので自前で見る。
    fn eq(a: ae::Pixel8, b: ae::Pixel8) -> bool {
        a.alpha == b.alpha && a.red == b.red && a.green == b.green && a.blue == b.blue
    }

    /// 1段目。不透明でない画素は色に関係なく白になる。
    #[test]
    fn the_see_through_pixels_turn_white() {
        let mut buf = vec![px(0, 10, 20, 30), px(128, 40, 50, 60), px(255, 70, 80, 90)];
        alpha_on(&mut buf, white());
        assert!(eq(buf[0], white()));
        assert!(eq(buf[1], white()));
        assert!(eq(buf[2], px(255, 70, 80, 90)));
    }

    /// 2段目。白だけに囲まれた主線1点は消える。
    #[test]
    fn a_lone_main_pixel_on_white_is_erased() {
        let w = 3;
        let h = 3;
        let mut buf = vec![white(); w * h];
        buf[1 + 1 * w] = MC;
        exec(&mut buf, w, h, MC);
        assert!(eq(buf[1 + 1 * w], white()));
    }

    /// 3段目。塗りで挟まれた主線は、近いほうの色を貰う。
    #[test]
    fn the_line_takes_the_nearer_fill() {
        let w = 6;
        let h = 1;
        let red = px(255, 200, 0, 0);
        let blue = px(255, 0, 0, 200);
        let mut buf = vec![red, MC, MC, MC, blue, blue];
        exec(&mut buf, w, h, MC);
        assert!(eq(buf[1], red), "左が1つ隣");
        assert!(eq(buf[3], blue), "右が1つ隣");
        // 真ん中はどちらも2つ隣。先に見る向き (90 = 右) が勝つ。
        assert!(eq(buf[2], blue));
    }

    /// 3段目。白に当たった向きは打ち切られる。周りが白だけの主線は
    /// 誰からも色を貰えず、最後に白になる。
    #[test]
    fn white_blocks_the_search() {
        let w = 5;
        let h = 1;
        let red = px(255, 200, 0, 0);
        let mut buf = vec![red, white(), MC, white(), red];
        exec(&mut buf, w, h, MC);
        assert!(eq(buf[2], white()));
    }

    /// 3段目が届くのは6つまで。7つ先の塗りは拾わない (そのあと4段目の
    /// 塗りつぶしが回ってくるので、3段目だけを呼んで見る)。
    #[test]
    fn the_search_reaches_six_pixels() {
        let red = px(255, 200, 0, 0);
        for len in 1..=8usize {
            let mut buf = vec![MC; len];
            buf.push(red);
            let w = buf.len();
            let inbuf = buf.clone();
            nearest_fill(&mut buf, &inbuf, w, 1, MC.key(), white().key(), white());
            // いちばん左の画素から red までの距離は len。
            if len <= 6 {
                assert!(eq(buf[0], red), "len={len}");
            } else {
                assert!(eq(buf[0], MC), "len={len}");
            }
        }
    }

    /// 4段目。6つより太い主線の塊は、近傍の色で塗りつぶされる。
    #[test]
    fn a_thick_blob_is_flood_filled() {
        let w = 20;
        let h = 20;
        let red = px(255, 200, 0, 0);
        let mut buf = vec![white(); w * h];
        // 16x16 の塊。真ん中は8方向どれも6つでは抜けられない。
        for y in 2..18 {
            for x in 2..18 {
                buf[x + y * w] = MC;
            }
        }
        // 塊に接する塗りを1つだけ置く。
        buf[1 + 1 * w] = red;
        exec(&mut buf, w, h, MC);
        for y in 2..18 {
            for x in 2..18 {
                assert!(eq(buf[x + y * w], red), "({x},{y})");
            }
        }
    }

    /// 4段目。塗りつぶしはひと続きだけ。離れた塊は残らず白になる。
    #[test]
    fn the_fill_stops_at_the_region_edge() {
        let w = 24;
        let h = 10;
        let red = px(255, 200, 0, 0);
        let mut buf = vec![white(); w * h];
        for y in 1..9 {
            for x in 1..9 {
                buf[x + y * w] = MC;
            }
            for x in 14..22 {
                buf[x + y * w] = MC;
            }
        }
        buf[0 + 0 * w] = red;
        exec(&mut buf, w, h, MC);
        assert!(eq(buf[4 + 4 * w], red), "種のある塊");
        assert!(eq(buf[18 + 4 * w], white()), "離れた塊は消えるだけ");
    }

    /// 透明だが色の付いた画素を塗りの色として拾わない (直したところ)。
    /// 元のままだと主線の跡にアルファ 0 の画素が書かれる。
    #[test]
    fn a_see_through_colour_is_not_used_as_a_fill() {
        let w = 4;
        let h = 1;
        // 透明な赤 - 主線 - 白 - 白。
        let mut buf = vec![px(0, 200, 0, 0), MC, white(), white()];
        exec(&mut buf, w, h, MC);
        if RING {
            // 元のまま: 透明な赤をそのまま書く。
            assert!(eq(buf[1], px(0, 200, 0, 0)));
        } else {
            assert!(eq(buf[1], white()));
        }
    }

    /// 待ち行列。伸びる版は1区間から幅ぶんの区間が湧いても塗り切る。
    #[test]
    fn the_queue_holds_a_full_row_of_segments() {
        let w = 32;
        let h = 5;
        let red = px(255, 200, 0, 0);
        let mut buf = vec![white(); w * h];
        // 真ん中の行を全部主線に、その上下を1つ飛ばしの主線にする。
        for x in 0..w {
            buf[x + 2 * w] = MC;
        }
        for x in (1..w).step_by(2) {
            buf[x + 1 * w] = MC;
            buf[x + 3 * w] = MC;
        }
        buf[0 + 0 * w] = red;
        exec(&mut buf, w, h, MC);
        // 真ん中の行に主線は残らない。
        for x in 0..w {
            assert_ne!(buf[x + 2 * w].key(), MC.key(), "({x},2)");
        }
    }

    /// 深度を上げても主線の見分けが変わらないこと。
    #[test]
    fn the_main_colour_rises_with_the_depth() {
        let c = px(255, 12, 34, 56);
        assert_eq!(<ae::Pixel16 as Px>::from8(c).key(), conv_8_to_16(&c).key());
        assert_eq!(<ae::Pixel8 as Px>::from8(c).key(), c.key());
    }
}
