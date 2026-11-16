# 移植の進み具合と残りの仕分け

元: [F's Plugins](https://github.com/bryful/F-s-PluginsProjects) の `Legacy/Project`。

移植した 56 本はすべて、出荷されている .aex とパラメータ構成 (型・範囲・既定値・色・
ポップアップ項目) が一致し、[AEXCompat](https://github.com/onmokoworks/AEXCompat) で
画素を突き合わせてある。意図して元と違えているのはバグ修正ぶんだけ。

以下は**まだ移植していない 55 本**の仕分け。`lines` は .cpp の行数、
`oracle` は元と画素比較する手段:

| oracle | 意味 |
|---|---|
| `smart` | AEXCompat の smart 経路で元が描ける (既定値での比較) |
| `classic` | smart では落ちるが classic 経路なら描ける (パラメータを振れる) |
| `none` | どちらの経路でも元が描けない。移植しても突き合わせられない |
| `no-binary` | 出荷 .aex が無い。古い作業用プロジェクトと思われる |

`needs` はソースを grep して出した必要な部品:
`rand` 乱数列 / `world` 一時ワールドやハンドル / `neighbor` 近傍参照 /
`minimax` 最大最小フィルタ / `blur` ぼかし / `draw` 線や矩形の描画 /
`point` 座標 / `angle` 角度 / `time` フレーム依存 / `suite` AE のカラースイート /
`preset` .pref の読み書き / `button` ボタン。

## A: 小さくて部品も揃っている

**全部移植した。** 次に着手するのは B 群。

## B: 中くらい。既存の部品で足りるが読む量が増える

| project | lines | oracle | needs | 双子 |
|---|---|---|---|---|
| UsedColorList | 210 | `smart` | `world` `neighbor` `point` | - |
| Max | 242 | `smart` | `minimax` | - |
| ChromaticAberration | 412 | `smart` | `world` `neighbor` `point` | - |
| RandomMosaic | 513 | `smart` | `rand` `time` | - |
| RandomShift | 528 | `smart` | `rand` `neighbor` `draw` `time` | - |
| ScreenShakeDir | 546 | `classic` | `rand` `neighbor` `angle` `time` | ScreenShakeMM |
| RandomLineNoise | 557 | `smart` | `rand` `neighbor` `time` | - |
| ScreenShakeMM | 562 | `classic` | `rand` `neighbor` `angle` `time` | ScreenShakeDir |
| sputteringCircle | 596 | `smart` | `neighbor` `draw` `point` | - |
| ChannelNoise | 613 | `smart` | `rand` `time` | - |
| SparkRing | 634 | `smart` | `rand` `neighbor` `point` `angle` `time` | - |
| RandomMosaic2nd | 655 | `smart` | `rand` `time` | - |
| SparkGrand | 665 | `smart` | `rand` `neighbor` `point` `angle` `time` | - |
| GuideFrame | 669 | `smart` | `neighbor` `draw` `point` | - |
| Spark | 694 | `smart` | `rand` `neighbor` `point` `angle` `time` | SparkStorm |
| SparkStorm | 701 | `smart` | `rand` `neighbor` `point` `angle` `time` | Spark |
| SparkMult | 773 | `smart` | `rand` `neighbor` `point` `angle` `time` | - |
| grayToWaveLine | 945 | `classic` | `world` `neighbor` `draw` `point` | - |
| AnimatedNoise | 969 | `smart` | `rand` `neighbor` `time` | - |
| ColorMatKey | 1088 | `smart` | `world` `neighbor` `suite` `time` | - |
| VideoGrid | 1123 | `smart` | `neighbor` | - |
| MaxFast | 1132 | `smart` | `world` `neighbor` | - |
| colorThreshold | 1236 | `smart` | `neighbor` | - |
| Filter | 1313 | `classic` | `time` | - |
| LineDetection | 1337 | `smart` | `neighbor` | - |
| Max_Kasumi | 1375 | `smart` | `world` `neighbor` | - |
| MainLineRepaint_old | 1378 | `classic` | `neighbor` `draw` `time` | - |
| MainLineRepaint | 1396 | `smart` | `neighbor` | - |
| TouchDrawCenter | 1809 | `smart` | `rand` `neighbor` `point` | - |

## C: 大物。乱数 + 幾何 + ワールドが絡む

| project | lines | oracle | needs | 双子 |
|---|---|---|---|---|
| CellGrad | 455 (+919)** | `smart` | `world` `draw` `minimax` `angle` | - |
| SelectedBlur | 1130 | `smart` | `world` `neighbor` `blur` | - |
| EdgeBlur | 1181 | `smart` | `world` `neighbor` `blur` | - |
| IroToreAdjacent | 1827 | `smart` | `world` `neighbor` `blur` | - |
| MaxBlur | 2009 | `smart` | `world` `neighbor` | InnerFlare_old, InnerFlareRot_old |
| Star | 2122 | `smart` | `neighbor` `angle` | - |
| TouchDrawStraght | 2213 | `smart` | `rand` `neighbor` `blur` `angle` | TouchDraw |
| TouchDraw | 2358 | `smart` | `rand` `neighbor` `blur` `point` | TouchDrawStraght |
| NamiGarasu | 2369 | `smart` | `rand` `neighbor` `angle` `time` | - |
| InnerFlareRot_old | 2373 | `smart` | `world` `neighbor` `angle` | InnerFlare_old, MaxBlur |
| PaperPlaneGetWeightInfo | 2413 | `classic` | `world` `neighbor` `draw` | - |
| IroTore | 3013 | `smart` | `world` `neighbor` `blur` `angle` `suite` | - |
| Flare | 3274 | `classic` | `world` `neighbor` | - |
| ShitsuKanBlur | 3510 | `smart` | `rand` `world` `neighbor` `angle` `time` | - |
| Star_Colorful | 4271 | `smart` | `world` `neighbor` `angle` `suite` `time` | - |
| Thin | 5895 | `smart` | `world` `neighbor` | - |

## D: 元と画素比較できない (出荷バイナリが無い / AEXCompat で描けない)

| project | lines | oracle | needs | 双子 |
|---|---|---|---|---|
| ChannelBlur | 263 | `no-binary` | - | - |
| Frame | 324 | `none` | `draw` | FsSSFrame |
| FsSSFrame | 324 | `none` | `draw` | Frame |
| Paint | 387 | `none`* | `point` | - |
| PaintMultPoint | 398 | `none`* | `point` | - |
| Spark_Test | 413 | `no-binary` | `rand` `neighbor` `point` | - |
| Extract_Edge | 416 | `no-binary` | `neighbor` | - |
| Shine | 1148 | `none` | `rand` `world` `neighbor` `point` | ShineParallel |
| ShineParallel | 1165 | `no-binary` | `rand` `world` `neighbor` `angle` | Shine |
| InnerFlare_old | 2313 | `none` | `world` `neighbor` | InnerFlareRot_old, MaxBlur |

\* `Paint` / `PaintMultPoint` は塗りつぶし (バケツ) が本体だが、それを
元と突き合わせる手段が無い。**どちらも元の classic の Render が
パラメータを読むだけで `Exec()` を呼んでいない** (`Paint` は入力のコピーまで、
`PaintMultPoint` は出力に何も書かない)。塗りつぶしは smart 経路にしかなく、
その smart 経路は `Paint` では AEXCompat で落ち、`PaintMultPoint` では
チェックボックスが既定で全部 OFF なので塗りつぶしに入らない。
パラメータを振れるのは classic 経路だけなので、どう組んでも塗りつぶしの
画素を比較できない。

\*\* `CellGrad` の .cpp は 455 行だが、処理の本体は同じフォルダの
`FWorldGrad.h` (729 行: 色の選択・マスク・最大最小・走査矩形) と
`FRectGrad.h` (190 行: 角度と伸ばし量から回転した矩形を出す) にあり、
どちらも libs に相当する部品が無い。実際の大きさは 1400 行ほど。

## 双子について

ソースがほぼ同じ組がいくつかある (`Unmult_RG` と `Unmult_KNSW`、`Tone` と `Toner` は
実際に #include 行以外同じだった)。片方を移植すればもう片方はほぼ無料なので、
組になっているものは続けて片付けるのが得。

## 出荷バイナリとソースの対応

出荷されている .aex は 107 本あり、**すべてソースが `Legacy/Project` にある**
(プロジェクト名が `_old` / `_Fake` 付きで出荷名と違うものがあるだけ)。
逆にソースだけあって出荷されていないものが 4 本ある
(`ChannelBlur` / `ShineParallel` / `Spark_Test` / `Extract_Edge`)。

## 部品の状況

`libs` に入っている再利用できる部品:

- 丸め・深度変換 (`utils`): 元の `Round*` が切り捨てであること、
  `FS_CONVERT8TO32` が小数4桁に量子化すること、double 版が要る場所まで含めて合わせてある
- 最大最小フィルタ (`fs_graphics8::minimax8` / `minimax_alpha8`、f32 版も)
- ぼかし (`fs_graphics8::gblur8`)
- 乱数 (`fs_graphics::XorShift`: 整数版・double 版・`F_RAND_D1` 相当。種まきも元と同じ)
- HLS (`hls`) と Lab (`lab`)
- ブレンドモード各種 (`fs_graphics8`)
- 飛び散りのスプライト表と描き方 (`sputtering`: 14x14 を 60 種類、
  大きさごとの引き表、深度ごとの描画 `sput_draw`)。sputteringAlpha /
  sputteringCircle も同じものを使う
- 16bit の画素と合成 (`fs_graphics8::Px16` / `pixel_blend16`)
- 角度と距離から XY のずれを出す `utils::rot_length` (元の CRotCalc)

まだ無くて、残りの移植で必要になるもの:

- 円の描画 (`Circle` 系) — sputteringCircle / GuideFrame など
- カスタム UI (`PF_ADD_NULL` + 描画イベント) — 今のところ残りには見当たらない
- `.pref` の読み書き (TargetGrad 系のボタン。描画には影響しない)
