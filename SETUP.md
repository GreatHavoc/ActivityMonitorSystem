# Activity Monitor — Setup Guide (Rust port)

## Prerequisites

1. **Windows 10/11 (64-bit)** — no .NET runtime needed (static CRT build)
2. **Rust toolchain** — only if building from source

Ollama does **not** need to be pre-installed: on first run the service
downloads it (official GitHub release zip into `%LOCALAPPDATA%\Programs\Ollama`),
starts `ollama serve`, and pulls `qwen3-vl:2b` automatically. If you prefer to
install it yourself: [ollama.ai](https://ollama.ai), `ollama pull qwen3-vl:2b`,
then verify `curl http://localhost:11434/api/tags` lists the model.

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

## MCP server (let an AI agent query your activity)

`activity-monitor-mcp.exe` speaks MCP spec 2026-07-28 (dual-era: also the
legacy 2025-11-25/2025-06-18 handshake) over stdio, and optionally Streamable
HTTP on `127.0.0.1` with `--http [port]` (default 8765). It opens the database
read-only, so it never blocks the service. Five read-only tools:
`todays_timeline`, `session_summary`, `activity_at`, `daily_report`,
`ask_about_my_day` (uses the Ollama endpoint/model from `appsettings.json`).

Claude Desktop / VS Code `mcpServers` snippet (run from publish2 so it finds
`Data\ActivityData.db` and `appsettings.json`; or pass `--db`):

```json
{
  "mcpServers": {
    "activity-monitor": {
      "command": "C:\\ActivityMonitor\\publish2\\activity-monitor-mcp.exe",
      "args": ["--db", "C:\\ProgramData\\ActivityMonitor\\Data\\ActivityData.db"]
    }
  }
}
```

HTTP mode for clients that prefer it: start
`activity-monitor-mcp.exe --http 8765`, then point the client at
`http://127.0.0.1:8765/mcp` (loopback only, POST with
`Accept: application/json, text/event-stream`).

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
