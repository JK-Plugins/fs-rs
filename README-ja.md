# fs-rs

[English](./README.md) | [日本語](./README-ja.md)

# 概要

このプラグインは [F's Plugin](https://github.com/bryful/F-s-PluginsProjects) を Rust に移植したものです。
F's Plugin の機能を Rust で再実装することで、macOSの対応と高速化を目指しています。

**ダウンロード：https://jk-plugins.github.io/JK-Plugins/**

## プラグイン一覧

| プラグイン | バージョン | 説明 |
|---|---|---|
| [JK Alpha Fix Fs](alphafix/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/alphafix-fs-v0.0.1) | 入力を背景色の上に合成して不透明にする |
| [JK Alpha Hyperbolic Fs](alphahyperbolic/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/alphahyperbolic-fs-v0.0.1) | アルファに双曲線カーブをかける |
| [JK Alpha Threshold Fs](alphathreshold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/alphathreshold-fs-v0.0.1) | アルファをしきい値で二値化する |
| [JK Animated Noise Fs](animatednoise/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/animatednoise-fs-v0.0.1) | アナログ映像のようなノイズを2段で乗せる |
| [JK CC Plus Fs](ccplus/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/ccplus-fs-v0.0.1) | 輝度から2段グラデーションの色を引いて着色する |
| [JK Cell Grad Fs](cellgrad/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/cellgrad-fs-v0.0.1) | 選んだ色のかたまりの外接矩形にグラデーションを塗る |
| [JK Cell Line Eraser Fs](celllineeraser/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/celllineeraser-fs-v0.0.1) | セル画の線画を消す |
| [JK Channel Blur Fs](channelblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/channelblur-fs-v0.0.1) | 元が未完成のため、入力をそのまま通す |
| [JK Channel Noise Fs](channelnoise/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/channelnoise-fs-v0.0.1) | RGB それぞれに別の幅のノイズを足す |
| [JK Channel Shift Fs](channelshift/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/channelshift-fs-v0.0.1) | チャンネルごとに別の向き・距離から画素を拾う |
| [JK Chromatic Aberration Fs](chromaticaberration/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/chromaticaberration-fs-v0.0.1) | 中心からの距離に RGB 別の倍率をかけて色収差を作る |
| [JK Color Change Eng Fs](colorchangeeng/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangeeng-fs-v0.0.1) | ColorChange の英語名版 |
| [JK Color Change From Point Fs](colorchangefrompoint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangefrompoint-fs-v0.0.1) | 指定した座標の色と同じ画素を置き換える |
| [JK Color Change Fs](colorchange/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchange-fs-v0.0.1) | 指定した色（最大8組）を別の色に置き換える |
| [JK Color Change Simple Fs](colorchangesimple/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangesimple-fs-v0.0.1) | 指定した1色を別の色に置き換える |
| [JK Color Key Fs](colorkey/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorkey-fs-v0.0.1) | 指定した色を透明にする |
| [JK Color Mat Key Fs](colormatkey/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colormatkey-fs-v0.0.1) | 指定した色（最大4色）に近い画素だけを残す |
| [JK Color Threshold Fs](colorthreshold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorthreshold-fs-v0.0.1) | アルファで切り出した中を決めた色のどれかに丸める |
| [JK Create Alpha Fs](createalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/createalpha-fs-v0.0.1) | 明るさからアルファを作る（黒が透明） |
| [JK Edge Blur Fs](edgeblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/edgeblur-fs-v0.0.1) | アルファだけを内側へ縮めてからぼかす |
| [JK Edge Line Fs](edgeline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/edgeline-fs-v0.0.1) | 対象色の周りに見本色があれば枠の色で塗る |
| [JK Edge Line Hi Fs](edgelinehi/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/edgelinehi-fs-v0.0.1) | 対象色から光線を伸ばし、見本色に当たった画素を塗る |
| [JK Exps Colors Fs](expscolors/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/expscolors-fs-v0.0.1) | 色を10個並べただけのパラメータ見本 |
| [JK Exps Pos Fs](expspos/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/expspos-fs-v0.0.1) | 座標を10個並べただけのパラメータ見本 |
| [JK Extract Edge Fs](extractedge/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/extractedge-fs-v0.0.1) | 明るさの段差から輪郭の強さを出す |
| [JK Extract Hi Fs](extracthi/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/extracthi-fs-v0.0.1) | 選んだ成分をしきい値カーブに通してアルファに掛ける |
| [JK Fill Color Fs](fillcolor/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/fillcolor-fs-v0.0.1) | RGB を指定色で塗る |
| [JK Filter Fs](filter/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/filter-fs-v0.0.1) | 抜き出し・膨張・ぼかし・合成を4組まで重ねるグロー |
| [JK Flare Fs](flare/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/flare-fs-v0.0.1) | 明るいところからにじむ光を作る |
| [JK Frame Fs](frame/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/frame-fs-v0.0.1) | 画面の真ん中に枠を描き、外を半透明の色で覆う |
| [JK Fs SS Frame](fsssframe/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/fsssframe-fs-v0.0.1) | F's Frame の双子（ソースは同一） |
| [JK Gray To Colorize Fs](graytocolorize/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytocolorize-fs-v0.0.1) | 輝度を5段に落として段ごとの色で塗る |
| [JK Gray To Countour Line Fs](graytocountourline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytocountourline-fs-v0.0.1) | 輝度を段階数だけの灰色に丸める |
| [JK gray To Wave Line Fs](graytowaveline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytowaveline-fs-v0.0.1) | 明るさを折れ線グラフの高さに読み替えて描く |
| [JK Graytone To Colorize Fs](graytonetocolorize/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/graytonetocolorize-fs-v0.0.1) | 選んだチャンネルのグレーで9色のグラデーションを引く |
| [JK Guide Frame Fs](guideframe/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/guideframe-fs-v0.0.1) | 2点で決まる長方形の枠を描く絵コンテ用の補助 |
| [JK HLS Reverse Fs](hlsreverse/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/hlsreverse-fs-v0.0.1) | HLS にして色相・明度・彩度を個別に反転する |
| [JK Inner Flare Fs](innerflare/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/innerflare-fs-v0.0.1) | 絵の内側にそのかたちの光をにじませる |
| [JK Inner Flare Rot Fs](innerflarerot/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/innerflarerot-fs-v0.0.1) | InnerFlare の向きつきの版 |
| [JK Iro Tore Adjacent Fs](irotoreadjacent/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/irotoreadjacent-fs-v0.0.1) | 隣接色に接した主線だけを別の色に置き換える |
| [JK Iro Tore Fs](irotore/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/irotore-fs-v0.0.1) | セル画の主線を消して、まわりの塗りで埋める |
| [JK Line Detection Fs](linedetection/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/linedetection-fs-v0.0.1) | 線になりそうなところを拾って指定色で塗る |
| [JK Line Trace Fs](linetrace/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/linetrace-fs-v0.0.1) | 色相で振り分けた色ごとのしきい値で線を塗る |
| [JK Main Line Repaint Fs](mainlinerepaint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/mainlinerepaint-fs-v0.0.1) | 主線を消して両側の塗りで埋め直す |
| [JK Main Line Repaint Old Fs](mainlinerepaintold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/mainlinerepaintold-fs-v0.0.1) | 同じ主線消しの古い版（やり方が別） |
| [JK Mask From RGB Fs](maskfromrgb/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maskfromrgb-fs-v0.0.1) | 1チャンネルをアルファにし、RGB は白で塗る |
| [JK Mask From RGB Multi Fs](maskfromrgbmulti/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maskfromrgbmulti-fs-v0.0.1) | R/G/B/Y/M/C それぞれの色らしさをアルファに足す |
| [JK Max Blur Fs](maxblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maxblur-fs-v0.0.1) | 膨張（収縮）・ぼかし・アルファのカーブを一度に掛ける |
| [JK Max Fast Fs](maxfast/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maxfast-fs-v0.0.1) | van Herk / Gil-Werman 法で半径に依らず速く膨張・収縮する |
| [JK Max Fs](max/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/max-fs-v0.0.1) | 最大・最小フィルタで膨張・収縮させる |
| [JK Max Kasumi Fs](maxkasumi/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/maxkasumi-fs-v0.0.1) | 隣が明るければその明るさに少しだけ寄せる |
| [JK Mosaic Fs](mosaic/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/mosaic-fs-v0.0.1) | size x size のブロックごとに平均色で塗る |
| [JK Nami Garasu Fs](namigarasu/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/namigarasu-fs-v0.0.1) | すりガラス越しに見たようにゆがませる |
| [JK Noise Hi Lo Alpha Fs](noisehiloalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/noisehiloalpha-fs-v0.0.1) | アルファの暗部・中間・明部に別の強さのノイズを乗せる |
| [JK Noise Hi Lo RGB Fs](noisehilorgb/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/noisehilorgb-fs-v0.0.1) | 輝度の暗部・中間・明部に別の強さのノイズを乗せる |
| [JK Optical Diffusion Fs](opticaldiffusion/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/opticaldiffusion-fs-v0.0.1) | 抜き出した光をぼかして合成する光学ディフュージョン |
| [JK Out Line Fs](outline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/outline-fs-v0.0.1) | アルファを収縮させた差を輪郭の帯にする |
| [JK Paint Fs](paint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paint-fs-v0.0.1) | 指定した1点から地続きの同じ色を塗りつぶす |
| [JK Paint Mult Point Fs](paintmultpoint/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paintmultpoint-fs-v0.0.1) | Paint の多点版（位置と色を8組まで） |
| [JK Paper Plane Get Weight Fs](paperplanegetweight/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paperplanegetweight-fs-v0.0.1) | 紙飛行機の重さのデータを RGB に書き込む |
| [JK Paper Plane Weight Fs](paperplaneweight/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/paperplaneweight-fs-v0.0.1) | RGB に埋めた重さを読み戻し、合計と重心を出す |
| [JK Pixel Extend Fs](pixelextend/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelextend-fs-v0.0.1) | 対象色の画素を8方向それぞれの長さだけ引き延ばす |
| [JK Pixel Replace Fs](pixelreplace/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelreplace-fs-v0.0.1) | 24組の対象色と置換色を上から順に当てはめる |
| [JK Pixel Selector Fs](pixelselector/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelselector-fs-v0.0.1) | 指定した色の画素だけを残す |
| [JK Posterization 8bit Fs](posterization8bit/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/posterization8bit-fs-v0.0.1) | 各チャンネルを level 段階に量子化する |
| [JK Premultiply Fs](premultiply/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/premultiply-fs-v0.0.1) | マット合成とストレートを相互変換する |
| [JK Random Line Noise Fs](randomlinenoise/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randomlinenoise-fs-v0.0.1) | 短い棒を乱数でばらまき、その下の RGB にノイズを足す |
| [JK Random Mosaic 2nd Fs](randommosaic2nd/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randommosaic2nd-fs-v0.0.1) | 桝目に区切り、桝ごとに決めた確率でモザイクにする |
| [JK Random Mosaic Fs](randommosaic/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randommosaic-fs-v0.0.1) | 四角いモザイクをフレームごとの乱数でばらまく |
| [JK Random Shift Fs](randomshift/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/randomshift-fs-v0.0.1) | 長方形の切れ端を何度も別の場所に貼り直す |
| [JK Rgb To Alpha Fs](rgbtoalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/rgbtoalpha-fs-v0.0.1) | 輝度をアルファに移し、RGB は指定色で塗る |
| [JK RGBA Control Fs](rgbacontrol/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/rgbacontrol-fs-v0.0.1) | R/G/B/A をそれぞれ持ち上げ下げする |
| [JK Rim Fill Fs](rimfill/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/rimfill-fs-v0.0.1) | まわりの透明な画素を塗って縁取りを足す |
| [JK Scanline Fs](scanline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/scanline-fs-v0.0.1) | 走査線状の帯を並べ、奇数列と偶数列で別々に掛ける |
| [JK Screen Shake Dir Fs](screenshakedir/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/screenshakedir-fs-v0.0.1) | 画面全体を乱数で決めた向きと量だけずらす |
| [JK Screen Shake MM Fs](screenshakemm/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/screenshakemm-fs-v0.0.1) | 画面全体を乱数で決めた向きと量だけずらす |
| [JK Select Color Fs](selectcolor/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/selectcolor-fs-v0.0.1) | 8組の対象色に一致した画素を透明にする |
| [JK Selected Blur Fs](selectedblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/selectedblur-fs-v0.0.1) | 指定した色（最大8色）のところだけをぼかす |
| [JK Shine Fs](shine/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/shine-fs-v0.0.1) | 指定した点から外へ向かう光条を描く |
| [JK Shine Parallel Fs](shineparallel/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/shineparallel-fs-v0.0.1) | Shine の平行版 |
| [JK Shitsu Kan Blur Fs](shitsukanblur/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/shitsukanblur-fs-v0.0.1) | ベタ塗りに決まった向きの光を足して質感を出す |
| [JK Smoke Threshold Fs](smokethreshold/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/smokethreshold-fs-v0.0.1) | アルファを二値化し、輝度で4帯に振り分けて塗る |
| [JK Spark Fs](spark/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/spark-fs-v0.0.1) | 2点のあいだにギザギザの稲妻を描く |
| [JK Spark Grand Fs](sparkgrand/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkgrand-fs-v0.0.1) | 1点から線分のどこかへ落ちる太い稲妻を描く |
| [JK Spark Mult Fs](sparkmult/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkmult-fs-v0.0.1) | 最大7点を結んだ折れ線をギザギザにして描く |
| [JK Spark Ring Fs](sparkring/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkring-fs-v0.0.1) | 楕円の上の点をギザギザの線でつないで輪にする |
| [JK Spark Storm Fs](sparkstorm/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparkstorm-fs-v0.0.1) | Spark の双子（描き方はそっくり同じ） |
| [JK Spark Test Fs](sparktest/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sparktest-fs-v0.0.1) | Spark 系の部品の試し描き用 |
| [JK Sputtering Alpha Fs](sputteringalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringalpha-fs-v0.0.1) | アルファの縁と中身に別の確率でかたまりを散らす |
| [JK Sputtering Circle Fs](sputteringcircle/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringcircle-fs-v0.0.1) | 1点のまわりの円（楕円）の中にかたまりを散らす |
| [JK Sputtering Rect Fs](sputteringrect/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringrect-fs-v0.0.1) | 四角い範囲の中に絵の具を飛ばしたようなかたまりを重ねる |
| [JK Sputtering Splash Fs](sputteringsplash/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/sputteringsplash-fs-v0.0.1) | アルファの縁から絵の具を飛ばしたように散らす |
| [JK Star Colorful Fs](starcolorful/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/starcolorful-fs-v0.0.1) | 明るいところの縁から十字と斜めの光を伸ばす |
| [JK Star Fs](star/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/star-fs-v0.0.1) | 明るいところから4方向へ光条を伸ばすクロスフィルター |
| [JK Target Grad Fs](targetgrad/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/targetgrad-fs-v0.0.1) | 対象色の外接矩形に沿った直線グラデーションをアルファに描く |
| [JK Target Grad Radical Fs](targetgradradical/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/targetgradradical-fs-v0.0.1) | 対象色を囲む楕円状のグラデーションをアルファに描く |
| [JK Thin Fs](thin/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/thin-fs-v0.0.1) | セル画の主線を細くする |
| [JK To Gray Fs](togray/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/togray-fs-v0.0.1) | 8通りのグレー化のどれかで作ったグレーと元の色を混ぜる |
| [JK Tone Fs](tone/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/tone-fs-v0.0.1) | 輝度から影・中間・ハイライトの色を引いて混ぜる |
| [JK Toner Fs](toner/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/toner-fs-v0.0.1) | Tone の双子（ソースは同一） |
| [JK Touch Draw Center Fs](touchdrawcenter/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/touchdrawcenter-fs-v0.0.1) | 四辺の外から中心へ向かう短い線で縁取りを描く |
| [JK Touch Draw Fs](touchdraw/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/touchdraw-fs-v0.0.1) | 対象の縁から中心へ向かう短い線をたくさん引く |
| [JK Touch Draw Straght Fs](touchdrawstraght/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/touchdrawstraght-fs-v0.0.1) | 対象の縁から指定した向きへ短い線をたくさん引く |
| [JK Unmult KNSW Fs](unmultknsw/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/unmultknsw-fs-v0.0.1) | プリマルチプライ後、RGB の最大値をアルファに入れ直す |
| [JK Unmult RG Fs](unmultrg/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/unmultrg-fs-v0.0.1) | プリマルチプライ後、RGB の最大値をアルファに入れ直す |
| [JK Used Color List Fs](usedcolorlist/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/usedcolorlist-fs-v0.0.1) | 使われている色を数え、多い順に桝目で並べる |
| [JK Video Grid Fs](videogrid/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/videogrid-fs-v0.0.1) | 桝目の辺に明暗を付けて立体的な格子を描く |
| [JK Video Line 2nd Fs](videoline2nd/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/videoline2nd-fs-v0.0.1) | 走査線状の縞を重ねて明るく（暗く）する |
| [JK Video Line Fs](videoline/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/videoline-fs-v0.0.1) | 走査線状の縞を重ねて明るく（暗く）する |
| [JK White In Out Fs](whiteinout/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/whiteinout-fs-v0.0.1) | 白に向かって飛ばす／白から戻す |
| [JK Yuv Control Fs](yuvcontrol/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/yuvcontrol-fs-v0.0.1) | YUV にして Y/U/V を個別に持ち上げ下げする |

プラグイン名を押すとパラメータの説明、バージョンを押すとその版のファイル（Windows：`.aex`、macOS：`.plugin.zip`）に飛びます。

## インストール

[最新のリリース](https://github.com/JK-Plugins/fs-rs/releases/latest) からダウンロードします。After Effects は終了しておいてください。

- **Windows**：`JK-Plugins-Fs-<日付>-windows-setup.exe` を実行する。「Windows によって PC が保護されました」と出たら「詳細情報」→「実行」（署名していないため）。
  `C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\JK-Plugins` に入ります。
  アンインストールは「設定 → アプリ → インストールされているアプリ」の **JK-Plugins Fs (After Effects)** から。
  インストーラーを使わないときは `JK-Plugins-Fs-<日付>-windows.zip` を解凍し、中の `JK-Plugins` フォルダーを `MediaCore` にコピーします。
- **macOS**：`JK-Plugins-Fs-<日付>-macos.pkg` を開く。開けないときは システム設定 → プライバシーとセキュリティ の「このまま開く」（署名していないため）。
  `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/JK-Plugins` に入ります。

アップデートは、新しいインストーラーをそのまま実行すれば上書きされます。

## 前提条件

### **LLVM**

1. 以下のページから LLVM をダウンロードしてインストールしてください  
   <https://github.com/llvm/llvm-project/releases>

2. インストール後、`LIBCLANG_PATH` に **LLVM の `bin` ディレクトリ** を設定します。

例: LLVM を `/path/to/llvm` にインストールした場合

```bash
# macOS
export LIBCLANG_PATH=/path/to/llvm/bin

# Windows (PowerShell)
setx LIBCLANG_PATH "C:\path\to\llvm\bin"
```

3. `PATH` に LLVM のバイナリを追加します。

```bash
# macOS
export PATH=/path/to/llvm/bin:$PATH

# Windows (PowerShell)
setx PATH "C:\path\to\llvm\bin;$env:PATH"
# ※ LLVM インストーラーを使った場合は自動で設定されることがあります
```

### **Adobe After Effects SDK (May 2023)**

1. SDK をダウンロード
   <https://console.adobe.io/downloads/ae>
2. 任意の場所に展開し、環境変数 AESDK_ROOT を設定します。

```bash
# macOS
export AESDK_ROOT=/path/to/AfterEffects_SDK

# Windows (PowerShell)
setx AESDK_ROOT "C:\path\to\AfterEffects_SDK"
```

### **Rust**

[rustup](https://rustup.rs/) を使って Rust をインストールしてください。

### **cargo-jk**

独自ビルドツール cargo-jk を使用します。以下でインストール可能です。

```bash
cargo install --git https://github.com/JK-Plugins/cargo-jk
```

## ビルド方法

以下のコマンドを実行してプロジェクトをビルドしてください。

```bash
cd pixelselector
cargo jk install
# this is the same as `cargo jk build && cargo jk mv [plugin_path]`
```

## リリース

GitHub の Actions → **Release** → **Run workflow** を押すと、全プラグインの最新版をビルドしてリリースします
（リリースの仕組みは [JK-Plugins](https://github.com/JK-Plugins/JK-Plugins) と同じ。設定は `.github/release.json`）。

- タグは今日の日付（日本時間）：`vYYYY.MM.DD`（同じ日の2回目以降は `.2`, `.3` …）
- Windows のインストーラーと zip、macOS の pkg、プラグインごとの `.aex` / `.plugin.zip` を置きます
- バージョンが新しいプラグインだけ、`<crate>-v<version>`（例：`createalpha-fs-v0.0.1`）の個別アーカイブも作ります
- プラグインのバージョンは各 `Cargo.toml` の `version` だけで管理します（build.rs が読んで AE のバージョンにします。
  AE の制限で minor と patch は 0〜15）
- プラグインが変わったのに `version` が同じ、または README.md / README-ja.md のプラグイン一覧のバージョンが合っていないと、
  リリースが止まります
- 配布サイト（https://jk-plugins.github.io/JK-Plugins/ ）は JK-Plugins 側で作っています
