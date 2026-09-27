# Install the prebuilt Bracel CLI. No administrator privileges or Rust required.
param(
    [string] $Version = '',
    [string] $InstallDir = (Join-Path $env:LOCALAPPDATA 'Bracel\bin'),
    [switch] $NoPath
)
$ErrorActionPreference = 'Stop'
if ([Environment]::Is64BitOperatingSystem -eq $false -or $env:PROCESSOR_ARCHITECTURE -eq 'ARM64') {
    throw 'This installer supports Windows x64. Other targets can build bracel-cli with Cargo.'
}
if (-not $Version) {
    $release = Invoke-RestMethod -Uri 'https://api.github.com/repos/4H1R/bracel/releases/latest'
    $Version = $release.tag_name
}
$Version = $Version -replace '^v', ''
if ($Version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$') { throw 'Expected a stable version such as 0.4.0.' }
$InstallDir = [IO.Path]::GetFullPath($InstallDir)
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$stage = Join-Path $InstallDir ('.install-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    $asset = 'bracel-x86_64-pc-windows-msvc.exe'
    $base = "https://github.com/4H1R/bracel/releases/download/v$Version"
    $download = Join-Path $stage $asset
    $sums = Join-Path $stage 'SHA256SUMS'
    Invoke-WebRequest -Uri "$base/SHA256SUMS" -OutFile $sums
    Invoke-WebRequest -Uri "$base/$asset" -OutFile $download
    $checksumLines = @(Get-Content -LiteralPath $sums | Where-Object { $_ -match ('^[a-fA-F0-9]{64}\s+\*?' + [regex]::Escape($asset) + '$') })
    $sha = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($download)
    try { $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '').ToLowerInvariant() }
    finally { $stream.Dispose(); $sha.Dispose() }
    if ($checksumLines.Count -ne 1 -or ($checksumLines[0] -split '\s+')[0].ToLowerInvariant() -ne $actual) { throw 'Release checksum mismatch. Existing installation preserved.' }
    $reported = & $download --version
    if ($LASTEXITCODE -ne 0 -or $reported -ne "bracel $Version") { throw 'Downloaded CLI has an unexpected version. Existing installation preserved.' }
    $installed = Join-Path $InstallDir 'bracel.exe'
    $previous = Join-Path $InstallDir 'bracel.previous'
    if (Test-Path -LiteralPath $previous) { Remove-Item -LiteralPath $previous }
    if (Test-Path -LiteralPath $installed) { Move-Item -LiteralPath $installed -Destination $previous }
    try { Move-Item -LiteralPath $download -Destination $installed }
    catch {
        if (Test-Path -LiteralPath $previous) { Move-Item -LiteralPath $previous -Destination $installed }
        throw
    }
    if (-not $NoPath) {
        $userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (($userPath -split ';') -notcontains $InstallDir) {
            [Environment]::SetEnvironmentVariable('Path', "$InstallDir;$userPath", 'User')
        }
        $env:Path = "$InstallDir;$env:Path"
    }
    Write-Host "Installed Bracel $Version to $installed"
    Write-Host 'Open a new terminal, then run: bracel setup'
    Write-Host 'New projects use Docker. Install Git and Docker Desktop when setup requests them.'
} finally {
    # Resolve and verify the owned temporary directory before recursive cleanup.
    $resolvedStage = [IO.Path]::GetFullPath($stage)
    if ($resolvedStage.StartsWith($InstallDir.TrimEnd('\') + '\', [StringComparison]::OrdinalIgnoreCase) -and
        [IO.Path]::GetFileName($resolvedStage).StartsWith('.install-')) {
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force -ErrorAction SilentlyContinue
    }
}
