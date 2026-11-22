# JK Color Key Fs

[F's Plugin](https://github.com/bryful/F-s-PluginsProjects) の ColorKey を Rust に移したもの。
指定した色を透明にします。

A Rust port of F's ColorKey. Makes the specified color transparent.

## パラメータ / Parameters

| 名前 | 内容 |
|---|---|
| threshold | 色の許容差（0〜100%）。0 なら完全に同じ色だけ |
| Key Color | 透明にする色 |
| Background Color | 透明にした画素の色（RGB）。アルファは 0 |

8 / 16 / 32bpc に対応。
