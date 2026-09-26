# Internal implementation. Public windows.ps1 wrappers forward an argument array
# so command flags such as Bash's -c cannot bind to PowerShell parameter names.
param(
    [string[]] $Command = @('cargo', '--version'),
    [string] $ProjectRoot = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = 'Stop'

function Get-BracelBuildJobs([int]$ProcessorCount, [double]$FreeMemoryGiB) {
    $limit = 2
    if ($FreeMemoryGiB -ge 12) { $limit = 16 }
    elseif ($FreeMemoryGiB -ge 8) { $limit = 8 }
    elseif ($FreeMemoryGiB -ge 4) { $limit = 4 }
    return [Math]::Max(1, [Math]::Min($ProcessorCount, $limit))
}

function Test-BracelCargoCustomization([string]$Root, [string]$CargoHome, [string[]]$Arguments) {
    # A target/config argument may be forwarded through a shell. Avoid guessing
    # whether Cargo will apply it to this command or a nested command.
    if (($Arguments -join ' ') -match '(?:^|\s)--(?:target|config)(?:=|\s|$)') { return $true }
    $directories = @()
    $directory = [System.IO.DirectoryInfo]$Root
    while ($directory) {
        $directories += Join-Path $directory.FullName '.cargo'
        $directory = $directory.Parent
    }
    if (-not $CargoHome) { $CargoHome = Join-Path $env:USERPROFILE '.cargo' }
    $directories += $CargoHome
    foreach ($directory in $directories | Select-Object -Unique) {
        foreach ($name in @('config.toml', 'config')) {
            $path = Join-Path $directory $name
            if (Test-Path -LiteralPath $path) {
                try {
                    $contents = Get-Content -LiteralPath $path -Raw -ErrorAction Stop
                    # Deliberately conservative, including quoted/dotted keys and
                    # comments mentioning a linker. Leave custom setups alone.
                    if ($contents -match '(?im)\b(linker|rustflags|rustc|rustc-wrapper|rustc-workspace-wrapper|target|include)\b\s*["'']?\s*=') { return $true }
                } catch { return $true }
            }
        }
    }
    return $false
}

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
$gitUsrBin = Join-Path $gitRoot 'usr\bin'
$bash = Join-Path $gitBin 'bash.exe'
if (-not (Test-Path $bash)) { throw 'Git for Windows Bash was not found.' }

$savedPath = $env:PATH
$savedBashEnv = $env:BASH_ENV
$savedWindowsPath = $env:BRACEL_WINDOWS_PATH
$savedTarget = $env:CARGO_TARGET_DIR
$savedJobs = $env:CARGO_BUILD_JOBS
$savedLinker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER
$savedNasm = $env:AWS_LC_SYS_PREBUILT_NASM
try {
    # The app may have been opened before rustup updated the user's PATH.
    # Put Git Bash ahead of Windows' legacy bash.exe (which launches WSL).
    $toolPaths = @($cargoBin, $gitBin)
    if (Test-Path -LiteralPath $gitUsrBin) { $toolPaths += $gitUsrBin }
    $env:PATH = ($toolPaths -join ';') + ";$savedPath;" + [Environment]::GetEnvironmentVariable('Path', 'User')
    # Some desktop parents supply both Path and PATH. Git Bash may read the
    # stale spelling, so BASH_ENV imports the effective path explicitly.
    $env:BRACEL_WINDOWS_PATH = $env:PATH
    $env:BASH_ENV = (Join-Path $PSScriptRoot 'windows-env.sh').Replace('\', '/')
    if (-not $env:CARGO_TARGET_DIR) {
        $env:CARGO_TARGET_DIR = Join-Path $root 'target\native-windows'
    }
    if (-not $env:CARGO_BUILD_JOBS) {
        $freeMemoryGiB = -1
        try {
            $memory = Get-CimInstance -ClassName Win32_OperatingSystem -ErrorAction Stop
            $freeMemoryGiB = [double]$memory.FreePhysicalMemory / (1024 * 1024)
        } catch { }
        $env:CARGO_BUILD_JOBS = (Get-BracelBuildJobs ([Environment]::ProcessorCount) $freeMemoryGiB).ToString()
    }
    Push-Location $root
    try {
        $customCargo = Test-BracelCargoCustomization $root $env:CARGO_HOME $Command
        $explicitFlags = $false
        foreach ($name in @('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS')) {
            if ($null -ne [Environment]::GetEnvironmentVariable($name, 'Process')) { $explicitFlags = $true }
        }
        $customCompiler = ($Command -join ' ') -match '(?:^|\s)\+\S+'
        foreach ($name in @('RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')) {
            if ($null -ne [Environment]::GetEnvironmentVariable($name, 'Process')) { $customCompiler = $true }
        }
        # rustup resolves this directory's pinned toolchain. Never set a linker
        # for another architecture or for an explicitly selected build target.
        if (-not $customCargo -and -not $customCompiler -and -not $env:CARGO_BUILD_TARGET) {
            $rustc = Join-Path $cargoBin 'rustc.exe'
            $version = & $rustc -vV 2>$null
            if ($LASTEXITCODE -eq 0 -and $version -contains 'host: x86_64-pc-windows-msvc') {
                if (-not $savedLinker -and -not $explicitFlags -and $env:BRACEL_LINKER -ne 'msvc') {
                    $sysroot = & $rustc --print sysroot 2>$null
                    if ($LASTEXITCODE -eq 0 -and $sysroot) {
                        $lld = Join-Path ($sysroot | Select-Object -Last 1) 'lib\rustlib\x86_64-pc-windows-msvc\bin\rust-lld.exe'
                        if (Test-Path -LiteralPath $lld) { $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $lld }
                    }
                }
                # The pinned AWS-LC Windows x64 package includes NASM objects.
                if ($null -eq $savedNasm -and -not (Get-Command nasm.exe -ErrorAction SilentlyContinue)) {
                    $env:AWS_LC_SYS_PREBUILT_NASM = '1'
                }
            }
        }
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
    $env:BRACEL_WINDOWS_PATH = $savedWindowsPath
    $env:CARGO_TARGET_DIR = $savedTarget
    $env:CARGO_BUILD_JOBS = $savedJobs
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $savedLinker
    $env:AWS_LC_SYS_PREBUILT_NASM = $savedNasm
}
exit $result
