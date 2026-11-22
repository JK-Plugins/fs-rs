# fs-rs

[English](./README.md) | [日本語](./README-ja.md)

## About

Those plugins are Rust port of the [F's Plugin](https://github.com/bryful/F-s-PluginsProjects).
By re-implementing the functionality of F's Plugin in Rust, we aim to add macOS support and achieve higher performance.

**Download: https://jk-plugins.github.io/JK-Plugins/**

## Plug-ins

| Plug-in | Version | Description |
|---|---|---|
| [JK Color Change Fs](colorchange/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchange-fs-v0.0.1) | Replaces up to 8 specified colors with other colors |
| [JK Color Change Simple Fs](colorchangesimple/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorchangesimple-fs-v0.0.1) | Replaces one specified color with another |
| [JK Color Key Fs](colorkey/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/colorkey-fs-v0.0.1) | Makes the specified color transparent |
| [JK Create Alpha Fs](createalpha/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/createalpha-fs-v0.0.1) | Builds the alpha channel from brightness (black becomes transparent) |
| [JK Pixel Selector Fs](pixelselector/README.md) | [0.0.1](https://github.com/JK-Plugins/fs-rs/releases/tag/pixelselector-fs-v0.0.1) | Keeps only pixels of the specified color |

Click a plug-in name for its parameters. Click a version for that version's files (Windows: `.aex`, macOS: `.plugin.zip`).

## Install

Download from the [latest release](https://github.com/JK-Plugins/fs-rs/releases/latest). Quit After Effects first.

- **Windows**: run `JK-Plugins-Fs-<date>-windows-setup.exe`. If "Windows protected your PC" appears, click "More info" → "Run anyway" (not signed).
  Plug-ins go into `C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\JK-Plugins`.
  Uninstall from Settings → Apps → Installed apps → **JK-Plugins Fs (After Effects)**.
  Without the installer: unzip `JK-Plugins-Fs-<date>-windows.zip` and copy the `JK-Plugins` folder into `MediaCore`.
- **macOS**: open `JK-Plugins-Fs-<date>-macos.pkg`. If it is blocked, click "Open Anyway" in System Settings → Privacy & Security (not signed).
  Plug-ins go into `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/JK-Plugins`.

To update, just run the newer installer.

## Prerequisites

### **LLVM**

Download and install from  
https://github.com/llvm/llvm-project/releases

and you need to set `LIBCLANG_PATH` to the path of the `bin` directory in the LLVM installation.

For example, if you installed LLVM to `/path/to/llvm`, you would set:

```bash
# macOS
export LIBCLANG_PATH=/path/to/llvm/bin
# Windows (PowerShell)
setx LIBCLANG_PATH "C:\path\to\llvm\bin"
```

and, you need to set `PATH` to include the LLVM binaries:

```bash
# macOS
export PATH=/path/to/llvm/bin:$PATH
" # Windows (PowerShell)
setx PATH "C:\path\to\llvm\bin;$env:PATH"
# or, if you use LLVM Installer, you can set it optionally
```

### **Adobe After Effects SDK (May 2023)**

Download from https://console.adobe.io/downloads/ae
Place the SDK folder anywhere you like, then set:

```bash
# macOS
export AESDK_ROOT=/path/to/AfterEffects_SDK

# Windows (PowerShell)
setx AESDK_ROOT "C:\path\to\AfterEffects_SDK"
```

### **Rust**

Install Rust using [rustup](https://rustup.rs/).

### **cargo-jk**

We are implementing our original build tool.

Install it with:

```bash
cargo install --git https://github.com/JK-Plugins/cargo-jk
```

## Build

Run the following command to build the project:

```bash
cd pixelselector
cargo jk install
# this is the same as `cargo jk build && cargo jk mv [plugin_path]`
```

## Release

Actions → **Release** → **Run workflow** builds all plug-ins and publishes a release
(same mechanism as [JK-Plugins](https://github.com/JK-Plugins/JK-Plugins); settings in `.github/release.json`).

- Tag: today's date in JST, `vYYYY.MM.DD` (`.2`, `.3`, … for later releases on the same day)
- Assets: Windows installer and zip, macOS pkg, and per-plug-in `.aex` / `.plugin.zip`
- A per-plug-in archive release `<crate>-v<version>` is created only for new versions
- Versions live only in each `Cargo.toml` (`build.rs` reads them; minor/patch must be 0–15 for After Effects)
- The release stops if a plug-in changed without a version bump, or if the versions in the README plug-in lists are out of date
