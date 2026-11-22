# JK Create Alpha Fs

[F's Plugin](https://github.com/bryful/F-s-PluginsProjects) の CreateAlpha を Rust に移したもの。
画像の明るさからアルファを作ります（黒が透明、明るいほど不透明）。

A Rust port of F's CreateAlpha. Builds the alpha channel from brightness (black becomes transparent).

## 動き / How it works

- 各画素の R・G・B の最大値をアルファにする
- もとのアルファが 1 未満の画素は、先に RGB にアルファを掛けてから求める
- パラメータはなし

8 / 16 / 32bpc に対応。
