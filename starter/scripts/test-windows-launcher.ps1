# Focused launcher checks. No Rust compilation or network access.
$ErrorActionPreference = 'Stop'
$launcher = Join-Path $PSScriptRoot 'windows-run.ps1'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($launcher, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw $parseErrors[0] }
foreach ($name in @('Get-BracelBuildJobs', 'Test-BracelCargoCustomization')) {
    $definition = $ast.Find({ param($node) $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq $name }, $true)
    if (-not $definition) { throw "Missing launcher helper: $name" }
    . ([scriptblock]::Create($definition.Extent.Text))
}
function Assert-Equal($Actual, $Expected, [string]$Label) {
    if ($Actual -ne $Expected) { throw "$Label`: expected '$Expected', got '$Actual'" }
}
Assert-Equal (Get-BracelBuildJobs 32 16) 16 'Large memory budget'
Assert-Equal (Get-BracelBuildJobs 32 12) 16 'Twelve GiB boundary'
Assert-Equal (Get-BracelBuildJobs 32 11.99) 8 'Below twelve GiB'
Assert-Equal (Get-BracelBuildJobs 6 12) 6 'CPU cap at sixteen-job tier'
Assert-Equal (Get-BracelBuildJobs 32 8) 8 'Eight GiB boundary'
Assert-Equal (Get-BracelBuildJobs 32 4) 4 'Four GiB boundary'
Assert-Equal (Get-BracelBuildJobs 32 3.9) 2 'Small memory budget'
Assert-Equal (Get-BracelBuildJobs 1 12) 1 'CPU cap'
Assert-Equal (Get-BracelBuildJobs 32 -1) 2 'Unknown memory budget'

$project = Split-Path $PSScriptRoot -Parent
$fixture = Join-Path $project ('target\windows-launcher-tests\' + [guid]::NewGuid().ToString('N'))
$childRoot = Join-Path $fixture 'application'
$cargoHome = Join-Path $fixture 'cargo-home'
New-Item -ItemType Directory -Force -Path $childRoot, $cargoHome | Out-Null
Copy-Item -LiteralPath (Join-Path $project 'rust-toolchain.toml') -Destination $childRoot
Assert-Equal (Test-BracelCargoCustomization $childRoot $cargoHome @('cargo', 'check')) $false 'No Cargo customization'
Assert-Equal (Test-BracelCargoCustomization $childRoot $cargoHome @('cargo', 'check', '--target=x86_64-unknown-linux-gnu')) $true 'Explicit target'
Assert-Equal (Test-BracelCargoCustomization $childRoot $cargoHome @('cargo', '--config', 'build.jobs=2')) $true 'Inline Cargo config'
New-Item -ItemType Directory -Force -Path (Join-Path $fixture '.cargo') | Out-Null
$config = Join-Path $fixture '.cargo\config.toml'
Set-Content -LiteralPath $config -Value '[target.x86_64-pc-windows-msvc]', 'linker = "custom-linker.exe"'
Assert-Equal (Test-BracelCargoCustomization $childRoot $cargoHome @('cargo', 'check')) $true 'Ancestor linker config'
Set-Content -LiteralPath $config -Value '[build]', 'jobs = 3'
Assert-Equal (Test-BracelCargoCustomization $childRoot $cargoHome @('cargo', 'check')) $false 'Unrelated Cargo config'
Set-Content -LiteralPath (Join-Path $cargoHome 'config') -Value '[build]', 'rustflags = ["-C", "debuginfo=1"]'
Assert-Equal (Test-BracelCargoCustomization $childRoot $cargoHome @('cargo', 'check')) $true 'Cargo home legacy config'

$probe = Join-Path $fixture 'probe.ps1'
Set-Content -LiteralPath $probe -Value @'
param([string] $Output)
@{
    jobs = $env:CARGO_BUILD_JOBS
    linker = $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER
    nasm = $env:AWS_LC_SYS_PREBUILT_NASM
    path = $env:PATH
    target = $env:CARGO_TARGET_DIR
} | ConvertTo-Json | Set-Content -LiteralPath $Output
exit 0
'@
$compilerOverrides = @('RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTC', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')
$names = @('PATH', 'BASH_ENV', 'BRACEL_WINDOWS_PATH', 'CARGO_TARGET_DIR', 'CARGO_BUILD_JOBS', 'CARGO_BUILD_TARGET', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS', 'BRACEL_LINKER', 'AWS_LC_SYS_PREBUILT_NASM') + $compilerOverrides
$saved = @{}
foreach ($name in $names) { $saved[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
$shell = (Get-Command powershell.exe -ErrorAction Stop).Source
function Invoke-Probe([string]$Label, [string[]]$ExtraArguments = @()) {
    $output = Join-Path $fixture ($Label + '.json')
    $before = @{}
    foreach ($name in $names) { $before[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    & $launcher -ProjectRoot $childRoot -Command (@($shell, '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', $probe, $output) + $ExtraArguments)
    Assert-Equal $LASTEXITCODE 0 "$Label exit status"
    foreach ($name in $names) { Assert-Equal ([Environment]::GetEnvironmentVariable($name, 'Process')) $before[$name] "$Label restores $name" }
    return Get-Content -LiteralPath $output -Raw | ConvertFrom-Json
}
try {
    foreach ($name in $names | Where-Object { $_ -ne 'PATH' }) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
    # Deterministic memory discovery while keeping real installed Rust and Git.
    function Get-CimInstance { [pscustomobject]@{ FreePhysicalMemory = 12 * 1024 * 1024 } }
    $default = Invoke-Probe 'defaults'
    $env:BRACEL_WINDOWS_PATH = 'caller-value'
    & $launcher -ProjectRoot $childRoot -Command @('bash', '-c', 'command -v dirname && command -v diff && bash -c ''command -v cargo && command -v dirname''') | Out-Null
    Assert-Equal $LASTEXITCODE 0 'Bash and nested Bash resolve Git utilities and Cargo'
    & $launcher -ProjectRoot $childRoot -Command @('bash', '-c', 'export PATH=/bracel-test-marker:$PATH; bash -c ''[[ $PATH == /bracel-test-marker:* ]]''') | Out-Null
    Assert-Equal $LASTEXITCODE 0 'Nested Bash preserves user PATH additions'
    Assert-Equal $env:BRACEL_WINDOWS_PATH 'caller-value' 'Bash restores path bridge'
    $env:BRACEL_WINDOWS_PATH = $null
    Assert-Equal $default.jobs ([Math]::Min([Environment]::ProcessorCount, 16)).ToString() 'Default jobs'
    if (-not (Test-BracelCargoCustomization $childRoot $env:CARGO_HOME @($shell))) {
        if ($default.linker -notlike '*\rust-lld.exe') { throw 'Default linker did not select bundled LLD' }
        if (-not (Get-Command nasm.exe -ErrorAction SilentlyContinue)) {
            Assert-Equal $default.nasm '1' 'Bundled assembly when NASM is absent'
        }
    }
    $env:CARGO_BUILD_JOBS = '3'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = 'chosen-linker.exe'
    $env:AWS_LC_SYS_PREBUILT_NASM = '0'
    $explicit = Invoke-Probe 'explicit'
    Assert-Equal $explicit.jobs '3' 'Explicit jobs'
    Assert-Equal $explicit.linker 'chosen-linker.exe' 'Explicit linker'
    Assert-Equal $explicit.nasm '0' 'Explicit NASM setting'
    $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER = $null
    $env:BRACEL_LINKER = 'msvc'
    Assert-Equal (Invoke-Probe 'msvc').linker $null 'MSVC opt out'
    $env:BRACEL_LINKER = $null
    Assert-Equal (Invoke-Probe 'toolchain-override' @('+nightly')).linker $null 'Explicit toolchain'
    foreach ($flag in (@('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'CARGO_BUILD_RUSTFLAGS', 'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS', 'CARGO_BUILD_TARGET') + $compilerOverrides)) {
        [Environment]::SetEnvironmentVariable($flag, 'user-value', 'Process')
        Assert-Equal (Invoke-Probe $flag).linker $null "Respect $flag"
        Remove-Item "Env:$flag" -ErrorAction SilentlyContinue
    }
    Set-Content -LiteralPath $config -Value '[target.x86_64-pc-windows-msvc]', 'rustflags = ["-C", "linker=custom.exe"]'
    Assert-Equal (Invoke-Probe 'config-linker').linker $null 'Configured linker flags'
    Set-Content -LiteralPath $config -Value '[build]', 'jobs = 3'
    Remove-Item Function:\Get-CimInstance
    function Get-CimInstance { throw 'Simulated unavailable memory query' }
    $env:CARGO_BUILD_JOBS = $null
    Assert-Equal (Invoke-Probe 'memory-fallback').jobs ([Math]::Min([Environment]::ProcessorCount, 2)).ToString() 'Memory query fallback'
    $location = (Get-Location).Path
    $before = @{}
    foreach ($name in $names) { $before[$name] = [Environment]::GetEnvironmentVariable($name, 'Process') }
    $failed = $false
    try { & $launcher -ProjectRoot $childRoot -Command @('bracel-deliberately-missing-command.exe') }
    catch { $failed = $true }
    Assert-Equal $failed $true 'Command failure surfaced'
    Assert-Equal (Get-Location).Path $location 'Failure restores working directory'
    foreach ($name in $names) { Assert-Equal ([Environment]::GetEnvironmentVariable($name, 'Process')) $before[$name] "Failure restores $name" }
} finally {
    Remove-Item Function:\Get-CimInstance -ErrorAction SilentlyContinue
    foreach ($name in $names) {
        if ($null -eq $saved[$name]) { Remove-Item "Env:$name" -ErrorAction SilentlyContinue }
        else { [Environment]::SetEnvironmentVariable($name, $saved[$name], 'Process') }
    }
}
Write-Output "PASS: Windows launcher settings, overrides, config detection, and environment restoration. Probe artifacts: $fixture"
