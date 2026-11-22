// ref: https://github.com/bryful/F-s-PluginsProjects/blob/master/Legacy/Project/FsLibrary/FsGraphicsPaint.cpp
//
//! 元の `CFsGraph::paint8/16/32` (塗りつぶし / バケツ) の移植。
//! `Paint` と `PaintMultPoint` が使う。
//!
//! 走査線ごとに種を積む古典的な seed fill。元から写した細かいところ:
//!
//! - 色の比較は **RGB だけ**で、アルファは見ない
//! - 種のキューは**幅ぶんの長さの輪**で、あふれると古いものを黙って
//!   上書きする。込み入った形では塗り残しが出るが、元のとおり
//! - 始点が画面の外なら何もしない
//! - 塗る色がもとの色と (RGB で) 同じなら、最初の種が「処理済み」と
//!   見なされて何も塗らずに終わる
//!
//! 元は種のキューを AE の `new_world` で取っておきながら、解放の条件が
//! 別のメンバ (`m_scanlineWorld.data`) を見ているので**ふつうはワールドが
//! 漏れる**。移植では持ち方そのものが無いので、自然に無くなっている。

/// 1画素。深度に依らず f64 で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Px {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

/// 元の paintQue。
#[derive(Debug, Clone, Copy, Default)]
struct Que {
    xl: i64,
    xr: i64,
    y: i64,
    yp: i64,
}

fn same(a: &Px, b: &Px) -> bool {
    a.r == b.r && a.g == b.g && a.b == b.b
}

/// 元の paint_scan_line。`y` の行の `xl..=xr` から、塗る色と同じ色の
/// 続きを見つけてキューに積む。
fn scan_line(
    data: &[Px],
    w: i64,
    buf: &mut [Que],
    last: &mut usize,
    target: &Px,
    mut xl: i64,
    xr: i64,
    y: i64,
    oy: i64,
) {
    let at = |x: i64| -> Px { data[(x + y * w) as usize] };
    let n = buf.len();
    while xl <= xr {
        while xl < xr {
            if same(&at(xl), target) {
                break;
            }
            xl += 1;
        }
        if !same(&at(xl), target) {
            break;
        }
        buf[*last].xl = xl;
        while xl <= xr {
            if !same(&at(xl), target) {
                break;
            }
            xl += 1;
        }
        buf[*last].xr = xl - 1;
        buf[*last].y = y;
        buf[*last].yp = oy;
        *last += 1;
        if *last == n {
            *last = 0;
        }
    }
}

/// 元の paint8/16/32。`(x, y)` と地続きで同じ色のところを `col` で塗る。
pub fn bucket(data: &mut [Px], w: i64, h: i64, x: i64, y: i64, col: Px) {
    if x < 0 || y < 0 || x >= w || y >= h {
        return;
    }
    // 元: キューは幅ぶんの長さ。あふれると古いものを上書きする。
    let n = w.max(1) as usize;
    let mut buf = vec![Que::default(); n];
    let mut start = 0usize;
    let mut last = 1usize;
    buf[0] = Que {
        xl: x,
        xr: x,
        y,
        yp: y,
    };
    let target = data[(x + y * w) as usize];

    loop {
        let q = buf[start];
        let (mut lx, mut rx) = (q.xl, q.xr);
        let (mut uy, mut dy, oy) = (q.y, q.y, q.yp);
        let lxsav = lx - 1;
        let rxsav = rx + 1;
        start += 1;
        if start == n {
            start = 0;
        }
        // 元: 処理済みの種なら飛ばす (do-while の条件へ戻る)。
        if !same(&data[(lx + uy * w) as usize], &col) {
            // 右の境を探す。
            while rx < w - 1 {
                if !same(&data[((rx + 1) + uy * w) as usize], &target) {
                    break;
                }
                rx += 1;
            }
            // 左の境を探す。
            while lx > 0 {
                if !same(&data[((lx - 1) + uy * w) as usize], &target) {
                    break;
                }
                lx -= 1;
            }
            // lx..=rx を塗る。
            for i in lx..=rx {
                data[(i + uy * w) as usize] = col;
            }
            // 真上の走査線。
            uy -= 1;
            if uy >= 0 {
                if uy == oy {
                    scan_line(data, w, &mut buf, &mut last, &target, lx, lxsav, uy, uy + 1);
                    scan_line(data, w, &mut buf, &mut last, &target, rxsav, rx, uy, uy + 1);
                } else {
                    scan_line(data, w, &mut buf, &mut last, &target, lx, rx, uy, uy + 1);
                }
            }
            // 真下の走査線。
            dy += 1;
            if dy < h {
                if dy == oy {
                    scan_line(data, w, &mut buf, &mut last, &target, lx, lxsav, dy, dy - 1);
                    scan_line(data, w, &mut buf, &mut last, &target, rxsav, rx, dy, dy - 1);
                } else {
                    scan_line(data, w, &mut buf, &mut last, &target, lx, rx, dy, dy - 1);
                }
            }
        }
        if start == last {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(w: usize, h: usize, s: &[&str]) -> Vec<Px> {
        // '.' = 白、'#' = 黒。
        let mut v = Vec::with_capacity(w * h);
        for row in s {
            for c in row.chars() {
                let g = if c == '#' { 0.0 } else { 255.0 };
                v.push(Px {
                    a: 255.0,
                    r: g,
                    g,
                    b: g,
                });
            }
        }
        assert_eq!(v.len(), w * h);
        v
    }

    fn red() -> Px {
        Px {
            a: 255.0,
            r: 255.0,
            g: 0.0,
            b: 0.0,
        }
    }

    fn show(data: &[Px], w: usize, h: usize) -> Vec<String> {
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| {
                        let p = data[x + y * w];
                        if p == red() {
                            'R'
                        } else if p.r == 0.0 {
                            '#'
                        } else {
                            '.'
                        }
                    })
                    .collect()
            })
            .collect()
    }

    /// 囲まれた中だけ塗る。
    #[test]
    fn the_fill_stays_inside_the_outline() {
        let s = ["######", "#....#", "#....#", "######"];
        let mut d = grid(6, 4, &s);
        bucket(&mut d, 6, 4, 2, 2, red());
        assert_eq!(
            show(&d, 6, 4),
            vec!["######", "#RRRR#", "#RRRR#", "######"]
        );
    }

    /// 切れ目から漏れる。
    #[test]
    fn the_fill_leaks_through_a_gap() {
        let s = ["######", "#....#", "#....#", "###.##"];
        let mut d = grid(6, 4, &s);
        bucket(&mut d, 6, 4, 2, 2, red());
        assert_eq!(
            show(&d, 6, 4),
            vec!["######", "#RRRR#", "#RRRR#", "###R##"]
        );
    }

    /// 始点が画面の外なら何もしない。
    #[test]
    fn a_seed_outside_does_nothing() {
        let s = ["....", "....."];
        let _ = s;
        let mut d = grid(4, 2, &["....", "...."]);
        let before = d.clone();
        bucket(&mut d, 4, 2, -1, 0, red());
        bucket(&mut d, 4, 2, 0, 5, red());
        assert_eq!(d, before);
    }

    /// 塗る色がもとの色と同じなら何も起きない。
    #[test]
    fn filling_with_the_same_colour_is_a_no_op() {
        let mut d = grid(4, 2, &["....", "...."]);
        let white = Px {
            a: 255.0,
            r: 255.0,
            g: 255.0,
            b: 255.0,
        };
        let before = d.clone();
        bucket(&mut d, 4, 2, 1, 1, white);
        assert_eq!(d, before);
    }

    /// U 字の内側も回り込んで塗る。
    #[test]
    fn the_fill_goes_around_a_u_shape() {
        let s = [
            "########",
            "#......#",
            "#.####.#",
            "#.#..#.#",
            "#.####.#",
            "#......#",
            "########",
        ];
        let mut d = grid(8, 7, &s);
        bucket(&mut d, 8, 7, 1, 1, red());
        let out = show(&d, 8, 7);
        // 外周の輪は全部塗られ、内側の小部屋は残る。
        assert_eq!(out[1], "#RRRRRR#");
        assert_eq!(out[3], "#R#..#R#");
        assert_eq!(out[5], "#RRRRRR#");
    }

    /// アルファは見ない (RGB が同じなら地続き)。
    #[test]
    fn the_alpha_is_ignored() {
        let mut d = grid(4, 1, &["...."]);
        d[2].a = 0.0;
        bucket(&mut d, 4, 1, 0, 0, red());
        assert!(d.iter().all(|p| p.r == 255.0 && p.g == 0.0));
    }
}
