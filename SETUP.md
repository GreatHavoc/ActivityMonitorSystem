# Activity Monitor — Setup Guide (Rust port)

## Prerequisites

1. **Windows 10/11 (64-bit)** — no .NET runtime needed (static CRT build)
2. **Ollama** — [ollama.ai](https://ollama.ai), with the model pulled:
   ```bash
   ollama pull qwen3-vl:2b
   curl http://localhost:11434/api/tags   # must list qwen3-vl:2b
   ```
3. **Rust toolchain** — only if building from source

## Quick start (prebuilt)

```cmd
xcopy /s /i publish2 C:\ActivityMonitor\publish2
cd C:\ActivityMonitor\publish2
activity-monitor-service.exe        REM run manually first; Ctrl+C to stop
activity-monitor-cli.exe stats      REM in another terminal
```

## Quick start (from source)

```cmd
cargo build --release --workspace
powershell -File deploy.ps1
cd publish2
activity-monitor-service.exe
```

## Auto-start

1. `Win + R`, type `shell:startup`
2. Shortcut to `C:\ActivityMonitor\publish2\activity-monitor-service.exe`,
   named "Activity Monitor"

Optional real Windows service:
```cmd
sc create "Activity Monitor Service" binPath= "C:\ActivityMonitor\publish2\activity-monitor-service.exe --service" start= auto
sc start "Activity Monitor Service"
```

## Configuration

Edit `publish2\appsettings.json` — see README.md for the full annotated
schema. `Storage.DatabasePath` is relative to the working directory; run the
service from the publish2 folder. Logs: `<exe_dir>\Logs\`.

## Uninstall

```cmd
taskkill /f /im activity-monitor-service.exe
sc delete "Activity Monitor Service"        REM only if installed as a service
REM Win+R, shell:startup, delete the "Activity Monitor" shortcut
rmdir /s "C:\ActivityMonitor"
```

## Troubleshooting

- CLI says "Database not found!" → run it from the folder with
  `Data\ActivityData.db`, or start the service first.
- No AI rows → Ollama not running / model not pulled; the service logs the
  connection error and keeps running.
- Logs: `publish2\Logs\activity-monitor.YYYY-MM-DD`.
