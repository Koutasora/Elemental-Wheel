# Elemental Wheel

Action wheel opened by holding the right mouse button, laid out like the circle of fifths (Windows 10/11).

Status: stage 06, choosing a sector runs its action (program, file, URL, shortcut, edit command, text, command, system action); outer sectors can have a submenu arc of up to 5 tiles; tiles show names or icons. The choice is also printed. A separate editor window edits the tiles and options.

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
- `--sectors` - sectors in the outer ring (default 12); the inner ring has its own count, `inner_sectors` in the config file (default 12, no command-line switch)
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

## Editor

```
target\release\wheel-editor.exe [--config <path>]
target\release\wheel-editor.exe --version
```

- Opens `%APPDATA%\Elemental-Wheel\config.json` or the file given with `--config`. A missing file shows the defaults and is created on the first save. Load errors and warnings are shown in the bottom bar.
- Left: preview of the wheel (both rings, sector count, names or icons, theme, submenu arc of the selected tile). Click a tile to select it.
- Right, tab "Kafelek": name, action type with its fields (program path, arguments and working folder; file or folder; URL; shortcut; edit command; text; command line; system action), icon path, emptying the slot. Outer tiles get a submenu of up to 5 tiles: add, remove, move up and down. Every name can be changed, edit commands in the inner ring included.
- Tab "Opcje": theme, size 60-140%, inner sectors 4-12, outer sectors 4-12, view, hold time, movement threshold, full screen switch, excluded programs. Fewer sectors hide the extra items without deleting them.
- Dropping a file or folder on a tile of the preview makes it a `program` item (`.exe`, `.lnk`, `.bat`, `.cmd`) or an `open` item, named after the file; the tile keeps its submenu.
- Save with "Zapisz" or Ctrl+S. The configuration is validated first; on an error the file is left as it was and the reason is shown. Unsaved changes are marked in the title bar and the bottom bar; closing or reloading asks first. A running `wheel.exe` picks up the saved file by itself.

## Log

`wheel.exe` (`--run`, `--demo`) and `wheel-editor.exe` write a log to `%LOCALAPPDATA%\Elemental-Wheel\wheel.log` and `editor.log` (UTC, one line per event, with the process id); the same lines go to stderr. Up to 1 MB per file; when full, it is moved to `wheel.old.log` (`editor.old.log`) and a new log starts. Panics are logged too. `--log <path>` selects another file. If the file cannot be opened, the program runs without it.

## Configuration

`%APPDATA%\Elemental-Wheel\config.json`, created with the defaults on first start. UTF-8 (a UTF-16 file with BOM, as PowerShell 5.1 writes it, is read too). Every valid file that is loaded is copied to `config.json.bak` (the last good version); saving is atomic (temporary file, then replace) and copies the previous file to `.bak` only if it loads without error.

```json
{
  "version": 1,
  "options": {
    "theme": "system",
    "scale_pct": 100,
    "inner_sectors": 12,
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
- `inner_sectors` and `outer_sectors` (4 to 12, default 12 each, independent; a file without `inner_sectors` uses 12) set how many sectors each ring shows. Items with a higher index stay in the file but are hidden: not drawn, not selectable.
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
