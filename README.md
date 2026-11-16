# Activity Monitor — Windows Application (Rust port)

Lightweight Windows app that monitors user activity through window focus
tracking, idle detection, and triggered screen capture with AI-powered
activity analysis via a local Ollama vision model (Qwen3-VL).

This is the Rust port of the original .NET app. The SQLite database is
byte-compatible with the .NET build, so both read and write the same
`ActivityData.db`. See `RELEASE_NOTES.md` for the list of C# defects fixed
during the port. The original .NET build is untouched in `publish/`; the Rust
build deploys to `publish2/`.

## Features

- **Activity Tracking**: foreground window + idle detection via Win32 APIs
- **Smart Screen Capture**: GDI `BitBlt` of the primary screen, JPEG q75,
  low-FPS burst on triggers
- **AI Analysis**: Qwen3-VL via Ollama `/api/chat` with structured output
- **Queue System**: priority-based processing with backpressure
- **Event Storage**: SQLite timeline (`ActivityEvents` + `InferenceResults`)
- **CLI Tools**: timeline, detailed, summary, stats, report (JSON / LLM)

## Requirements

- **Windows 10/11 (64-bit)** — the release executables are statically linked
  (no .NET runtime or VC++ redistributable needed)
- **Ollama** with the `qwen3-vl:2b` model pulled (`ollama pull qwen3-vl:2b`),
  listening on `http://localhost:11434`
- **Rust toolchain** only if building from source

## Installation (from source)

```cmd
cargo build --release --workspace
powershell -File deploy.ps1       REM copies exes + appsettings.json to publish2\
```

Or just use the prebuilt `publish2\` folder.

## Auto-start

1. `Win + R`, type `shell:startup`
2. Create a shortcut to `C:\ActivityMonitor\publish2\activity-monitor-service.exe`
3. Name it "Activity Monitor"

To run as a real Windows service instead: `activity-monitor-service.exe --service`
(installed via `sc create`), or run it in a console with Ctrl+C to stop.

## Configuration

Edit `publish2\appsettings.json` (loaded from the exe's folder):

```json
{
  "ActivityMonitor": {
    "SamplingIntervalSeconds": 5,
    "IdleThresholdSeconds": 300,
    "CaptureSettings": {
      "FrameRate": 1, "MaxDurationSeconds": 30,
      "TriggerOnFocusChange": true, "TriggerOnIdleResume": true,
      "MaxFramesPerCapture": 1
    },
    "OllamaEndpoint": "http://localhost:11434",
    "OllamaModel": "qwen3-vl:2b",
    "QueueSettings": {
      "MaxConcurrentTasks": 4, "MaxQueueSize": 100,
      "HighPriorityThreshold": 10, "ProcessingTimeoutSeconds": 0
    },
    "Storage": {
      "DatabasePath": "Data\\ActivityData.db",
      "RetainFrames": false, "CompactionIntervalHours": 24, "MaxEventAgeDays": 90
    }
  }
}
```

`DatabasePath` is relative to the working directory, so run the service from
the `publish2` folder (the CLI resolves `.\Data\ActivityData.db` first).
`ProcessingTimeoutSeconds: 0` means the 120 s default (the C# code left the
HTTP timeout infinite). Logs go to `<exe_dir>\Logs\`.

## CLI usage

```cmd
cd C:\ActivityMonitor\publish2
activity-monitor-cli.exe timeline --from "2026-11-15" --to "2026-11-16"
activity-monitor-cli.exe detailed --from "2026-11-15" --to "2026-11-16" --limit 50
activity-monitor-cli.exe summary --date "2026-11-15"
activity-monitor-cli.exe stats
activity-monitor-cli.exe report --from "2026-11-15" --to "2026-11-16" --output report.json
activity-monitor-cli.exe report --from "2026-11-15" --to "2026-11-16" --llm
activity-monitor-cli.exe query --from "2026-11-15" --to "2026-11-16" --limit 100
```

Report JSON schemas (v2.0 standard, v3.0-llm) are unchanged from the .NET
CLI; output is byte-identical except no UTF-8 BOM and `GeneratedAtUtc`.

## Development

```
Cargo.toml                  # workspace
crates/activity-monitor-core    # config, db, capture, sensors, ollama, queue
crates/activity-monitor-service # host loop (console / --service)
crates/activity-monitor-cli     # timeline/detailed/summary/stats/report/query
publish2/                       # Rust deploy target (publish/ = old .NET build)
```

```cmd
cargo test --workspace
cargo build --release --workspace
```

## Troubleshooting

- **"Database not found!"** from the CLI — run it from the folder containing
  `Data\ActivityData.db`, or make sure the service is running first.
- Service won't start — check `<exe_dir>\Logs\activity-monitor.YYYY-MM-DD`.
- No inference rows — `curl http://localhost:11434/api/tags` must list
  `qwen3-vl:2b`; connection errors fail fast (120 s timeout) and are logged.
- Corrupt DB — stop the service, delete `Data\ActivityData.db`, restart.

## License

MIT
