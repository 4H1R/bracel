# Use the starter's exportable Windows launcher for the framework workspace.
$root = Split-Path $PSScriptRoot -Parent
& (Join-Path $root 'starter\scripts\windows-run.ps1') -ProjectRoot $root -Command $args
exit $LASTEXITCODE
