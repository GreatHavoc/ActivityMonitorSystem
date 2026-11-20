# Release Notes — Rust port (publish2/)

## v1.5.0 (2026-11-20): reliability and housekeeping

- macOS AX hang guard. Every Accessibility read on macOS is a synchronous cross-process call, so a frozen app could block the service loop. Each element we read (frontmost app, window, focused element) now gets a one-second AX messaging timeout, and the focused value plus selected text are fetched in a single call instead of two. A unit test asserts that without the Accessibility grant, focus and text return None while idle events keep flowing, which is the same branch macOS 27.3 ask-every-time hits.
- jiff 1.0. All four crates moved from jiff 0.2 to 1.0.1. The only breakage was mechanical: `to_zoned` is now `to_zoned_datetime` (8 call sites) and `Timestamp - Timestamp` returns `SignedDuration`, so two `.get_seconds()` calls became `.as_secs()`. The "O" UTC database format and ICU report ordering are unchanged. The new 3-day publish cooldown held 1.0.1 back at first; it was pulled in with the documented override because 1.0.1 fixes an out-of-bounds timestamp parse.
- Supply-chain hygiene. `.cargo/config.toml` sets `global-min-publish-age = "3 days"`, so `cargo update` skips crate versions younger than three days (verified: a dry run held back clap 4.6.9, published hours earlier). All four crates inherit the `unused_dependencies` cargo lint and report none. No CI downloads needed pinning; the actions are already on fixed major tags.
- New export format. `activity-monitor-cli export --format sqlite --output backup.db` writes a consistent snapshot of the whole database via `VACUUM INTO`. `--from/--to` do not apply to it, and an existing output file is never overwritten.
- MCP server card. `server.json` now uses schema 2025-12-11 and a 60-character description, so it passes registry validation. Publishing to the MCP registry is declined: the server stays local-only (stdio, or HTTP on 127.0.0.1).
- macOS 27.3 onboarding text. The install script, the service log hint and the README describe the macOS 27.3 permission flow (approve by selecting the app from the list, re-approve when set to ask every time, transparency indicator while the AX API is in use) next to the classic Accessibility pane. Denied or ungranted access still degrades to idle-only events with a clear log line.
- Documented macOS AX limit: Electron and Chromium apps (VS Code, Slack, Chrome, Edge) expose no accessibility tree until an assistive client is enabled, so on macOS they record app and window title but empty text context. Use `ExcludedProcesses` to skip them.
- Model refresh rejected: Gemma 4 edge models need more memory than the target machines have and read screens worse than the shipped `qwen3-vl:2b`, so the default model is unchanged.
- Tests: workspace 75/75 green on Windows. CI green on windows-latest and macos-15.

## v1.4.0 (2026-11-19): macOS support

- macOS build. The workspace now compiles on macOS arm64. Sensors are split per platform: Windows keeps Win32; macOS gets idle time from CoreGraphics (`CGEventSourceSecondsSinceLastEventType`, no permission needed) and focus from the Accessibility API via the `axuielement` crate (frontmost app, window title, focused element text). DB, queue, ranking, Ollama client, CLI and MCP server were already portable and are unchanged.
- Consent-first capture on macOS. macOS cannot capture the screen silently: Screen Recording is a TCC permission and any active capture shows the mandatory menu-bar indicator. The macOS default backend is therefore `axtext`: frontmost app, window title and accessibility text go to the model, and no screenshot is ever taken or stored. The shipped `macos/appsettings.json` sets `CaptureBackend: "axtext"`; on macOS any other value logs a warning and uses axtext, on Windows an unknown value falls back to GDI.
- Text-only inference path. `analyze_text` in `core/src/ollama.rs` posts the accessibility context through the same `/api/chat` endpoint with the same structured-output schema (text-adapted prompt, no images), so events and inference rows flow exactly as on Windows. Works with both the `ollama` and `openai` API styles.
- Install and autostart. `install-macos.sh` copies the binaries to `~/Library/Application Support/ActivityMonitor` and registers a per-user launchd agent (`com.greathavoc.activitymonitor`, KeepAlive + RunAtLoad); `./install-macos.sh uninstall` removes it. On macOS the service resolves a relative `DatabasePath` against that folder (launchd starts agents with CWD=/), so the DB lives at `~/Library/Application Support/ActivityMonitor/Data/ActivityData.db`; the CLI and MCP resolve it through the same `default_data_dir()` helper.
- Ollama on macOS is probe-only. The service never downloads or starts anything on a Mac; when Ollama is not reachable it logs the manual fix (`brew install ollama`, `ollama serve`, `ollama pull <model>`) and keeps monitoring without inference. Grant the agent the Accessibility permission in System Settings > Privacy & Security > Accessibility, otherwise only idle events are recorded.
- CI. A `macos-15` job runs the full workspace tests plus the release build on arm64, ad-hoc signs the binaries (`codesign --force --sign -`; unsigned binaries fail the Screen Recording preflight on Sequoia+) and packages the `publish2-macos` artifact (3 binaries + `appsettings.json` + `install-macos.sh`) that the release page carries next to the Windows zip.
- Tests: workspace 74/74 green on Windows, including the new axtext request-body shape and a mock-server round trip for `analyze_text`. The macOS module adds path-parsing and clean-failure smoke tests that run on the macOS runner. The cfg(macos) code is compile-verified by CI; it cannot be cross-compiled locally because rusqlite `bundled` needs a C compiler for the target.
- Still later: the ScreenCaptureKit screen backend (needs a real Mac with Screen Recording granted; the recording indicator is mandatory) and Developer ID signing + notarization (needs the paid Apple Developer account). See ROADMAP.md.

## v1.3.0 (2026-11-18)

- Data ownership. A new `export` CLI command writes the raw records (events and AI summaries) as JSON or Markdown with a `--from`/`--to` range, so your history is a file you own and a GDPR-style export/erasure is one command. Compaction now purges AI summaries together with their events once they age past `MaxEventAgeDays`, and a new `ExcludedProcesses` list in `appsettings.json` stops recording for named apps entirely: no focus event, no capture while they are in the foreground.
- Weekly digest. `report --week` on the CLI and the new `weekly_digest` MCP tool (the seventh tool) aggregate the last 7 days into a Markdown digest: event counts, context switches, top apps, AI category distribution, top topics, busiest hours. Both surfaces use the same aggregation in `core/src/digest.rs`.
- Trust and distribution. The repo ships `server.json` (MCP server card) and every release carries a `SHA256SUMS.txt` asset, so agents can verify the binary before connecting. GitHub attestation is the follow-up; the local `gh` CLI has no `attest` command yet. The MCP server stays read-only and binds HTTP to 127.0.0.1 only.
- The README now maps the privacy posture to the rules that apply: local-only data, invisible capture on Windows, export and purge controls, and a workplace-use note covering the EU AI Act (high-risk duties enforceable since 2026-08-02), California AB 1331 and GDPR.
- Tests: workspace 72/72 green, including exclusion matching, compaction purging inference rows, digest rendering and the 7-tool MCP list. CI green on `rust-release`.

## v1.2.0 (2026-11-18)

- `search_activity`, the MCP server's sixth tool, now runs hybrid search. One leg is FTS5 bm25 over the AI-written app, topic, summary and visible text, kept in sync by triggers inside the same SQLite file, with a LIKE fallback on databases the .NET app created. The other is trigram fuzzy matching, which tolerates typos and paraphrases. Results are RRF-merged and each hit is tagged `matchType: keyword|fuzzy|both`. We looked at embeddings and decided against an ML runtime: fastembed/ONNX Runtime does not link on current MSVC toolchains (LNK2019 on the CRT imports), sqlite-vec is still pre-1.0, and Ollama embeddings would compete with the vision model for the GPU. No new dependencies, nothing to download, works on any machine. If FTS is missing, search degrades to LIKE plus trigram.
- DXGI Desktop Duplication is available as a capture backend via `CaptureBackend: "dxgi"`. The default stays `"gdi"`. It captures silently on the GPU and was verified live with real 1920×1080 content. Two findings are built in: the D3D device needs `D3D11_CREATE_DEVICE_BGRA_SUPPORT` (some adapters, including VMs, otherwise produce blank frames), and a uniform-frame guard falls back to GDI so a blank frame never reaches inference. Dirty-rectangle skipping is not in yet, so each capture is still one full frame.
- The inference runtime is no longer Ollama-only. `OllamaApiStyle: "openai"` speaks `/v1/chat/completions` (images as `data:image/jpeg;base64` URLs, top-level `max_tokens: 8192`) and checks health via `GET /v1/models`. That covers Foundry Local (`http://127.0.0.1:5273/v1`; aliases such as `phi-4-multimodal-instruct` run on the NPU on Copilot+ PCs), LM Studio, and Windows ML's WinMLServer. With this style the first-run bootstrap installs nothing: it probes and logs the manual fix (`foundry service start`, `foundry model run <alias>`). The default stays `"ollama"`, and `ask_about_my_day` works against either runtime.
- The README now documents the privacy and consent posture: data stays local, capture is invisible (GDI BitBlt, so no consent picker, no border, no notification), and the retention and purge controls are described.
- Workspace tests are 69/69 green, including mock-server round trips for both API styles and both search legs. CI is green on `rust-release`.

## v1.1.0 (2026-11-17)

- **MCP server** (`activity-monitor-mcp.exe`, new): spec revision 2026-07-28
  with dual-era handshake (legacy 2025-11-25 / 2025-06-18 still accepted).
  stdio by default, Streamable HTTP on 127.0.0.1 with `--http <port>`.
  Five read-only tools over the same `ActivityData.db`:
  `todays_timeline`, `session_summary`, `activity_at`, `daily_report`,
  `ask_about_my_day` (the last one chats with the configured Ollama model).
- **Configurable capture intervals**: `PeriodicCaptureIntervalSeconds`
  (default 180) and `MinEventCaptureIntervalSeconds` (default 10) in
  `appsettings.json`; defaults equal the old hardcoded C# values, so legacy
  configs behave identically.
- **Multi-monitor capture**: `CaptureAllMonitors: true` (set in the shipped
  config) captures the whole virtual screen (all displays) in one GDI frame.
  GDI `BitBlt` remains the only capture path — deliberately invisible to the
  user (no consent picker, no capture border); WGC was rejected on stealth
  grounds.
- **Culture-exact report ordering**: CLI reports sort with ICU at the Windows
  user-default locale, matching .NET `OrderBy`/`Comparer<string>.Default`.
- **Ollama `num_predict` 1024 → 8192** (both chat paths): the `qwen3-vl`
  thinking renderer ignores `think:false` on Ollama 0.40.x and burned the
  whole 1024-token budget on `<think>`, returning empty answers
  (`done_reason: length`). Found by the clean-machine drill against real
  Ollama 0.40.16; 8192 is reliable, verified live.
- **`install-service.ps1`** (new): `sc.exe create/config/start` for the
  Windows service plus `-Uninstall`. Run it from an elevated PowerShell.
- **P4 clean-machine drill verified end-to-end**: fresh folder + real Ollama
  → capture → structured inference in `InferenceResults` → CLI and MCP
  reports. Recommendation for slow/cold machines: set
  `ProcessingTimeoutSeconds: 600` (the 120 s default can cut off the first
  inference while the model loads).
- CI (`.github/workflows/ci.yml`): `cargo test`, release build, MCP stdio
  smoke, CLI smoke on every push/PR to `rust-release` (Rust-only; the .NET
  golden diff stays manual — no dotnet SDK on the runner).

## Day-zero bootstrap (added post-release, 2026-11-16)

The service now self-provisions on a new system: it creates
`Data\ActivityData.db` if missing, checks Ollama via `/api/tags`, starts
`ollama serve` when the server is down, runs `ollama pull <model>` when the
model is missing, and downloads the official `ollama-windows-amd64.zip`
into `%LOCALAPPDATA%\Programs\Ollama` when no `ollama.exe` exists. Startup
blocks until the model is ready (first run downloads ~2 GB); any failure is
logged with the manual fix and monitoring continues without inference.
Override the binary location with `ACTIVITY_MONITOR_OLLAMA_EXE`.

Parity-first port of the .NET Activity Monitor to Rust. The SQLite database is
byte-compatible (same DDL, `"O"`-format UTC TEXT timestamps), so the new
service and CLI read and write the same `ActivityData.db` as the .NET build.

## Fixed C# defects (fixed during the port)

1. **Timezone comparison bug** — idle/last-input comparisons mixed `DateTime`
   kinds; the port compares UTC instants only (jiff `Timestamp`).
2. **Dead compaction + typo'd config key** — `CompactOldEventsAsync` was never
   called and the shipped `appsettings.json` had `"MaxEventAgeD ays"`, so the
   setting silently never applied. The Rust service runs compaction at startup
   and every `CompactionIntervalHours`, and the key is `MaxEventAgeDays`
   (the typo'd key is still accepted as an alias for old configs).
3. **Infinite HTTP timeout** — `ProcessingTimeoutSeconds: 0` left the Ollama
   HTTP call with no timeout; the port treats `<= 0` as the 120 s default.
4. **GetTickCount 49.7-day wrap** — 32-bit tick math could wrap; the port uses
   monotonic `Instant`.
5. **GDI-vs-WGC doc mismatch** — README described Windows Graphics Capture;
   the code uses GDI `BitBlt`. Docs now match the code (GDI, JPEG q75).
6. **Stale README/SETUP** — claimed .NET 9 SDK / WGC / wrong paths; rewritten
   for the Rust build (no .NET runtime needed at runtime).
7. **Queue capacity race** — increment-then-compare could transiently exceed
   `MaxQueueSize`; the port rejects atomically (CAS).
8. **Hardcoded log path** — logs went to `C:\ActivityMonitor\publish\Logs`;
   now `<exe_dir>\Logs`.
9. **Hardcoded DB path in shipped config** — `C:\ActivityMonitor\publish\Data\ActivityData.db`;
   `publish2/appsettings.json` uses relative `Data\ActivityData.db`, resolved
   against the working directory (same as the CLI).
10. **Per-operation connection churn** — the Ollama client rebuilt its HTTP
    connection per request; the port reuses one client.

## Known differences (accepted)

- JPEG bytes differ from System.Drawing (different encoder); capture gate is
  size band + decoded-pixel check, never byte equality.
- Rust JSON output has no UTF-8 BOM (the .NET CLI wrote one).
- 3-minute periodic capture and the 10 s event-capture throttle were
  hardcoded in v1.0.0; v1.1.0 makes them configurable (defaults unchanged).
- `DistinctTopics`/`DistinctApplications` ordering was ordinal in v1.0.0;
  v1.1.0 sorts with ICU at the user-default locale, matching C#.
- The Rust service loads `appsettings.json` from the exe directory; the .NET
  host loads it from the working directory. Identical in the standard
  deployment (service runs from its publish folder); the soak had to place a
  copy of the config in the CWD for the .NET side.
