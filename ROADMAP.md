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

## Known limits (v1.1.0)

- Windows-service mode: `install-service.ps1` (elevated) ships with the
  release; the actual `sc.exe create` run still needs one elevated test on a
  real machine (this dev box has no admin token) — use the Startup-shortcut
  mode until then
- First inference on a cold model can exceed the 120 s default HTTP timeout;
  set `ProcessingTimeoutSeconds: 600` on slow machines
- Report topic/app lists sort culture-aware (ICU collator at the Windows
  user locale) exactly like the .NET `OrderBy` (fixed 2026-11-17)
- Capture intervals are configurable (defaults = old hardcoded 3 min / 10 s)
  and multi-monitor capture is on via `CaptureAllMonitors: true`

---

## Now / Next / Later

### Now — shipped (v1.0.0 on 2026-11-16, v1.1.0 on 2026-11-17)
- v1.1.0 release page on GitHub with the `publish2/` zip (service + CLI +
  MCP + `appsettings.json` + `install-service.ps1`) — see `RELEASE_NOTES.md`
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
- Verified end-to-end on a clean install (2026-11-17): fresh folder + real
  Ollama 0.40.16 + `qwen3-vl:2b` — capture → inference → DB → CLI → MCP all
  work; `install-service.ps1` covers the `sc.exe` install (needs an elevated
  shell). Drill also caught and fixed a real bug: `num_predict` 1024 → 8192
  (the model's thinking budget otherwise starves the answer)

### Next — the following 1–3 months
- **Every code change tested automatically.** CI (`.github/workflows/ci.yml`,
  windows runner, every push/PR to `rust-release`) runs `cargo test
  --workspace`, the release build, and MCP/CLI smoke tests (green since
  2026-11-17, run 43955153804). The golden diff against the .NET CLI stays a
  manual gate until the runner image carries the dotnet SDK.
### Later — when there is evidence it's worth it
- ~~**A capture method that matches the old README's promise.**~~ **Rejected
  2026-11-17 (stealth requirement):** WGC always shows the OS consent picker on
  first use and draws a capture border around the captured item — user-visible
  by design and not suppressible in a normal process. GDI `BitBlt` is silent:
  no border, no notification, nothing on screen. Decision: GDI (now
  multi-monitor via `CaptureAllMonitors`) is the permanent capture path; WGC
  is off the roadmap.
- **See your day, don't just query it.** A local read-only web page over the
  database, replacing the CLI for browsing.
- **Keep the frames, keep as much history as you want.** Optional frame
  retention and a configurable retention age.
- **Updates that find you.** Publish `publish2/` as GitHub Release assets
  with a proper product release page, and add an auto-update channel.

---

## v1.2 roadmap — market-informed (drafted 2026-11-17)

Evidence base: MCP is the settled agent-to-tool standard (47k+ servers,
~500M SDK downloads/month; we already implement spec 2026-07-28 and dual-era
handshake — no spec work due yet); Hubstaff shipped a workforce-data MCP
server (Sept 2026), so agent-readable activity data is a real product
category; a May 2026 Northeastern/Columbia audit found 9/9 monitoring
platforms share identifying worker data with third parties; California AB
1331 (Sept 2026) and the EU AI Act tighten workplace surveillance toward
consent + data minimization; Foundry Local GA + Windows ML GGUF support make
NPU local inference a Windows path; research (Coco, MobileMem, MemArena,
"Time and Space") converges on continuous desktop observation → local
personalization → proactive agents, with SQLite + local embeddings as the
substrate.

### Now — in progress
- **Privacy/consent posture as the product story** — README "Privacy &
  consent" section shipped (local-only data, capture invisibility explained,
  purge/retention controls documented)
- **MCP as the product surface** — `search_activity` (keyword over past days,
  with `from`/`to`/`limit`) shipped in v1.2-dev; semantic search (sqlite-vec)
  and MCP Server Card (`.well-known`) metadata to follow
- **Service install on real machines** — elevated `install-service.ps1` run
  and a documented Scheduled-Task fallback for non-admin users

### Next — the following 1–3 months
- **DXGI Desktop Duplication capture backend (optional)** — GPU path, also
  silent (no picker/border; only WGC shows those); dirty rectangles mean we
  only re-infer when the screen changed. GDI stays the permanent fallback.
  Known limits: one duplicating process per output (max 4 concurrent),
  interfaces must be recreated on display/desktop changes.
- **Semantic search in the same SQLite file** — sqlite-vec + FTS5 hybrid
  retrieval with local embeddings; ship as a *degradable* derived index
  (keyword search keeps working if the extension fails). Powers MCP
  `search_activity` v2.
- **Runtime flexibility** — swap the Ollama endpoint for Foundry Local /
  Windows ML (OpenAI-compatible) so Copilot+ PCs get NPU inference.

### Later — when there is evidence it's worth it
- **From recording to advising** — weekly "shape of your work" digest: focus
  blocks, context switching, share of AI-tool usage (the AI-governance
  segment is the market's fastest-growing budget line). Read-only insights,
  no intervention engine (research-grade personalization is the version after
  that).
- **Data ownership** — one-command full export, purge-by-age, per-app
  exclusion rules.

**Explicitly not doing** (market said no): WGC capture (stealth, settled),
cloud sync/telemetry (the entire wedge), keystroke logging (legally
radioactive).

---

## Note on the GitHub release pages

Releases exist on `rust-release`: v1.0.0 and v1.1.0 (2026-11-17). Each page
carries only the `publish2/` zip (service + CLI + MCP + `appsettings.json`
+ `install-service.ps1`) — never the legacy .NET `publish/`. Release body =
`RELEASE_NOTES.md`. `main` stays the .NET original.
