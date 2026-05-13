param(
    [string[]]$Network,
    [switch]$Deep,
    [switch]$IncludeLow,
    [switch]$ListNetworks,
    [switch]$Watch,
    [double]$Interval = 30,
    [string]$Ports,
    [string]$Database,
    [switch]$NoDb,
    [int]$Threads = 128
)

$ErrorActionPreference = 'Stop'
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$Exe = Join-Path $ScriptDir 'bin\asic-discover.exe'

$ArgsList = @()
foreach ($Item in $Network) {
    $ArgsList += @('--network', $Item)
}
if ($Deep) {
    $ArgsList += '--deep'
}
if ($IncludeLow) {
    $ArgsList += '--include-low'
}
if ($ListNetworks) {
    $ArgsList += '--list-networks'
}
if ($Watch) {
    $ArgsList += '--watch'
    if ($Interval -gt 0) {
        $ArgsList += @('--interval', $Interval.ToString([System.Globalization.CultureInfo]::InvariantCulture))
    }
}
if ($Ports) {
    $ArgsList += @('--ports', $Ports)
}
if ($Database) {
    $ArgsList += @('--database', $Database)
}
if ($NoDb) {
    $ArgsList += '--no-db'
}
if ($Threads -gt 0) {
    $ArgsList += @('--threads', $Threads)
}

Push-Location $ScriptDir
try {
    if (Test-Path -LiteralPath $Exe) {
        & $Exe @ArgsList
    }
    else {
        $Cargo = Get-Command cargo -ErrorAction SilentlyContinue
        if (-not $Cargo) {
            Write-Error 'Compiled binary was not found and Cargo is not installed. Install Rust/Cargo or rebuild the utility on this machine.'
        }
        & $Cargo.Source run --release -- @ArgsList
    }
}
finally {
    Pop-Location
}
