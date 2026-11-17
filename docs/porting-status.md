# 移植の進み具合

元: [F's Plugins](https://github.com/bryful/F-s-PluginsProjects) の `Legacy/Project`。

**109 本すべて移植した。** ソースが `Legacy/Project` にあるものは、出荷された .aex が
無いもの (`ChannelBlur` / `ShineParallel` / `Spark_Test` / `Extract_Edge`) も含めて
ぜんぶ入っている。

移植したものはすべて、出荷されている .aex とパラメータ構成 (型・範囲・既定値・色・
ポップアップ項目) が一致し、[AEXCompat](https://github.com/onmokoworks/AEXCompat) で
画素を突き合わせてある。意図して元と違えているのはバグ修正ぶんだけで、どの修正にも
`FAITHFUL` で元に戻す口を付けて「直したところ以外は画素まで一致する」ことを
確かめてある (戻せない事情があるものは下に書いた)。

確かめかたは `scratchpad` の3本:

- `port_compare.py <crate>` — classic 経路で元と移植を同じパラメータで描かせて
  画素を突き合わせる。`FAITHFUL=1` で「直したバグを戻した版」を回す
- `audit_layouts.py` — 出荷 .aex と移植のパラメータ構成を総当たりで見比べる
- 各クレートの `cargo test` — 手順ごとの単体テスト

## 元の都合で画素を比べられないところ

各コミットに書いてあるが、まとめると:

- **`Minimax` を通る経路** (`Max` など)。元は `Minimax` を呼ぶたびにワールドを
  2つ漏らす (`dispose` が `Minimax` では埋まらない別のメンバで条件分けされている)。
  AEXCompat は `world_lifetimes_balanced` を満たせないセッションを閉じないので、
  フィルタが効く値では元を描かせられない。値 0 の素通しだけ比較できる。
  なお `MaxFast` はこの `Minimax` ではなく自前の実装 (van Herk / Gil-Werman) で、
  ワールドを `new`/`dispose` で釣り合わせているので値を振っても比較できた
- **16/32bit で既定以外のパラメータを使う経路**。classic 経路は 8bit 専用で、
  深度を選べる smart 経路はパラメータを取らないため
- **`Paint` / `PaintMultPoint` の塗りつぶし**。どちらも元の classic の `Render` が
  パラメータを読むだけで `Exec()` を呼んでいない (`Paint` は入力のコピーまで、
  `PaintMultPoint` は出力に何も書かない)。塗りつぶしは smart 経路にしかなく、
  その smart 経路は `Paint` では AEXCompat で落ち、`PaintMultPoint` では
  チェックボックスが既定で全部 OFF なので塗りつぶしに入らない
- **`Star_Colorful` の下の縁**。元の `GetPX` が縦のはみ出しで `xx` に代入していて
  (`yy` のつもり)、いちばん下の行でワールドの外を読む。読んだ値は決まって
  いないので、下の縁から「光の長さ + 1」行を外して突き合わせてある
  (残りは 13 通りぜんぶ画素まで一致)
- **`TouchDraw` / `TouchDrawStraght` の `blockDraw`** (`Block_Value` で四角を置いて
  旗を消すところ)。四角は乱数4つで決まるだけなのに、出荷版が消す四角が
  ソースのとおりの式にならない (大きさ・枚数・乱数を捨てる数を総当たりしても
  合わない)。乱数そのものとほかの段は合っていて、`Block_Value` を 0 にした回は
  種を4通り × ばらつきの入り切りの8通りでぜんぶ画素まで一致する。出荷版の
  `blockDraw` は手元のソースと違うものだとみて、ソースのとおりに移植してある
- **`Thin` の `EdgeFilter`**。元はこの走査だけマルチスレッドで、担当の継ぎ目の行が
  0 埋めの写しを読む。しかも分ける数が `hardware_concurrency()` なので出る絵が
  機械のコア数で変わる。1本の走査に直したので、`EdgeFilter` を入れた回だけ
  継ぎ目の行に数画素の差が出る (それがこの直しの裏づけになっている)

## 直したバグ

元の見た目をそのまま引き継ぐのではなく、**明らかなバグは直す** 方針で移植した
(それがモダン化の目的)。直したものは各クレートの先頭の注と、`FAITHFUL` の
切り替えに書いてある。多かったのは:

- **32bit (と 16bit) の経路が空**。`case PF_PixelFormat_ARGB128: break;` で
  何もしないものが多い (`IroTore` / `Flare` / `grayToWaveLine` / `Filter` /
  `IroToreAdjacent` / `PaperPlaneGetWeightInfo` など)。8bit と同じ道を通すようにした
- **縁の押さえ違い**。`if (yy >= height) xx = height - 1;` のように別の変数へ
  代入しているもの (`Extract_Edge` / `Star_Colorful`)
- **ぼかしの四隅と辺**。`TouchDrawCenter` 系の `AlphaBlur` は四隅3つの読み書きが
  1行 (1列) ずれていて、角にぼかしが掛からない。`TouchDraw` / `NamiGarasu` も
  同じ中身を持っていた
- **使われない結果**。`TouchDraw` の `copyAlpha` がぼかす前の緑を使っていて、
  直前の `greenBlur` の結果が一度も絵に出てこなかった
- **classic 経路で描かない**。`Paint` / `PaintMultPoint` の `Render` が `Exec` を
  呼んでいない
- **非 ASCII のパラメータ名**。AE 上では空の名前になるので英語にした
  (`IroTore` の「あきらめ色」→ `GiveUpColor` など)

## 双子について

ソースがほぼ同じ組がいくつかある (`Unmult_RG` と `Unmult_KNSW`、`Tone` と `Toner` は
実際に #include 行以外同じだった)。片方を移植すればもう片方はほぼ無料なので、
組になっているものは続けて片付けた。`TouchDraw` / `TouchDrawStraght` は
`scratchpad/gen_touchdrawstraght.py` で片方からもう片方を作っている。

## 部品の状況

`libs` に入っている再利用できる部品:

- 丸め・深度変換 (`utils`): 元の `Round*` が切り捨てであること、
  `FS_CONVERT8TO32` が小数4桁に量子化すること、double 版が要る場所まで含めて合わせてある
- 最大最小フィルタ (`fs_graphics8::minimax8` / `minimax_alpha8`、f32 版も)
- 最大最小の速い版 (`minmax_vhgw`: van Herk / Gil-Werman。MaxFast が使う)
- ぼかし (`fs_graphics8::gblur8`)
- 乱数 (`fs_graphics::XorShift`: 整数版・double 版・`F_RAND_D1` 相当。種まきも元と同じ)
- HLS (`hls`) と Lab (`lab`)
- ブレンドモード各種 (`fs_graphics8`)
- モザイクの塗りつぶし (`fs_mosaic::flat`、元の flat8/16/32)。`Mosaic` と
  `RandomMosaic` が共有している
- 飛び散りのスプライト表と描き方 (`sputtering`: 14x14 を 60 種類、
  大きさごとの引き表、深度ごとの描画 `sput_draw`)。sputteringAlpha /
  sputteringCircle も同じものを使う
- 16bit の画素と合成 (`fs_graphics8::Px16` / `pixel_blend16`)
- 角度と距離から XY のずれを出す `utils::rot_length` (元の CRotCalc)
- 火花の折れ線 (`spark`: 元の Spark/CPointInfo.h と Spark/CLineDraw.h。
  点の並び・乱数での折り方・白地の下書きへのアンチエイリアス付きの描画・
  色の塗り替え・入力との重ね合わせ)。Spark 系5本が共有する
- 画面全体をずらして拾い直すところと縁の処理 (`screen_shake`: 元の
  FsScreenShake.h。透明/伸ばす/繰り返す/鏡 と深度ごとのバイリニア、
  mm2px、揺れの量の決め方)。ScreenShakeDir と ScreenShakeMM が共有している
- 小さい絵への最大最小・ぼかし・双曲線 (`tiny`: MaxBlur と InnerFlare 系が
  共有する。元の `*10` の取りこぼしを `Quirk` で切り替えられる)
- バケツ塗り (`paint`: Paint と PaintMultPoint が共有)
- 光条 (`shine`: Shine と ShineParallel が共有)
- タッチ描きの共通部分 (`touchdraw`: 対象探し・四角の間引き・点の置きかた・
  赤 (緑) をぼかしてアルファに入れるところ・色付け。TouchDraw /
  TouchDrawStraght / NamiGarasu が共有)

元のデータ表をそのまま持ってきたもの:

- `namigarasu/src/sq_data.bin` — 丸のビットマップ11枚 (10x10〜200x200)
- `paperplaneweight/src/font.bin` — 12x18 のビットマップフォント (ASCII 95字)

## 残っている宿題

絵には影響しないが、まだ手を付けていないもの:

- `AlphaHyperbolic` だけ、元の .aep で保存した値が復元されない
  (matchName は合っているのに。原因は未解明)
- `PixelSelector` / `PixelReplace` の `Disp` / `Use Count` の supervise
  (出し入れの連動) が無い
- `TargetGrad` 系の colorTable の読み書きボタン (`.pref`)
