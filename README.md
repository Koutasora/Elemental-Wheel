# Elemental Wheel

Action wheel opened by holding the right mouse button, laid out like the circle of fifths (Windows 10/11).

Status: stage 02, the wheel is drawn by a demo command; the right-button trigger and actions come later.

## Build

Requires Rust stable (MSVC toolchain) and Visual Studio 2022 Build Tools with the C++ workload.

```
cargo build --release
cargo test
target\release\wheel.exe --version
```

## Layout

- `crates/wheel-core` - wheel geometry and logic
- `apps/wheel` - background program
- `apps/wheel-editor` - editor

Concept mockup: [docs/koncept.html](docs/koncept.html) (open locally in a browser).

## License

MIT
