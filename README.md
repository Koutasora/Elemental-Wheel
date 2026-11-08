# Elemental Wheel

Action wheel opened by holding the right mouse button, laid out like the circle of fifths (Windows 10/11).

Status: stage 08, choosing a sector runs its action (program, file, URL, shortcut, edit command, text, command, system action); outer sectors can have a submenu arc of up to 5 tiles; tiles show names or icons. The wheel is drawn as glass over a blurred copy of the screen under it, with short animations. A separate editor window edits the tiles and options.

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
- With `replace_right_click` the wheel opens on the press itself and the right button never reaches programs (their context menu is available through a `context_menu` tile); the exceptions below still apply. `hold_ms` and `move_px` are ignored.
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
- Left: preview of the wheel (both rings, sector count, names or icons, theme, glass look over a sample backdrop, submenu arc of the selected tile). Click a tile to select it.
- Right, tab "Kafelek": name, action type with its fields (program path, arguments and working folder; file or folder; URL; shortcut; edit command; text; command line; system action), icon path, emptying the slot. Outer tiles get a submenu of up to 5 tiles: add, remove, move up and down. Every name can be changed, edit commands in the inner ring included.
- Tab "Opcje": theme, size 60-140%, glass transparency 0-100%, animations, inner sectors 4-12, outer sectors 4-12, view, "wheel instead of the right button", hold time, movement threshold, full screen switch, excluded programs. Fewer sectors hide the extra items without deleting them.
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
    "excluded": ["blender.exe"],
    "glass_pct": 50,
    "animations": true,
    "replace_right_click": false
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
- Property windows opened with `properties` belong to the wheel and close when it stops.
- `edit` commands: `cut`, `copy`, `paste`, `paste_plain`, `delete`, `undo`, `redo`, `select_all`, `save`, `find`, `rename`, `properties`.
- `system` actions: `lock`, `close_window`, `minimize_window`, `show_desktop`, `volume_mute`, `volume_up`, `volume_down`, `screenshot`, `context_menu` (a right click at the point where the wheel was opened; the pointer moves there).
- `program` and `open` paths and arguments expand `%VARIABLES%`.
- `excluded` lists program file names (case does not matter).
- `glass_pct` (0 to 100, default 50): how transparent the glass of the wheel is; 0 gives the opaque look. Without Windows transparency (Settings > Personalization > Colors) the wheel is opaque.
- `animations` (default `true`): animations of the wheel on or off. With animation effects off in Windows (Settings > Accessibility > Visual effects) the wheel does not animate either.
- `replace_right_click` (default `false`): the wheel opens as soon as the right button goes down, without the hold time and movement threshold; programs get no right clicks (except as in "Run in the background").
- Numbers outside the allowed range are clamped with a warning (in the log and on stderr). An invalid file (bad JSON or encoding, unknown type, invalid shortcut, too many items) is never overwritten. At start the program falls back to `config.json.bak`, then to the defaults. While running, an invalid file is ignored and the current configuration stays; either way the reason goes to the log (and stderr).
- The config folder is watched: a changed file is reloaded within a moment, no restart needed. Exclusions, actions, names and trigger thresholds apply from the next press, wheel size and sector count from the next opening. If the folder is missing at start, the file is checked when the wheel opens instead.

## Measurements

Release build, 1920x1080 at 96 DPI over RDP, 12 outer and 12 inner sectors, glass 50; `--run --stats` with a physical mouse.

| | |
|---|---|
| opening, median / max | 6.3 / 19.6 ms |
| screen copy for the glass (on the press, before the wheel opens) | 20-50 ms |
| private memory in the background, names / icons view, glass | 22.9 / 25.0 MB |
| private memory in the background, names / icons view, no glass | 13.4 / 16.7 MB |

## Known issues

- Holding the right button right after the wheel was closed with Escape sometimes does not open it; the next hold does.

## Layout

- `crates/wheel-core` - wheel geometry, configuration and shortcut parsing
- `apps/wheel` - background program
- `apps/wheel-editor` - editor
- `tools/soak.ps1` - soak test and measurements

Concept mockup: [docs/koncept.html](docs/koncept.html) (open locally in a browser).

## License

MIT
