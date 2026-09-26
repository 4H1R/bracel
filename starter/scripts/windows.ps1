# Run native Windows Cargo or a project shell script from the application root.
# Quote Cargo's separator as '--' when passing flags through PowerShell.
$root = Split-Path $PSScriptRoot -Parent
& (Join-Path $PSScriptRoot 'windows-run.ps1') -ProjectRoot $root -Command $args
exit $LASTEXITCODE
