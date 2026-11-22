# JK Color Change Simple Fs

[F's Plugin](https://github.com/bryful/F-s-PluginsProjects) の ColorChangeSimple を Rust に移したもの。
指定した1色を別の色に置き換えます。

A Rust port of F's ColorChangeSimple. Replaces one specified color with another.

## パラメータ / Parameters

| 名前 | 内容 |
|---|---|
| level | 色の許容差（0〜100%）。0 なら完全に同じ色だけ |
| Source Color | 置き換える元の色 |
| Destination Color | 置き換えた後の色 |

アルファはそのまま。8 / 16 / 32bpc に対応。
