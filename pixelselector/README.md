# JK Pixel Selector Fs

[F's Plugin](https://github.com/bryful/F-s-PluginsProjects) の PixelSelector を Rust に移したもの。
指定した色の画素だけを残し、それ以外を透明にします。

A Rust port of F's PixelSelector. Keeps only pixels of the specified color and makes the rest transparent.

## パラメータ / Parameters

| 名前 | 内容 |
|---|---|
| Options → Invert | 反転（指定した色の画素を透明にし、それ以外を残す） |
| target1 → Enabled | 色の指定を使うか |
| target1 → Target Color | 残す色 |
| Threshold | 色の許容差 |

8 / 16 / 32bpc に対応。
