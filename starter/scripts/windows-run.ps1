# Internal implementation. Public windows.ps1 wrappers forward an argument array
# so command flags such as Bash's -c cannot bind to PowerShell parameter names.
param(
    [string[]] $Command = @('cargo', '--version'),
    [string] $ProjectRoot = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = 'Stop'
if (-not $Command -or $Command.Count -eq 0) { $Command = @('cargo', '--version') }
$root = (Resolve-Path -LiteralPath $ProjectRoot).Path
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if ($env:CARGO_HOME) { $cargoBin = Join-Path $env:CARGO_HOME 'bin' }
if (-not (Test-Path (Join-Path $cargoBin 'cargo.exe'))) {
    throw 'Install native Windows Rust with rustup before running this script.'
}

$git = Get-Command git.exe -ErrorAction Stop
$gitRoot = Split-Path (Split-Path $git.Source -Parent) -Parent
$gitBin = Join-Path $gitRoot 'bin'
$bash = Join-Path $gitBin 'bash.exe'
if (-not (Test-Path $bash)) { throw 'Git for Windows Bash was not found.' }

$savedPath = $env:PATH
$savedBashEnv = $env:BASH_ENV
$savedTarget = $env:CARGO_TARGET_DIR
$savedJobs = $env:CARGO_BUILD_JOBS
try {
    # The app may have been opened before rustup updated the user's PATH.
    # Put Git Bash ahead of Windows' legacy bash.exe (which launches WSL).
    $env:PATH = "$cargoBin;$gitBin;$savedPath;" + [Environment]::GetEnvironmentVariable('Path', 'User')
    $env:BASH_ENV = (Join-Path $PSScriptRoot 'windows-env.sh').Replace('\', '/')
    if (-not $env:CARGO_TARGET_DIR) {
        $env:CARGO_TARGET_DIR = Join-Path $root 'target\native-windows'
    }
    # Keep simultaneous MSVC link jobs within a modest memory budget.
    if (-not $env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS = '2' }
    Push-Location $root
    try {
        if ($Command[0] -eq 'bash') {
            & $bash --noprofile --norc @($Command | Select-Object -Skip 1)
        } elseif ($Command[0].EndsWith('.sh')) {
            & $bash --noprofile --norc @Command
        } else {
            & $Command[0] @($Command | Select-Object -Skip 1)
        }
        $result = $LASTEXITCODE
    } finally {
        Pop-Location
    }
} finally {
    $env:PATH = $savedPath
    $env:BASH_ENV = $savedBashEnv
    $env:CARGO_TARGET_DIR = $savedTarget
    $env:CARGO_BUILD_JOBS = $savedJobs
}
exit $result
