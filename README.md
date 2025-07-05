# fs-rs

[English](./README.md) | [日本語](./README-ja.md)

## About

Those plugins are Rust port of the [F's Plugin](https://github.com/bryful/F-s-PluginsProjects).

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
