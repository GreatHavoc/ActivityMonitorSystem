# Release Notes — Rust port (publish2/)

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
