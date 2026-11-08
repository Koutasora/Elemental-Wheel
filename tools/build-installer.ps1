# Builds the release binaries and the Inno Setup installer. Run from any folder.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

$iscc = @(
    (Join-Path $env:LOCALAPPDATA 'Programs\Inno Setup 6\ISCC.exe'),
    (Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe')
) | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $iscc) {
    throw 'Inno Setup 6 (ISCC.exe) not found in %LOCALAPPDATA%\Programs or %ProgramFiles(x86)%.'
}

Push-Location $root
try {
    cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed (exit code $LASTEXITCODE)." }

    $cargoToml = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
    if ($cargoToml -notmatch '(?m)^version\s*=\s*"([^"]+)"') { throw 'No version found in Cargo.toml.' }
    $cargoVersion = $Matches[1]

    $exe = Join-Path $root 'target\release\wheel.exe'
    # wheel.exe is a GUI-subsystem program: PowerShell waits for it and captures its output only through a pipe.
    $exeVersion = ((& $exe --version | Out-String).Trim()) -replace '^.*\s', ''
    if ($exeVersion -ne $cargoVersion) {
        throw "Version mismatch: wheel.exe reports '$exeVersion', Cargo.toml has '$cargoVersion'."
    }

    & $iscc "/DAppVersion=$cargoVersion" (Join-Path $root 'installer\elemental-wheel.iss')
    if ($LASTEXITCODE -ne 0) { throw "ISCC failed (exit code $LASTEXITCODE)." }

    $setup = Get-Item -LiteralPath (Join-Path $root "target\installer\Elemental-Wheel-$cargoVersion-setup.exe")
    Write-Host "Version:   $cargoVersion"
    Write-Host "Installer: $($setup.FullName)"
    Write-Host ("Size:      {0:N0} bytes ({1:N2} MB)" -f $setup.Length, ($setup.Length / 1MB))
}
finally {
    Pop-Location
}
