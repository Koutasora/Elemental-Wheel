# Elemental Wheel

Action wheel opened by holding the right mouse button, laid out like the circle of fifths (Windows 10/11).

Status: stage 05, choosing a sector runs its action (program, file, URL, shortcut, edit command, text, command, system action); outer sectors can have a submenu arc of up to 5 tiles; tiles show names or icons. The choice is also printed.

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
- Release in the centre right after opening keeps the wheel open: left click selects; left click in the centre or outside, right click or Esc closes. Release in the centre after the pointer has been on the tiles closes the wheel. Esc while holding cancels.
- Submenu: pointing at an outer sector with a submenu (marked at its outer edge) opens an arc of up to 5 tiles in the same direction. Releasing or clicking on a tile, or releasing further out in its direction, selects it; the sector itself keeps its own action. Moving to another sector closes the arc.
- The right button also passes through unchanged over windows of programs run as administrator, over programs on the exclusion list and over full screen windows (`disable_in_fullscreen`).
- The selection is printed with sectors numbered from 1 (`selected: outer 3 "Name"`, `selected: outer 10 submenu 2 "Name"`, `cancelled`). One instance at a time; `--quit` stops it.
- `--hold-ms`, `--move-px` - hold time and movement threshold (defaults 180 and 6).
- The action runs in the window under the cursor when the button went down. Windows blocks injected input to windows of programs run as administrator, where the wheel does not open anyway.

## Configuration

`%APPDATA%\Elemental-Wheel\config.json`, created with the defaults on first start. UTF-8 (a UTF-16 file with BOM, as PowerShell 5.1 writes it, is read too). Every valid file that is loaded is copied to `config.json.bak` (the last good version); saving is atomic (temporary file, then replace).

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
    { "name": "Ustawienia", "action": { "type": "url", "url": "ms-settings:" },
      "submenu": [
        { "name": "Ekran", "action": { "type": "url", "url": "ms-settings:display" } },
        { "name": "Dźwięk", "action": { "type": "url", "url": "ms-settings:sound" } }
      ] },
    { "name": "Blender", "action": { "type": "program", "path": "C:\\Program Files\\Blender\\blender.exe" }, "icon": "%SystemRoot%\\System32\\shell32.dll,3" },
    { "name": "Schowek", "action": { "type": "shortcut", "keys": "Win+V" } }
  ],
  "inner": [
    { "name": "Kopiuj", "action": { "type": "edit", "command": "copy" } }
  ]
}
```

- `outer` and `inner` hold up to 12 items each; the index is the sector (0 at the top, clockwise). Missing sectors are empty.
- `submenu`: up to 5 items of an outer sector, shown as an arc centred on its direction; submenu items cannot have their own submenu.
- `view`: `names` or `icons`. In the icon view a tile shows `icon` if set (`.ico`, `.exe` or `.dll`, optionally `,index`; `%VARIABLES%` expand), otherwise the icon of the program, file or folder, a built-in symbol for edit commands, system actions, URLs, shortcuts, text and commands, or the first letter of the name. The centre shows the name of the tile under the cursor.
- Action types: `none`, `program` (`path`, `args`, `working_dir`), `open` (file or folder), `url`, `shortcut` (`keys`, e.g. `Ctrl+Shift+V`, `Win+V`, `F2`), `edit`, `paste_text` (`text`; the clipboard is restored afterwards), `command` (`line`, run with `cmd /c` without a window), `system`.
- `edit` commands: `cut`, `copy`, `paste`, `paste_plain`, `delete`, `undo`, `redo`, `select_all`, `save`, `find`, `rename`, `properties`.
- `system` actions: `lock`, `close_window`, `minimize_window`, `show_desktop`, `volume_mute`, `volume_up`, `volume_down`, `screenshot`.
- `program` and `open` paths and arguments expand `%VARIABLES%`.
- `excluded` lists program file names (case does not matter).
- Numbers outside the allowed range are clamped with a warning on stderr. An invalid file (bad JSON or encoding, unknown type, invalid shortcut, too many items) is not used and never overwritten: the program falls back to `config.json.bak`, then to the defaults, and says why on stderr.
- The config folder is watched: a changed file is reloaded within a moment, no restart needed. Exclusions, actions, names and trigger thresholds apply from the next press, wheel size and sector count from the next opening. If the folder is missing at start, the file is checked when the wheel opens instead.

## Layout

- `crates/wheel-core` - wheel geometry, configuration and shortcut parsing
- `apps/wheel` - background program
- `apps/wheel-editor` - editor

Concept mockup: [docs/koncept.html](docs/koncept.html) (open locally in a browser).

## License

MIT
