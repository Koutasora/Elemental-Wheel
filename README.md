# Elemental Wheel

Action wheel opened by holding the right mouse button, laid out like the circle of fifths (Windows 10/11).

Status: stage 03, holding the right mouse button opens the wheel; the selection is only printed, actions come later.

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

## Run in the background

```
target\release\wheel.exe --run [--theme ...] [--scale ...] [--sectors ...] [--hold-ms 50..2000] [--move-px 1..50]
target\release\wheel.exe --quit
```

- Hold the right button for 180 ms without moving: the wheel opens under the cursor; release selects the sector in that direction.
- Short click and drag with the right button (moved more than 6 px before 180 ms) reach programs unchanged.
- Release in the centre keeps the wheel open: left click selects; left click in the centre or outside, right click or Esc closes. Esc while holding cancels.
- Windows of programs run as administrator get the right button unchanged.
- The selection is printed (`selected: outer 3 "Name"`, `cancelled`). One instance at a time; `--quit` stops it.
- `--hold-ms`, `--move-px` - hold time and movement threshold (defaults 180 and 6).

## Layout

- `crates/wheel-core` - wheel geometry and logic
- `apps/wheel` - background program
- `apps/wheel-editor` - editor

Concept mockup: [docs/koncept.html](docs/koncept.html) (open locally in a browser).

## License

MIT
