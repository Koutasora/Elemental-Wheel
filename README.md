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

## Demo

```
target\release\wheel.exe --demo [--theme system|light|dark] [--scale 60..140] [--sectors 4..12]
```

Shows the wheel centred under the cursor without taking focus; closes on a click or after 10 s.

- `--theme` - `system` (default) follows the Windows app theme
- `--scale` - size in percent relative to the monitor DPI (default 100)
- `--sectors` - sectors in the outer ring (default 12); the inner ring always has 12

## Layout

- `crates/wheel-core` - wheel geometry and logic
- `apps/wheel` - background program
- `apps/wheel-editor` - editor

Concept mockup: [docs/koncept.html](docs/koncept.html) (open locally in a browser).

## License

MIT
