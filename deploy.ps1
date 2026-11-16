# Deploys the Rust build to publish2\ (leaves the .NET publish\ untouched).
# Usage: powershell -File deploy.ps1   (from the repo root)
$ErrorActionPreference = "Stop"
$root = $PSScriptRoot
cargo build --release --workspace
$dest = Join-Path $root "publish2"
New-Item -ItemType Directory -Force -Path $dest | Out-Null
Copy-Item "$root\target\release\activity-monitor-service.exe" $dest -Force
Copy-Item "$root\target\release\activity-monitor-cli.exe" $dest -Force
Copy-Item "$root\target\release\activity-monitor-mcp.exe" $dest -Force
# appsettings.json already lives in publish2\ — nothing to copy.
Write-Host "Deployed to $dest :"
Get-ChildItem $dest | ForEach-Object { "  $($_.Name)" }
Write-Host "Run from $dest : .\activity-monitor-service.exe   (Ctrl+C to stop)"
Write-Host "CLI:               .\activity-monitor-cli.exe stats"
Write-Host "MCP server:        .\activity-monitor-mcp.exe   (stdio; --http 8765 for Streamable HTTP on 127.0.0.1)"
