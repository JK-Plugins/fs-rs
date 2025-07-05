# fs-rs

[English](./README.md) | [日本語](./README-ja.md)

# 概要

このプラグインは [F's Plugin](https://github.com/bryful/F-s-PluginsProjects) を Rust に移植したものです。
F's Plugin の機能を Rust で再実装することで、macOSの対応と高速化を目指しています。

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
