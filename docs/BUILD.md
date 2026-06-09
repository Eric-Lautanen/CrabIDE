# Building crabide

## Prerequisites

### Rust Toolchain

- Rust 1.80+ (stable)
- Install via [rustup](https://rustup.rs/): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Linux

```bash
sudo apt-get install -y \
    libclang-dev \
    libgtk-3-dev \
    libxcb-render0-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libxkbcommon-dev \
    libssl-dev \
    libgit2-dev \
    pkg-config
```

### macOS

Xcode Command Line Tools: `xcode-select --install`

### Windows

- Build Tools for Visual Studio 2022 (or Visual Studio 2022 with "Desktop development with C++")
- CMake (for tree-sitter grammar compilation)

## Build Commands

```bash
# Default build (includes git support)
cargo build --release

# Minimal build (no git support)
cargo build --release --no-default-features

# With WASM extension support
cargo build --release --features wasm-extensions

# All features
cargo build --release --all-features

# Check (faster than build, catches type errors)
cargo check --workspace
```

## Feature Flags

| Flag | Description |
|------|-------------|
| `git-support` (default) | Git integration via libgit2 |
| `wasm-extensions` | WASM extension runtime (wasmtime + cranelift) |
| `webview` | WebView extension panels (wry) |
| `remote-ssh` | SSH remote development (russh) |
| `dev-containers` | Docker Dev Containers (bollard) |

## Running

```bash
# From source
cargo run --release

# With file arguments
cargo run --release -- path/to/file.rs

# With custom log level
cargo run --release -- --log debug
```

## Testing

```bash
# All tests
cargo test --workspace

# Single crate
cargo test --package crabide-buffer

# With features
cargo test --features wasm-extensions
```

## Packaging

Packaging scripts are in `tools/`:

| Script | Platform | Output |
|--------|----------|--------|
| `tools/package-windows.ps1` | Windows | `.zip` portable + optional NSIS installer |
| `tools/package-macos.sh` | macOS | `.app` bundle + optional `.dmg` |
| `tools/package-linux.sh` | Linux | AppImage + `.deb` + `.rpm` |

```bash
# First build in release mode
cargo build --release

# Then package
# Linux
bash tools/package-linux.sh

# macOS
bash tools/package-macos.sh

# Windows (PowerShell)
powershell -File tools/package-windows.ps1
```

## Troubleshooting

### Linker errors on Linux

Install `mold` for faster linking:
```bash
sudo apt-get install mold
```

### tree-sitter compilation failures

Ensure CMake is installed:
```bash
sudo apt-get install cmake
```

### libgit2 errors

On older systems, libgit2 1.7 may not be available. Install from source or use the vendored feature:
```toml
# In crates/crabide-git/Cargo.toml
git2 = { version = "0.21", features = ["vendored-openssl", "vendored-libgit2"] }
```
