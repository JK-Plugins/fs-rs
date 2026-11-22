# fs-rs

[English](./README.md) | [日本語](./README-ja.md)

# 概要

このプラグインは [F's Plugin](https://github.com/bryful/F-s-PluginsProjects) を Rust に移植したものです。
F's Plugin の機能を Rust で再実装することで、macOSの対応と高速化を目指しています。

**ダウンロード：https://jk-plugins.github.io/JK-Plugins/**

## プラグイン一覧

| プラグイン | バージョン | 説明 |
|---|---|---|
| [JK Color Change Fs](colorchange/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchange-fs-v0.0.1) | 指定した色（最大8組）を別の色に置き換える |
| [JK Color Change Simple Fs](colorchangesimple/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangesimple-fs-v0.0.1) | 指定した1色を別の色に置き換える |
| [JK Color Key Fs](colorkey/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorkey-fs-v0.0.1) | 指定した色を透明にする |
| [JK Create Alpha Fs](createalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/createalpha-fs-v0.0.1) | 明るさからアルファを作る（黒が透明） |
| [JK Pixel Selector Fs](pixelselector/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelselector-fs-v0.0.1) | 指定した色の画素だけを残す |

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
