# Installs ActivityMonitor as a Windows service (run ELEVATED: right-click
# PowerShell -> Run as administrator, then:  powershell -File install-service.ps1)
# Usage: install-service.ps1 [-Path C:\ActivityMonitor\publish2] [-Uninstall]
param(
    [string]$Path = (Join-Path $PSScriptRoot "publish2"),
    [switch]$Uninstall
)
$ErrorActionPreference = "Stop"
$name = "ActivityMonitor"
$exe = Join-Path $Path "activity-monitor-service.exe"
if (-not (Test-Path $exe)) { throw "service exe not found: $exe (run deploy.ps1 first)" }

if ($Uninstall) {
    sc.exe stop $name 2>$null
    sc.exe delete $name 2>$null
    Write-Host "Service '$name' removed (if it existed)."
    exit 0
}

if (Get-Service $name -ErrorAction SilentlyContinue) {
    Write-Host "Service '$name' already installed; restarting it."
    sc.exe stop $name | Out-Null
    Start-Sleep -Seconds 2
} else {
    # binPath must be quoted; the service runs in its own folder so
    # appsettings.json + Data\ resolve relative to the exe.
    sc.exe create $name binPath= "`"$exe`"" start= auto DisplayName= "Activity Monitor (Rust)"
}
sc.exe config $name start= auto | Out-Null
sc.exe start $name | Out-Null
sc.exe query $name
Write-Host "Logs: $Path\Logs\   DB: $Path\Data\ActivityData.db"
