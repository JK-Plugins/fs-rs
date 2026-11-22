# JK Color Change Fs

[F's Plugin](https://github.com/bryful/F-s-PluginsProjects) の ColorChange を Rust に移したもの。
指定した色（最大8組）を別の色に置き換えます。

A Rust port of F's ColorChange. Replaces up to 8 specified colors with other colors.

## パラメータ / Parameters

| 名前 | 内容 |
|---|---|
| level | 色の許容差（0〜100%）。0 なら完全に同じ色だけ。上げると近い色も置き換える |
| Enabled | 全体のオン・オフ（今の版では効果なし） |
| Target0〜7 | その組を使うか |
| Source Color0〜7 | 置き換える元の色 |
| Destination Color0〜7 | 置き換えた後の色 |

アルファはそのまま。8 / 16 / 32bpc に対応。
