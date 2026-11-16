# Activity Monitor — Product Roadmap

**What this document is:** the public roadmap for the Rust release of Activity
Monitor (branch `rust-release`). It says what the product does today, why it
exists, and what is planned next — in plain terms first, technical detail
second. Plans are grouped by horizon (Now / Next / Later), not by dates;
nothing here is a commitment until it ships.

---

## Why it exists — the problem it solves (~10%)

Your computer knows what you worked on; you don't. At review time, at a
standup, or when someone asks "what did you actually do last Tuesday?" the
answer is usually a guess.

Activity Monitor quietly records your screen activity into a private, local
timeline of your day — which apps and windows you used, when you were idle,
and an AI-written label for what each session was about. Everything stays on
your machine in one small database file. No cloud, no account, no screenshots
leaving the device.

## What you get today (~60%, business view)

Shipped in `rust-release` (`publish2/` folder), verified end-to-end against
the original .NET version.

- **Runs anywhere, sets itself up.** Copy the `publish2` folder to any Windows
  10/11 PC and run the service. First run creates its own database and, if
  needed, downloads and starts the AI engine and its model automatically. No
  installers, no runtimes to pre-install, no admin rights needed.
- **Your day, written down.** Every few seconds it notes the active app and
  window and whether you were idle. It takes a short screen capture when you
  switch apps, come back from idle, and every 3 minutes — and the AI turns
  each capture into a plain-language label ("coding in VS Code on the billing
  service") with a confidence score.
- **Ask questions, get answers.** The CLI turns the database into timelines,
  daily summaries, statistics, and two JSON reports — a detailed one for
  tools, a compact narrative one for pasting into any LLM. Report formats
  are identical to the old .NET version, so existing scripts keep working.
- **Your agent can see your day.** `activity-monitor-mcp.exe` (shipped in
  `publish2/`) speaks MCP spec 2026-07-28 over stdio (plus Streamable HTTP
  on `127.0.0.1` via `--http`), exposing five read-only tools over the same
  database — today's timeline, per-app summaries, "what was on screen at
  2 pm?", a JSON daily report, and ask-the-model questions — so any
  MCP-capable agent (Claude Desktop, VS Code, your own scripts) connects
  with one config line (see SETUP.md).
- **Safe to try, easy to undo.** The new build reads and writes the exact
  same database as the old one and keeps its own logs; switching back is one
  shortcut change, your history is never at risk. History older than 90 days
  is cleaned up automatically.

## Under the hood (~30%, technical view)

- Rust workspace (`core` / `service` / `cli` / `mcp`); release exes in
  `publish2/` are static-CRT — zero runtime dependencies on the target machine
- SQLite (`ActivityEvents`, `InferenceResults`), schema byte-compatible with
  the .NET version, `"O"`-format UTC timestamps
- Win32 sensors (foreground window, idle time); GDI screen capture → JPEG q75
- Ollama `/api/chat` with structured JSON output and a 120 s timeout;
  first-run bootstrap auto-installs/starts Ollama and pulls `qwen3-vl:2b`
  (`ACTIVITY_MONITOR_OLLAMA_EXE` overrides the binary path)
- Priority queue (Critical/High/Normal/Low, depth 100, 4 workers,
  drop-when-full), graceful drain on stop
- 47 tests green; CLI output golden-diffed MATCH against the .NET CLI;
  10 defects from the C# version fixed during the port (`RELEASE_NOTES.md`)
- MCP server: hand-rolled JSON-RPC core (no SDK), dual-era 2026-07-28 /
  2025-11-25 / 2025-06-18, `server/discover`, read-only SQLite
  (`SQLITE_OPEN_READ_ONLY`) so it never blocks the service; protocol unit
  tests + stdio/HTTP smoke tests against a seeded DB
- Runs as a console app via the Startup folder, or as a Windows service
  (`--service` + `sc create`)

## Known limits of this release

- Windows-service (`--service`) mode is built but not yet drill-tested under
  `sc.exe` — use the Startup-shortcut mode for now
- AI inference verified against a local stand-in; watch the first inference
  row on a real machine once
- Report topic/app lists now sort culture-aware (ICU collator at the Windows
  user locale) exactly like the .NET `OrderBy` — the former tie-order
  cosmetic difference is fixed (2026-11-17)
- Capture intervals (3 min periodic / 10 s event throttle) are fixed, and
  capture covers the primary screen only

---

## Now / Next / Later

### Now — shipped in this release
- Day-zero self-provisioning (database + AI engine + model)
- Local timeline of app/window activity with AI session labels
- CLI reports in .NET-compatible formats
- MCP server next to the service (read-only agent access to your day)
- One-folder deploy, rollback by switching one shortcut
- Configurable capture intervals (`PeriodicCaptureIntervalSeconds` /
  `MinEventCaptureIntervalSeconds` in `appsettings.json`, defaults 180/10 =
  the old hardcoded values) and byte-exact report ordering (ICU sort)
- Multi-monitor capture: `CaptureAllMonitors: true` grabs the whole virtual
  screen (all displays) in one frame instead of just the primary

### Next — the following 1–3 months
- **Every code change tested automatically.** CI (`.github/workflows/ci.yml`,
  windows runner, every push/PR to `rust-release`) runs `cargo test
  --workspace`, the release build, and MCP/CLI smoke tests (green since
  2026-11-17, run 43955153804). The golden diff against the .NET CLI stays a
  manual gate until the runner image carries the dotnet SDK.
- **Verified on a clean machine.** Real-Ollama end-to-end run plus a
  Windows-service install drill on a fresh VM.

### Later — when there is evidence it's worth it
- **A capture method that matches the old README's promise.** Optional Windows
  Graphics Capture mode behind a config flag (the original docs described WGC;
  the C# code actually used GDI — see RELEASE_NOTES). Vetted backend:
  `windows-capture` crate; BitBlt stays the default path. Deferred 2026-11-17:
  multi-monitor GDI covers the actual gap; WGC needs a desktop session to
  verify consent/border behavior properly.
- **See your day, don't just query it.** A local read-only web page over the
  database, replacing the CLI for browsing.
- **Keep the frames, keep as much history as you want.** Optional frame
  retention and a configurable retention age.
- **Updates that find you.** Publish `publish2/` as GitHub Release assets
  with a proper product release page, and add an auto-update channel.

---

## Note on the GitHub release page

The repo currently has no GitHub releases or tags, so there is no old C#
artifact on any release page to remove. When creating the release: tag
`rust-release`, attach only the `publish2/` folder (service + CLI +
`appsettings.json`) — never the legacy .NET `publish/` — and use
`RELEASE_NOTES.md` plus this roadmap as the release body. `main` stays the
.NET original.
