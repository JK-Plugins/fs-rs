# fs-rs

## Prerequisites

### **LLVM**  

Download and install from  
https://github.com/llvm/llvm-project/releases

### **Adobe After Effects SDK (May 2023)**  

Download from https://console.adobe.io/downloads/ae
Place the SDK folder anywhere you like, then set:
```bash
# Linux / macOS
export AESDK_ROOT=/path/to/AfterEffects_SDK

# Windows (PowerShell)
setx AESDK_ROOT "C:\path\to\AfterEffects_SDK"
```

### **Rust**

Install Rust using [rustup](https://rustup.rs/).  

### **Just**

We are using Just as build tool.

Install it with:
```bash
cargo install just
```

## Build

Run the following command to build the project:
```bash
cd pixelselector
just build
```
