# Elemental Wheel

Action wheel opened by holding the right mouse button, laid out like the circle of fifths (Windows 10/11).

Status: stage 04, choosing a sector runs its action (program, file, URL, shortcut, edit command, text, command, system action). The choice is also printed.

## Build

Requires Rust stable (MSVC toolchain) and Visual Studio 2022 Build Tools with the C++ workload.

```
cargo build --release
cargo test
target\release\wheel.exe --version
```

## Demo

```
target\release\wheel.exe --demo [--theme system|light|dark] [--scale 60..140] [--sectors 4..12] [--config <path>]
```

Shows the wheel centred under the cursor without taking focus; closes on a click or after 10 s.

- `--theme` - `system` (default) follows the Windows app theme
- `--scale` - size in percent relative to the monitor DPI (default 100)
- `--sectors` - sectors in the outer ring (default 12); the inner ring always has 12
- `--config` - config file to use instead of `%APPDATA%\Elemental-Wheel\config.json`

Options given on the command line override the file.

## Run in the background

```
target\release\wheel.exe --run [--theme ...] [--scale ...] [--sectors ...] [--hold-ms 50..2000] [--move-px 1..50] [--config <path>]
target\release\wheel.exe --quit
```

- Hold the right button for 180 ms without moving: the wheel opens under the cursor; release selects the sector in that direction.
- Short click and drag with the right button (moved more than 6 px before 180 ms) reach programs unchanged.
- Release in the centre keeps the wheel open: left click selects; left click in the centre or outside, right click or Esc closes. Esc while holding cancels.
- The right button also passes through unchanged over windows of programs run as administrator, over programs on the exclusion list and over full screen windows (`disable_in_fullscreen`).
- The selection is printed with sectors numbered from 1 (`selected: outer 3 "Name"`, `cancelled`). One instance at a time; `--quit` stops it.
- `--hold-ms`, `--move-px` - hold time and movement threshold (defaults 180 and 6).
- The action runs in the window under the cursor when the button went down. Windows blocks injected input to windows of programs run as administrator, where the wheel does not open anyway.

## Configuration

`%APPDATA%\Elemental-Wheel\config.json`, created with the defaults on first start. Saved atomically (temporary file, then replace) with the previous version kept as `config.json.bak`.

```json
{
  "version": 1,
  "options": {
    "theme": "system",
    "scale_pct": 100,
    "outer_sectors": 12,
    "view": "names",
    "hold_ms": 180,
    "move_px": 6,
    "disable_in_fullscreen": true,
    "excluded": ["blender.exe"]
  },
  "outer": [
    { "name": "Notatnik", "action": { "type": "program", "path": "notepad.exe" } },
    { "name": "Schowek", "action": { "type": "shortcut", "keys": "Win+V" } }
  ],
  "inner": [
    { "name": "Kopiuj", "action": { "type": "edit", "command": "copy" } }
  ]
}
```

- `outer` and `inner` hold up to 12 items each; the index is the sector (0 at the top, clockwise). Missing sectors are empty. `submenu` and `icon` are reserved for stage 05.
- Action types: `none`, `program` (`path`, `args`, `working_dir`), `open` (file or folder), `url`, `shortcut` (`keys`, e.g. `Ctrl+Shift+V`, `Win+V`, `F2`), `edit`, `paste_text` (`text`; the clipboard is restored afterwards), `command` (`line`, run with `cmd /c` without a window), `system`.
- `edit` commands: `cut`, `copy`, `paste`, `paste_plain`, `delete`, `undo`, `redo`, `select_all`, `save`, `find`, `rename`, `properties`.
- `system` actions: `lock`, `close_window`, `minimize_window`, `show_desktop`, `volume_mute`, `volume_up`, `volume_down`, `screenshot`.
- `program` and `open` paths and arguments expand `%VARIABLES%`.
- `excluded` lists program file names (case does not matter).
- Numbers outside the allowed range are clamped with a warning on stderr. An invalid file (bad JSON, unknown type, invalid shortcut, too many items) is not used and never overwritten: the program falls back to `config.json.bak`, then to the defaults, and says why on stderr.
- A changed file is reloaded the next time the wheel opens, no restart needed. Trigger thresholds apply from the next press, wheel size and sector count from the next opening.

## Layout

- `crates/wheel-core` - wheel geometry, configuration and shortcut parsing
- `apps/wheel` - background program
- `apps/wheel-editor` - editor

Concept mockup: [docs/koncept.html](docs/koncept.html) (open locally in a browser).

## License

MIT
