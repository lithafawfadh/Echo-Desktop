# Environment activation for Echo Music (100% portable on D: drive)
$scriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path

$env:RUSTUP_HOME = Join-Path $scriptDir ".rustup"
$env:CARGO_HOME = Join-Path $scriptDir ".cargo"
$cargoBin = Join-Path $env:CARGO_HOME "bin"

if (Test-Path $cargoBin) {
    if ($env:PATH -notlike "*$cargoBin*") {
        $env:PATH = "$cargoBin;$env:PATH"
    }
}

Write-Host "Echo Music environment loaded on D: drive." -ForegroundColor Green
Write-Host "CARGO_HOME: $env:CARGO_HOME"
Write-Host "RUSTUP_HOME: $env:RUSTUP_HOME"
