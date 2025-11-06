# Activity Monitor - Complete Setup from Scratch

## Prerequisites Check

Before starting, verify you have:
- [ ] Windows 10/11
- [ ] .NET 8.0 SDK installed
- [ ] Administrator access
- [ ] Ollama installed

## Step 1: Verify .NET SDK

```cmd
dotnet --version
```

Should show version 8.0.x or higher. If not, download from: https://dotnet.microsoft.com/download/dotnet/8.0

## Step 2: Install Ollama

1. Download from: https://ollama.ai
2. Install the application
3. Verify installation:

```cmd
ollama --version
```

## Step 3: Pull the AI Model

```cmd
ollama pull qwen2.5vl:3b
```

Wait for download to complete (~2-3GB). Verify:

```cmd
ollama list
```

You should see `qwen2.5vl:3b` in the list.

## Step 4: Clean Build Everything

```cmd
cd C:\ActivityMonitor

REM Clean everything
dotnet clean
rd /s /q publish
rd /s /q bin
rd /s /q obj

REM Restore packages
dotnet restore

REM Build in Release mode
dotnet build -c Release
```

Check for errors. All projects should build successfully.

## Step 5: Publish Applications

```cmd
cd C:\ActivityMonitor

REM Publish both service and CLI to the same folder
dotnet publish ActivityMonitor.Service\ActivityMonitor.Service.csproj -c Release -o publish
dotnet publish ActivityMonitor.CLI\ActivityMonitor.CLI.csproj -c Release -o publish
```

## Step 7: Verify Published Files

```cmd
cd publish
dir *.exe
```

You should see:
- `ActivityMonitor.Service.exe`
- `ActivityMonitor.CLI.exe`

## Step 8: Copy Configuration

```cmd
cd C:\ActivityMonitor\publish

REM Copy appsettings.json if not already there
copy ..\ActivityMonitor.Service\appsettings.json . /Y
```

## Step 8: Verify Configuration

```cmd
type appsettings.json
```

Make sure it has the current configuration:

```json
{
  "ActivityMonitor": {
    "SamplingIntervalSeconds": 5,
    "IdleThresholdSeconds": 300,
    "CaptureSettings": {
      "FrameRate": 1,
      "MaxDurationSeconds": 30,
      "TriggerOnFocusChange": true,
      "TriggerOnIdleResume": true,
      "MaxFramesPerCapture": 1
    },
    "OllamaEndpoint": "http://localhost:11434",
    "OllamaModel": "qwen2.5vl:3b",
    "QueueSettings": {
      "MaxConcurrentTasks": 4,
      "MaxQueueSize": 100,
      "HighPriorityThreshold": 10,
      "ProcessingTimeoutSeconds": 0
    },
    "Storage": {
      "DatabasePath": "Data\\ActivityData.db",
      "RetainFrames": false,
      "CompactionIntervalHours": 24,
      "MaxEventAgeDays": 90
    }
  }
}
```

## Step 10: Test Ollama is Running

```cmd
curl http://localhost:11434/api/tags
```

Should return JSON with your models. If it fails, start Ollama.

## Step 10: Create Required Directories

```cmd
cd C:\ActivityMonitor\publish

mkdir Data 2>nul
```

## Step 12: Test Service Manually First

```cmd
cd C:\ActivityMonitor\publish

REM Run service in console mode to see errors
ActivityMonitor.Service.exe
```

You should see:
```
[INF] Starting Activity Monitor Service
[INF] Activity Monitor Service starting...
[INF] Initializing database at Data\ActivityData.db
[INF] Database initialized successfully
[INF] Activity Monitor Service started successfully
```

Press `Ctrl+C` to stop it.

## Step 13: Check Database Was Created

```cmd
dir Data\ActivityData.db
```

Should show the database file.

## Step 14: Test CLI

```cmd
ActivityMonitor.CLI.exe stats
```

Should show:
```
Total Events     │ 0
Total Inferences │ 0
Events Today     │ 0
```

## Step 14: Configure Auto-Start

Create a shortcut in Windows Startup folder for automatic startup:

1. Press `Win + R`, type `shell:startup`
2. Create shortcut to `C:\ActivityMonitor\publish\ActivityMonitor.Service.exe`
3. Name it "Activity Monitor"

## Step 15: Test Auto-Start Setup

```cmd
REM Test that the service starts from the shortcut path
C:\ActivityMonitor\publish\ActivityMonitor.Service.exe
```

Service should start successfully. Press `Ctrl+C` to stop.

## Step 16: Wait and Test

Wait 3-5 minutes for the service to capture some activity, then:

```cmd
cd C:\ActivityMonitor\publish

REM Check basic stats
ActivityMonitor.CLI.exe stats

REM View timeline
ActivityMonitor.CLI.exe timeline --date 2024-01-15

REM Detailed view
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 10

REM Daily summary
ActivityMonitor.CLI.exe summary --date 2024-01-15

# Export report
ActivityMonitor.CLI.exe report --date 2024-01-15 --output test_report.json
```

## Verification Checklist

### Issue: "Dll was not found" error

**Solution**: Make sure you built with SQLitePCLRaw.bundle_e_sqlite3 package.

Check `ActivityMonitor.Core\ActivityMonitor.Core.csproj` has:
```xml
<PackageReference Include="SQLitePCLRaw.bundle_e_sqlite3" Version="2.1.10" />
```

If missing, add it and rebuild.

### Issue: "model 'qwen2.5vl:3b' not found"

**Solution**:
```cmd
ollama pull qwen2.5vl:3b
ollama list
```

### Issue: Service won't start

**Solution**: Check Windows Event Viewer for errors (Windows Logs → Application), or run manually to see console output.

### Issue: Database not found

**Solution**: 
```cmd
REM Check if service created it
dir C:\ActivityMonitor\publish\Data\ActivityData.db

REM If not, service may not be running - check startup shortcut
```

### Issue: No inference results

**Causes**:
1. Ollama not running
2. Model not downloaded
3. Not enough activity captured yet

**Solution**:
```cmd
REM Check Ollama
curl http://localhost:11434/api/tags

REM Check service logs
type C:\ActivityMonitor\publish\Logs\activitymonitor-*.log

REM Wait longer (3-5 minutes) for periodic capture
```

## Verification Checklist

After setup, verify:

- [ ] .NET 8.0 SDK installed
- [ ] Ollama installed and running
- [ ] Model `qwen2.5vl:3b` downloaded
- [ ] All projects build without errors
- [ ] Service.exe and CLI.exe in publish folder
- [ ] appsettings.json in publish folder
- [ ] Data folder created
- [ ] Database file exists: `Data\ActivityData.db`
- [ ] Startup shortcut created and tested
- [ ] CLI shows stats (even if 0)
- [ ] After 5 minutes, inference results appear

## Expected Timeline After Starting Service

```
00:00 - Service starts
00:05 - First focus change captured
00:35 - First screen capture completed
00:36 - First Ollama analysis (if model is loaded)
03:00 - First periodic capture
05:00 - Multiple activities logged
```

## Quick Start Commands (After Setup)

```cmd
cd C:\ActivityMonitor\publish

REM View today's timeline
ActivityMonitor.CLI.exe timeline --date 2024-01-15

REM Detailed analysis
ActivityMonitor.CLI.exe detailed --date 2024-01-15 --limit 20

REM Daily summary
ActivityMonitor.CLI.exe summary --date 2024-01-15

REM Export JSON report
ActivityMonitor.CLI.exe report --date 2024-01-15 --output report.json

REM Quick stats
ActivityMonitor.CLI.exe stats
```

## Complete Fresh Install Script

Save this as `fresh_install.bat`:

```batch
@echo off
echo ================================================
echo Activity Monitor - Fresh Installation
echo ================================================
echo.

echo Step 1: Checking Ollama...
ollama --version
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Ollama not found! Install from https://ollama.ai
    pause
    exit /b 1
)

echo Step 2: Pulling AI model...
ollama pull qwen2.5vl:3b

echo Step 3: Clean build...
cd C:\ActivityMonitor
dotnet clean
if exist publish rd /s /q publish

echo Step 4: Build solution...
dotnet build -c Release
if %ERRORLEVEL% NEQ 0 (
    echo [ERROR] Build failed!
    pause
    exit /b 1
)

echo Step 5: Publish applications...
dotnet publish ActivityMonitor.Service\ActivityMonitor.Service.csproj -c Release -o publish
dotnet publish ActivityMonitor.CLI\ActivityMonitor.CLI.csproj -c Release -o publish

echo Step 6: Setup directories...
cd publish
if not exist Data mkdir Data

echo Step 7: Copy config...
copy ..\ActivityMonitor.Service\appsettings.json . /Y

echo.
echo ================================================
echo Installation Complete!
echo ================================================
echo.
echo Next steps:
echo 1. Create startup shortcut: Win+R, shell:startup
echo 2. Add shortcut to C:\ActivityMonitor\publish\ActivityMonitor.Service.exe
echo 3. Wait 5 minutes, then run:
echo    cd C:\ActivityMonitor\publish
echo    ActivityMonitor.CLI.exe timeline --date 2024-01-15
echo.
pause
```

Run with:
```cmd
fresh_install.bat
```

---

**You're now ready to start from the beginning!** 🚀

Follow the steps above in order, and you'll have a fully working Activity Monitor system.
