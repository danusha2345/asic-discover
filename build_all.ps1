param(
    [string[]]$Targets,
    [switch]$InstallTargets,
    [switch]$KeepGoing
)

$ErrorActionPreference = 'Stop'
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$DistDir = Join-Path $ScriptDir 'dist'
$DefaultTargets = @(
    'x86_64-pc-windows-msvc',
    'x86_64-unknown-linux-musl',
    'aarch64-unknown-linux-musl',
    'armv7-unknown-linux-musleabihf'
)

if (-not $Targets -or $Targets.Count -eq 0) {
    $Targets = $DefaultTargets
}
else {
    $Targets = @(
        foreach ($Item in $Targets) {
            foreach ($Part in ($Item -split ',')) {
                $Trimmed = $Part.Trim()
                if ($Trimmed) {
                    $Trimmed
                }
            }
        }
    )
}

Push-Location $ScriptDir
try {
    $Cargo = Get-Command cargo -ErrorAction Stop
    $Rustup = Get-Command rustup -ErrorAction SilentlyContinue

    if ($InstallTargets -and -not $Rustup) {
        throw 'rustup was not found; cannot install Rust targets automatically.'
    }

    New-Item -ItemType Directory -Force -Path $DistDir | Out-Null
    $ShaFile = Join-Path $DistDir 'SHA256SUMS.txt'
    if (Test-Path -LiteralPath $ShaFile) {
        Remove-Item -LiteralPath $ShaFile -Force
    }

    $Failures = @()

    foreach ($Target in $Targets) {
        Write-Host "==> target: $Target"

        if ($InstallTargets) {
            & $Rustup.Source target add $Target
            if ($LASTEXITCODE -ne 0) {
                $Failures += "$Target target install failed"
                if (-not $KeepGoing) { break }
                continue
            }
        }

        & $Cargo.Source build --release --target $Target
        if ($LASTEXITCODE -ne 0) {
            $Failures += "$Target build failed"
            Write-Warning "Build failed for $Target. If the standard library target is missing, run: rustup target add $Target"
            Write-Warning "For linux-gnu targets you may also need a C cross-linker. Prefer musl targets for portable static builds."
            if (-not $KeepGoing) { break }
            continue
        }

        $ExeName = if ($Target -like '*windows*') { 'asic-discover.exe' } else { 'asic-discover' }
        $Built = Join-Path $ScriptDir "target\$Target\release\$ExeName"
        if (-not (Test-Path -LiteralPath $Built)) {
            $Failures += "$Target binary not found at $Built"
            if (-not $KeepGoing) { break }
            continue
        }

        $TargetDist = Join-Path $DistDir $Target
        New-Item -ItemType Directory -Force -Path $TargetDist | Out-Null
        $OutFile = Join-Path $TargetDist $ExeName
        Copy-Item -LiteralPath $Built -Destination $OutFile -Force
        Copy-Item -LiteralPath (Join-Path $ScriptDir 'README.md') -Destination $TargetDist -Force

        $Hash = Get-FileHash -Algorithm SHA256 -LiteralPath $OutFile
        Add-Content -LiteralPath $ShaFile -Value "$($Hash.Hash.ToLower())  $Target/$ExeName"
        Write-Host "    built: $OutFile"
    }

    if ($Failures.Count -gt 0) {
        Write-Warning 'Some targets failed:'
        foreach ($Failure in $Failures) {
            Write-Warning "  $Failure"
        }
        exit 1
    }

    Write-Host "Done. Artifacts are in: $DistDir"
}
finally {
    Pop-Location
}
