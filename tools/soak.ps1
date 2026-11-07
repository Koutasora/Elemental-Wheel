# Soak test and measurement of wheel open time and memory (Windows PowerShell 5.1, no modules).
# Runs wheel.exe with injected input in a scratch folder: opens the wheel with a submenu arc and
# cancels it in a loop, rewrites the config now and then (also with broken JSON), and collects the
# --stats lines of the log into CSV files and a summary. Exit code 0 = PASS, 1 = FAIL.
#
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\soak.ps1 -WorkDir <dir> [-Minutes 10]
#     [-View names|icons] [-NoConfigChanges] [-ConfigEveryMinutes 2] [-Progress <file>]
param(
    [Parameter(Mandatory = $true)][string]$WorkDir,
    [int]$Minutes = 10,
    [string]$Exe,
    [int]$ConfigEveryMinutes = 2,
    [switch]$NoConfigChanges,
    [ValidateSet('names', 'icons')][string]$View = 'icons',
    [string]$Progress
)
$ErrorActionPreference = 'Stop'

# --- paths and refusals -----------------------------------------------------------------------
$WorkDir = [IO.Path]::GetFullPath($WorkDir).TrimEnd('\')
foreach ($root in @($env:APPDATA, $env:LOCALAPPDATA)) {
    if (-not $root) { continue }
    $user = (Join-Path $root 'Elemental-Wheel').TrimEnd('\')
    if ($WorkDir -ieq $user -or $WorkDir.StartsWith($user + '\', [StringComparison]::OrdinalIgnoreCase)) {
        Write-Host "Refusing: WorkDir $WorkDir is inside $user (real user data)."
        exit 1
    }
}
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
if (-not $Exe) { $Exe = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\target\release\wheel.exe' }
if (-not (Test-Path -LiteralPath $Exe)) { Write-Host "Missing exe: $Exe"; exit 1 }
$Exe = (Resolve-Path -LiteralPath $Exe).Path
$cfgPath = Join-Path $WorkDir 'config.json'
$logPath = Join-Path $WorkDir 'wheel.log'
$oldLogPath = Join-Path $WorkDir 'wheel.old.log'
$outPath = Join-Path $WorkDir 'stdout.txt'
$errPath = Join-Path $WorkDir 'stderr.txt'
$memCsv = Join-Path $WorkDir 'mem.csv'
$opensCsv = Join-Path $WorkDir 'opens.csv'
$errorsTxt = Join-Path $WorkDir 'errors.txt'
$summaryTxt = Join-Path $WorkDir 'summary.txt'
$arcPng = Join-Path $WorkDir 'arc.png'
foreach ($f in @($logPath, $oldLogPath, $outPath, $errPath, $memCsv, $opensCsv, $errorsTxt, $summaryTxt, $arcPng, "$cfgPath.bak", "$cfgPath.tmp")) {
    Remove-Item -LiteralPath $f -ErrorAction SilentlyContinue
}

# --- input injection --------------------------------------------------------------------------
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class W {
    [DllImport("user32.dll")] public static extern IntPtr SetProcessDpiAwarenessContext(IntPtr v);
    [DllImport("user32.dll")] public static extern uint GetDpiForSystem();
    [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
    [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    [StructLayout(LayoutKind.Sequential)] struct POINT { public int x, y; }
    [DllImport("user32.dll")] static extern IntPtr WindowFromPoint(POINT p);
    [DllImport("user32.dll")] static extern IntPtr GetAncestor(IntPtr h, uint flags);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    [DllImport("user32.dll")] static extern bool GetCursorPos(out POINT p);
    public static int[] CursorPos() { POINT p; GetCursorPos(out p); return new int[] { p.x, p.y }; }
    // Process id of the top-level window under a screen point (0 when none); for diagnosing failed cycles.
    public static uint PidAt(int x, int y) {
        IntPtr h = WindowFromPoint(new POINT { x = x, y = y });
        if (h == IntPtr.Zero) return 0;
        IntPtr root = GetAncestor(h, 2);
        uint pid; GetWindowThreadProcessId(root == IntPtr.Zero ? h : root, out pid);
        return pid;
    }
    [StructLayout(LayoutKind.Sequential)] struct MOUSEINPUT { public int dx, dy; public uint data, flags, time; public IntPtr extra; }
    [StructLayout(LayoutKind.Sequential)] struct INPUT { public uint type; public MOUSEINPUT mi; }
    [DllImport("user32.dll")] static extern uint SendInput(uint n, INPUT[] i, int size);
    public static void Mouse(uint flags, int dx, int dy) {
        var i = new INPUT[1]; i[0].type = 0; i[0].mi.flags = flags; i[0].mi.dx = dx; i[0].mi.dy = dy;
        SendInput(1, i, Marshal.SizeOf(typeof(INPUT)));
    }
}
'@
Add-Type -AssemblyName System.Drawing
[void][W]::SetProcessDpiAwarenessContext([IntPtr](-4))
$screenW = [W]::GetSystemMetrics(0)
$screenH = [W]::GetSystemMetrics(1)
$dpi = [W]::GetDpiForSystem()
$centerX = [int][math]::Floor($screenW / 2)
$centerY = [int][math]::Floor($screenH / 2)
$script:curX = $centerX
$script:curY = $centerY
$script:rightDown = $false

# Absolute moves (MOUSEEVENTF_MOVE | ABSOLUTE), so the low-level hook sees real move events;
# a few steps per move, like a hand.
function Move-To([int]$x, [int]$y) {
    $steps = 6
    $sx = $script:curX; $sy = $script:curY
    for ($i = 1; $i -le $steps; $i++) {
        $px = [int][math]::Round($sx + ($x - $sx) * $i / $steps)
        $py = [int][math]::Round($sy + ($y - $sy) * $i / $steps)
        $nx = [int][math]::Round($px * 65535.0 / ($screenW - 1))
        $ny = [int][math]::Round($py * 65535.0 / ($screenH - 1))
        [W]::Mouse(0x8001, $nx, $ny)
        Start-Sleep -Milliseconds 8
    }
    $script:curX = $x; $script:curY = $y
}
function Right-Down { [W]::Mouse(0x0008, 0, 0); $script:rightDown = $true }
function Right-Up { [W]::Mouse(0x0010, 0, 0); $script:rightDown = $false }
function Press-Escape {
    [W]::keybd_event(0x1B, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 30
    [W]::keybd_event(0x1B, 0, 2, [UIntPtr]::Zero)
}

# --- config variants --------------------------------------------------------------------------
$holdMs = 180
$variants = @(
    @{ theme = 'dark'; outer = 8; scale = 100 },
    @{ theme = 'light'; outer = 8; scale = 100 },
    @{ theme = 'dark'; outer = 6; scale = 120 },
    @{ theme = 'light'; outer = 12; scale = 80 },
    @{ theme = 'dark'; outer = 10; scale = 100 },
    @{ theme = 'light'; outer = 8; scale = 140 },
    @{ theme = 'dark'; outer = 12; scale = 100 },
    @{ theme = 'light'; outer = 6; scale = 80 }
)
# Outer sector 2 carries the submenu; it exists in every variant (4..12 sectors).
$subParent = 2
$subCount = 3

function New-ItemJson([string]$name, [int]$icon, $submenu) {
    @{ name = $name; action = @{ type = 'none' }; icon = "C:\Windows\System32\shell32.dll,$icon"; submenu = @($submenu) }
}
function New-ConfigText($v) {
    $outer = @()
    for ($i = 0; $i -lt 12; $i++) {
        $sub = @()
        if ($i -eq $subParent) {
            for ($k = 0; $k -lt $subCount; $k++) { $sub += , (New-ItemJson "Sub$k" (20 + $k) @()) }
        }
        $outer += , (New-ItemJson "Outer$i" (3 + $i) $sub)
    }
    $inner = @()
    for ($i = 0; $i -lt 8; $i++) { $inner += , (New-ItemJson "Inner$i" (40 + $i) @()) }
    $cfg = @{
        version = 1
        options = @{
            theme = $v.theme; scale_pct = $v.scale; inner_sectors = 8; outer_sectors = $v.outer
            view = $View; hold_ms = $holdMs; move_px = 6; disable_in_fullscreen = $false; excluded = @()
        }
        outer = $outer
        inner = $inner
    }
    ConvertTo-Json -InputObject $cfg -Depth 8
}
$utf8 = New-Object System.Text.UTF8Encoding($false)
function Write-Config($v) {
    $tmp = "$cfgPath.tmp"
    [IO.File]::WriteAllText($tmp, (New-ConfigText $v), $utf8)
    Move-Item -LiteralPath $tmp -Destination $cfgPath -Force
}
function Write-BrokenConfig {
    [IO.File]::WriteAllText($cfgPath, '{ "version": 1, "options": { "theme": "dark", ', $utf8)
}

# --- collecting the log -----------------------------------------------------------------------
$script:logOff = 0L
$script:outOff = 0L
$script:cancelCount = 0
$script:selectedCount = 0
$script:openCount = 0
$script:logErrors = 0
$script:expectedErrors = 0
$script:cycleErrors = 0
$script:expectedUntil = [datetime]::MinValue
$script:memT0 = $null
$script:lastMem = $null
$script:mem = New-Object System.Collections.ArrayList
$script:opens = @{ first = (New-Object System.Collections.ArrayList); next = (New-Object System.Collections.ArrayList); reload = (New-Object System.Collections.ArrayList); dpi = (New-Object System.Collections.ArrayList) }
[IO.File]::WriteAllText($memCsv, "time,minute,private_kb,working_set_kb,peak_working_set_kb,handles,gdi,user`r`n", $utf8)
[IO.File]::WriteAllText($opensCsv, "time,n,kind,total_ms,queue_ms,work_ms`r`n", $utf8)
$inv = [Globalization.CultureInfo]::InvariantCulture

# Complete new lines of a file from a byte offset; the offset moves only past the last newline.
function Read-Tail([string]$path, [long]$from) {
    $res = @{ lines = @(); next = $from }
    if (-not (Test-Path -LiteralPath $path)) { return $res }
    $fs = $null
    try {
        $fs = New-Object IO.FileStream($path, [IO.FileMode]::Open, [IO.FileAccess]::Read, ([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
        $len = $fs.Length
        if ($len -le $from) { return $res }
        [void]$fs.Seek($from, [IO.SeekOrigin]::Begin)
        $buf = New-Object byte[] ($len - $from)
        $n = $fs.Read($buf, 0, $buf.Length)
        $last = [Array]::LastIndexOf($buf, [byte]10, $n - 1)
        if ($last -lt 0) { return $res }
        $text = [Text.Encoding]::UTF8.GetString($buf, 0, $last + 1)
        $res.lines = $text.Split("`n") | ForEach-Object { $_.TrimEnd("`r") } | Where-Object { $_ -ne '' }
        $res.next = $from + $last + 1
    }
    finally { if ($fs) { $fs.Dispose() } }
    $res
}

function Add-Error([string]$text) {
    [IO.File]::AppendAllText($errorsTxt, ("{0} {1}`r`n" -f (Get-Date -Format 'HH:mm:ss'), $text), $utf8)
}

function Handle-LogLine([string]$line) {
    if ($line -match '^(\S+ \S+) \[\d+\] stats mem private_kb=(\d+) working_set_kb=(\d+) peak_working_set_kb=(\d+) handles=(\d+) gdi=(\d+) user=(\d+)') {
        $t = [datetime]::ParseExact($Matches[1], "yyyy-MM-dd HH:mm:ss.fff'Z'", $inv, [Globalization.DateTimeStyles]::AssumeUniversal -bor [Globalization.DateTimeStyles]::AdjustToUniversal)
        if ($null -eq $script:memT0) { $script:memT0 = $t }
        $row = [pscustomobject]@{
            minute = ($t - $script:memT0).TotalMinutes
            private = [long]$Matches[2]; ws = [long]$Matches[3]; peak = [long]$Matches[4]
            handles = [long]$Matches[5]; gdi = [long]$Matches[6]; user = [long]$Matches[7]
        }
        [void]$script:mem.Add($row)
        $script:lastMem = $row
        [IO.File]::AppendAllText($memCsv, ("{0},{1},{2},{3},{4},{5},{6},{7}`r`n" -f $Matches[1], $row.minute.ToString('0.00', $inv), $Matches[2], $Matches[3], $Matches[4], $Matches[5], $Matches[6], $Matches[7]), $utf8)
    }
    elseif ($line -match '^(\S+ \S+) \[\d+\] stats open n=(\d+) kind=(\w+) total_ms=([\d.]+) queue_ms=([\d.]+) work_ms=([\d.]+)') {
        $script:openCount++
        $kind = $Matches[3]
        if ($script:opens.ContainsKey($kind)) { [void]$script:opens[$kind].Add([double]::Parse($Matches[4], $inv)) }
        [IO.File]::AppendAllText($opensCsv, ("{0},{1},{2},{3},{4},{5}`r`n" -f $Matches[1], $Matches[2], $kind, $Matches[4], $Matches[5], $Matches[6]), $utf8)
    }
    elseif ($line -match '^\S+ \S+ \[\d+\] stats open') {
        # An open line without queue_ms (no timing of the request): counted, not measured.
        $script:openCount++
    }
    elseif ($line -match '(?i)panic') {
        $script:logErrors++
        Add-Error "log panic: $line"
    }
    elseif ($line -match '(?i)\b(error|cannot|failed)\b') {
        # Errors about the deliberately broken config are expected; anything else counts.
        if ((Get-Date) -le $script:expectedUntil -and $line -match 'config error') {
            $script:expectedErrors++
            Add-Error "log (expected, broken config): $line"
        }
        else {
            $script:logErrors++
            Add-Error "log: $line"
        }
    }
}

function Update-Collected {
    # Rotation: the file got smaller, so the rest of the unread part is in wheel.old.log.
    if ((Test-Path -LiteralPath $logPath) -and (Get-Item -LiteralPath $logPath).Length -lt $script:logOff) {
        $r = Read-Tail $oldLogPath $script:logOff
        foreach ($l in $r.lines) { Handle-LogLine $l }
        $script:logOff = 0L
    }
    $r = Read-Tail $logPath $script:logOff
    $script:logOff = $r.next
    foreach ($l in $r.lines) { Handle-LogLine $l }
    $r = Read-Tail $outPath $script:outOff
    $script:outOff = $r.next
    foreach ($l in $r.lines) {
        if ($l -eq 'cancelled') { $script:cancelCount++ }
        elseif ($l -match '^selected') { $script:selectedCount++ }
    }
}

# --- helpers ----------------------------------------------------------------------------------
function Get-Median($list) {
    if ($list.Count -eq 0) { return $null }
    $s = @($list | Sort-Object)
    $m = [int][math]::Floor($s.Count / 2)
    if ($s.Count % 2 -eq 1) { $s[$m] } else { ($s[$m - 1] + $s[$m]) / 2 }
}
function Fmt($v) { if ($null -eq $v) { 'n/a' } else { ([double]$v).ToString('0.000', $inv) } }
function Note-Progress([string]$text) {
    if ($Progress) {
        try { [IO.File]::AppendAllText($Progress, ("{0} INFO {1}`r`n" -f (Get-Date -Format 'HH:mm:ss'), $text), $utf8) } catch { }
    }
}

function Save-ArcShot($layoutFactor) {
    $size = [int][math]::Ceiling((292 + 4) * $layoutFactor * 2)
    $x = [math]::Max(0, $centerX - [int]($size / 2))
    $y = [math]::Max(0, $centerY - [int]($size / 2))
    $w = [math]::Min($size, $screenW - $x)
    $h = [math]::Min($size, $screenH - $y)
    $bmp = New-Object System.Drawing.Bitmap($w, $h)
    try {
        $g = [System.Drawing.Graphics]::FromImage($bmp)
        try { $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size($w, $h))) } finally { $g.Dispose() }
        $bmp.Save($arcPng, [System.Drawing.Imaging.ImageFormat]::Png)
    }
    finally { $bmp.Dispose() }
}

# --- run --------------------------------------------------------------------------------------
$started = Get-Date
$proc = $null
$ownProcess = $false
$dead = $false
$fatal = $null
$cycles = 0
$configChanges = 0
$brokenWrites = 0
$variantIndex = 0
$pendingFix = $null
$changeNo = 0
Write-Config $variants[0]
$current = $variants[0]

try {
    $argList = @('--run', '--accept-injected', '--stats', '--log', "`"$logPath`"", '--config', "`"$cfgPath`"")
    $proc = Start-Process -FilePath $Exe -ArgumentList $argList -RedirectStandardOutput $outPath -RedirectStandardError $errPath -PassThru -WindowStyle Hidden
    Start-Sleep -Milliseconds 1500
    if ($proc.HasExited) {
        $text = (@(Get-Content -LiteralPath $outPath -ErrorAction SilentlyContinue) -join ' ')
        Write-Host "wheel.exe exited at start (code $($proc.ExitCode)): $text"
        $fatal = "start failed: $text"
        exit 1
    }
    $ownProcess = $true
    Update-Collected
    Note-Progress "start dpi=$dpi screen=${screenW}x${screenH}"

    $end = $started.AddMinutes($Minutes)
    $nextChange = $started.AddMinutes($ConfigEveryMinutes)
    $nextProgress = $started.AddMinutes(1)

    while ((Get-Date) -lt $end) {
        $cycleStart = Get-Date
        if ($proc.HasExited) { $dead = $true; break }

        # Config changes: only between cycles.
        if (-not $NoConfigChanges) {
            if ($pendingFix -and (Get-Date) -ge $pendingFix.at) {
                Write-Config $pendingFix.variant
                $current = $pendingFix.variant
                $configChanges++
                $pendingFix = $null
                Start-Sleep -Milliseconds 1200
            }
            elseif (-not $pendingFix -and (Get-Date) -ge $nextChange) {
                $changeNo++
                $nextChange = (Get-Date).AddMinutes($ConfigEveryMinutes)
                $v = $variants[$changeNo % $variants.Count]
                if ($changeNo % 4 -eq 0) {
                    Write-BrokenConfig
                    $brokenWrites++
                    $pendingFix = @{ at = (Get-Date).AddSeconds(15); variant = $v }
                    $script:expectedUntil = (Get-Date).AddSeconds(30)
                }
                else {
                    Write-Config $v
                    $current = $v
                    $configChanges++
                }
                Start-Sleep -Milliseconds 1200
            }
        }

        # One cycle: open (hold), arc tile, then cancel (return to the center, or Escape).
        $cycles++
        $f = $dpi / 96.0 * $current.scale / 100.0
        $angle = $subParent * 360.0 / $current.outer
        $rad = $angle * [math]::PI / 180.0
        $ux = [math]::Sin($rad); $uy = -[math]::Cos($rad)
        $cancelBefore = $script:cancelCount
        $openBefore = $script:openCount
        $selBefore = $script:selectedCount
        try {
            Move-To $centerX $centerY
            Start-Sleep -Milliseconds 150
            Right-Down
            # Moving before the wheel is open would turn the press into a normal drag, so wait
            # for the "stats open" line (a busy machine can delay the hold timer).
            Start-Sleep -Milliseconds ($holdMs + 100)
            $openDeadline = (Get-Date).AddSeconds(2)
            Update-Collected
            while ($script:openCount -le $openBefore -and (Get-Date) -lt $openDeadline) {
                Start-Sleep -Milliseconds 50
                Update-Collected
            }
            Start-Sleep -Milliseconds 150
            # Outer ring of the sector with the submenu (arc opens), then the middle arc tile
            # (3 tiles: the middle one lies exactly on the sector's axis).
            Move-To ([int]($centerX + $ux * 170 * $f)) ([int]($centerY + $uy * 170 * $f))
            Start-Sleep -Milliseconds 300
            Move-To ([int]($centerX + $ux * 255 * $f)) ([int]($centerY + $uy * 255 * $f))
            Start-Sleep -Milliseconds 300
            if ($cycles -eq 1) {
                Start-Sleep -Milliseconds 400
                Save-ArcShot $f
            }
            if ($cycles % 5 -eq 0) {
                Press-Escape
                Start-Sleep -Milliseconds 200
                Right-Up
            }
            else {
                Move-To $centerX $centerY
                Start-Sleep -Milliseconds 150
                # Something else moved the pointer (a person at this PC): put it back first.
                $pos = [W]::CursorPos()
                if ([math]::Abs($pos[0] - $centerX) -gt 20 -or [math]::Abs($pos[1] - $centerY) -gt 20) {
                    $script:curX = $pos[0]; $script:curY = $pos[1]
                    Move-To $centerX $centerY
                    Start-Sleep -Milliseconds 100
                }
                Right-Up
            }
            # Wait for the result of the cycle.
            $deadline = (Get-Date).AddSeconds(3)
            do {
                Start-Sleep -Milliseconds 100
                Update-Collected
            } while (((Get-Date) -lt $deadline) -and -not ($script:cancelCount -gt $cancelBefore -and $script:openCount -gt $openBefore))
        }
        finally {
            if ($script:rightDown) { Press-Escape; Right-Up }
        }
        if ($proc.HasExited) { $dead = $true; Add-Error "cycle ${cycles}: wheel.exe is dead (exit code $($proc.ExitCode))"; break }
        $problems = @()
        if ($script:cancelCount -le $cancelBefore) { $problems += 'no "cancelled" on stdout' }
        if ($script:openCount -le $openBefore) { $problems += 'no "stats open" in the log' }
        if ($script:selectedCount -gt $selBefore) { $problems += 'unexpected selection' }
        if ($problems.Count -gt 0) {
            $script:cycleErrors++
            $under = try { (Get-Process -Id ([W]::PidAt($centerX, $centerY)) -ErrorAction Stop).ProcessName } catch { 'unknown (maybe elevated)' }
            Add-Error ("cycle {0}: {1}; window under the center: {2}; variant {3}/{4}/{5}" -f $cycles, ($problems -join '; '), $under, $current.theme, $current.outer, $current.scale)
        }

        if ((Get-Date) -ge $nextProgress) {
            $nextProgress = $nextProgress.AddMinutes(1)
            $m = if ($script:lastMem) { "private_kb=$($script:lastMem.private) handles=$($script:lastMem.handles)" } else { 'no mem yet' }
            Note-Progress "cycles=$cycles cycle_errors=$($script:cycleErrors) log_errors=$($script:logErrors) config_changes=$configChanges $m"
        }
        $rest = 2500 - ((Get-Date) - $cycleStart).TotalMilliseconds
        if ($rest -gt 0) { Start-Sleep -Milliseconds ([int]$rest) }
    }
}
catch {
    $fatal = "script error: $($_.Exception.Message)"
    Add-Error $fatal
}
finally {
    if ($script:rightDown) { Press-Escape; Right-Up }
    if ($ownProcess -and $proc) {
        if (-not $proc.HasExited) {
            & $Exe --quit | Out-Null
            [void]$proc.WaitForExit(3000)
            if (-not $proc.HasExited) { Stop-Process -Id $proc.Id -Force }
        }
        Start-Sleep -Milliseconds 300
    }
    if ($ownProcess) { try { Update-Collected } catch { } }
}
if ($null -ne $fatal -and -not $ownProcess) { exit 1 }

# --- summary ----------------------------------------------------------------------------------
$elapsed = (Get-Date) - $started
$warmMin = if ($Minutes -lt 10) { 1 } else { 5 }
$warm = @($script:mem | Where-Object { $_.minute -ge ($warmMin - 0.05) })
$fails = @()
if ($dead) { $fails += 'wheel.exe died' }
if ($fatal) { $fails += $fatal }
if ($script:cycleErrors -gt 0) { $fails += "cycle errors: $($script:cycleErrors)" }
if ($script:logErrors -gt 0) { $fails += "log errors/panics: $($script:logErrors) (errors.txt)" }

$s = New-Object System.Collections.Generic.List[string]
$s.Add(("soak: duration {0:hh\:mm\:ss}, requested {1} min, view {5}, dpi {2}, screen {3}x{4}" -f $elapsed, $Minutes, $dpi, $screenW, $screenH, $View))
$s.Add("cycles: $cycles, cycle errors: $($script:cycleErrors), log errors: $($script:logErrors), expected config errors: $($script:expectedErrors)")
$s.Add("config changes: good $configChanges, broken $brokenWrites")
$s.Add('open times (total_ms), median / max / count:')
foreach ($k in 'first', 'next', 'reload', 'dpi') {
    $l = $script:opens[$k]
    $max = if ($l.Count) { ($l | Measure-Object -Maximum).Maximum } else { $null }
    $s.Add(("  {0,-7} {1} / {2} / {3}" -f $k, (Fmt (Get-Median $l)), (Fmt $max), $l.Count))
}
$medNext = Get-Median $script:opens['next']
if ($null -eq $medNext) { $fails += 'no "next" opens measured' }
elseif ($medNext -gt 16) { $fails += "median next $(Fmt $medNext) ms > 16 ms" }

if ($warm.Count -eq 0) {
    $s.Add("memory: no sample after warm-up ($warmMin min); $($script:mem.Count) samples in all")
    $fails += 'no memory sample after warm-up'
}
else {
    $first = $warm[0]; $last = $warm[$warm.Count - 1]
    $pmax = ($script:mem | Measure-Object -Property private -Maximum).Maximum
    $growth = $last.private - $first.private
    $s.Add("private_kb after warm-up ($warmMin min): first $($first.private), last $($last.private), max $pmax, growth $growth")
    foreach ($c in @(@('handles', 'handles'), @('gdi', 'gdi'), @('user', 'user'))) {
        $st = $warm | Measure-Object -Property $c[1] -Minimum -Maximum
        $s.Add("$($c[0]) after warm-up: first $($first.$($c[1])), last $($last.$($c[1])), min $($st.Minimum), max $($st.Maximum)")
    }
    $hd = $last.handles - $first.handles
    $s.Add("handles difference last - first: $hd (allowed +-5)")
    if ($growth -gt 1024) { $fails += "private growth $growth KB > 1024 KB" }
    if ([math]::Abs($hd) -gt 5) { $fails += "handles changed by $hd (allowed +-5)" }
    if ($pmax -gt 15360) { $fails += "private max $pmax KB > 15360 KB" }
}
$verdict = if ($fails.Count -eq 0) { 'PASS' } else { 'FAIL' }
$s.Add("verdict: $verdict")
foreach ($f in $fails) { $s.Add("  - $f") }
[IO.File]::WriteAllLines($summaryTxt, $s, $utf8)
$s | ForEach-Object { Write-Host $_ }
Note-Progress "DONE verdict=$verdict"
exit $(if ($verdict -eq 'PASS') { 0 } else { 1 })
