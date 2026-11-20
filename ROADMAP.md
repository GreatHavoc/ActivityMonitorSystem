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

## v1.2 roadmap — market-informed (drafted 2026-11-17, **released as v1.2.0 on 2026-11-18**)

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
- **MCP as the product surface** — `search_activity` (hybrid search over past
  days, with `from`/`to`/`limit`) shipped in v1.2-dev. Design decision after
  research: **no ML runtime** — ranking is FTS5 bm25 (inside the bundled
  SQLite) + a trigram fuzzy leg, RRF-merged. Rejected: fastembed/ORT (cannot
  link on current MSVC toolchains; load-dynamic needs an externally-sourced
  `onnxruntime.dll`), sqlite-vec (pre-v-v1 extension), Ollama embeddings
  (GPU is the vision model's). Zero deps, zero download, works on every
  machine. MCP Server Card (`.well-known`) metadata to follow
- **Service install on real machines** — elevated `install-service.ps1` run
  and a documented Scheduled-Task fallback for non-admin users
- **DXGI Desktop Duplication capture backend** — shipped, opt-in via
  `CaptureBackend: "dxgi"` (default `"gdi"`). Verified live on the dev box:
  real desktop content, 1920×1080 whole-screen JPEG. Two real findings baked
  in: the D3D device needs `D3D11_CREATE_DEVICE_BGRA_SUPPORT` (some drivers
  — including VM adapters — otherwise deliver blank frames), and a
  uniform-frame guard falls back to GDI so blank frames never reach
  inference. Known limits: dirty-rectangle skipping not implemented yet
  (still one full frame per capture); one duplicating process per output;
  interfaces are recreated per capture, so a stale-interface case
  degrades to the GDI fallback instead of failing.

### Next — the following 1–3 months
- **Runtime flexibility** — shipped: `OllamaApiStyle: "openai"` speaks
  OpenAI-compatible `/v1/chat/completions` (data-URL images, top-level
  `max_tokens`), covering Foundry Local (`http://127.0.0.1:5273/v1`, vision
  aliases like `phi-4-multimodal-instruct` get NPU inference on Copilot+ PCs),
  LM Studio and Windows ML's WinMLServer. Bootstrap skips install/serve/pull
  for this style and only probes + logs the manual fix. Health check uses
  `GET /v1/models`. Default stays `"ollama"` — zero change for existing installs.

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

## v1.3 roadmap — market scan + usage audit (drafted 2026-11-18, **released as v1.3.0 on 2026-11-18**)

Evidence base: the MCP registry passed 47,000 servers on 2026-10-21 (about 40x
the end of 2025) with 97M+ monthly SDK downloads, but 2026 security reports
found exploitable flaws in 30–82% of public MCP servers and 1,184+ confirmed
malicious entries, so provenance and verification now decide whether agents
connect at all. EU AI Act high-risk obligations have been enforceable since
2026-08-02 and Annex III covers worker-management systems, so privacy controls
are compliance, not marketing. Windows Recall is still Copilot+ only and still
draws security criticism, which leaves room for a screen-memory tool that runs
on any PC and stores summaries instead of screenshots. Local-first PKM tools
keep winning on "your data stays in plain files". Usage audit of our own
release: one download per release, no telemetry by design, and the local soak
shows why search matters: 335 raw events in 30 minutes but only 5 inference
summaries, and databases from older versions have no FTS table, so the
LIKE+fuzzy fallback is the path most real installs take until the service
re-indexes. Full notes: brain inbox note `market-research-activity-monitor-v13`.

### Now — shipped in v1.3.0
- Data ownership bundle: `cli export` to Markdown and JSON with a date range,
  purge of inference rows when their events age out, and an `ExcludedProcesses`
  list in `appsettings.json`. Grounds: GDPR portability/erasure, AI Act data
  minimization, and the local-first expectation that your history is a file
  you own.
- Weekly "shape of your work" digest: a `weekly_digest` MCP tool plus
  `cli report --week`, written as a Markdown file. Promoted from Later because
  the data already exists and it is the most visible proof of value.
- Trust and distribution: `server.json` (MCP server card) shipped in the repo,
  SHA256SUMS on every release, and a provenance note in the README's MCP
  section. Remaining follow-ups: GitHub attestations (the local `gh` CLI has
  no `attest` command yet) and registry submission (needs an OCI package
  push). Grounds: 47k servers and a live supply-chain crisis; agents verify a
  server before connecting.
- Compliance note in the README: personal-use scope, what changes if the tool
  is used at work (AI Act Annex III, consent), and how export/delete map to
  GDPR rights.

### Next — the following 1–3 months
- NPU text path: send text-only prompts (digest, search explanations) to a
  small on-device model (Phi Silica / Foundry Local) and keep the vision model
  for screenshots only, so the GPU stays free for capture.
- DXGI improvements: dirty-rectangle skipping, one duplicator shared across
  outputs.
- Search quality gate: a small golden query set in CI so ranking regressions
  fail the build instead of reaching users.

### Later — when there is evidence it's worth it
- Proactive assistant that reads the digest and proposes actions.
- Phone/wearable context; team or enterprise consent flows.
- Still rejected: cloud sync, telemetry, keystroke logging, WGC capture,
  storing raw screenshots.

---

## macOS expansion plan (drafted 2026-11-18; M0 + M1 + launchd installer + macOS CI shipped as v1.4.0 on 2026-11-19, M2 and M3 stay Later)

Grounding research (2026-11-18), fact-checked against Apple docs, crate
docs and 2026 bug reports: macOS cannot capture the screen silently.
Screen Recording is a TCC permission and any active capture shows a
menu-bar recording indicator, so the Windows stealth posture cannot be
replicated there; the privacy story on macOS has to be consent-first. Two
sharper constraints from the fact-check: the indicator for a third-party
ScreenCaptureKit stream is mandatory and unsuppressible (the only exemption
is external displays), and on macOS 15+ using ScreenCaptureKit without the
system picker triggers periodic TCC re-authorization prompts, while the
picker itself needs a user click on every launch, so a background service
cannot use it. Also, unsigned binaries get `CGPreflightScreenCaptureAccess
== false` on Sequoia+, so release binaries must be at least ad-hoc signed
(`codesign --force --sign -`), and only a stable Developer ID signature
keeps TCC grants across releases because ad-hoc signatures hash per build.
The port itself is cheap: only `capture.rs`, `dxgi.rs` and `sensors.rs` in core
are `cfg(windows)`, plus one locale call in the CLI, so db/rank/ollama/queue
and the whole MCP server are already portable Rust. The bindings macOS needs
exist: `screencapturekit` (ScreenCaptureKit, macOS 12.3+), `axuielement` for
the Accessibility API, IOKit for idle detection. Ollama now runs on MLX on
Apple Silicon (since March 2026), so the default inference path works
unchanged. Distribution outside the App Store requires a Developer ID
signature plus notarization (paid Apple Developer account); that is the one
hard external dependency.

Product decision: on macOS the default capture backend is text-only
(frontmost app, window title and accessibility text sent to the model, no
screenshots), which keeps the "no raw screen images stored" wedge. Full
screen capture via ScreenCaptureKit stays opt-in for users who grant Screen
Recording and accept the visible indicator.

### Phase M0 — scaffold (Done in v1.4.0)
- Implemented as a cfg-split rather than a platform trait (less code): `sensors.rs`
  is cross-platform (Windows Win32 kept, macOS in `core/src/mac.rs`), `queue.rs`
  is portable with a per-OS capture step, and `lib.rs` gates `capture`/`dxgi` to
  Windows. Zero behavior change on Windows (74/74 workspace tests green).
- DB path resolution via `default_data_dir()`: macOS resolves a relative
  `DatabasePath` against `~/Library/Application Support/ActivityMonitor`
  (launchd starts agents with CWD=/), so the DB is
  `~/Library/Application Support/ActivityMonitor/Data/ActivityData.db`.
  Config still loads from next to the executable.
- CI: `macos-15` (arm64) job runs the workspace tests + release build, ad-hoc
  signs the binaries, and packages the `publish2-macos` release artifact.
  It is the compile source of truth for cfg(macos) code (cannot be
  cross-compiled locally; rusqlite bundled needs a target cc).

### Phase M1 — macOS sensors + text backend (Done in v1.4.0)
- Idle via CoreGraphics `CGEventSourceSecondsSinceLastEventType` (no
  permission needed; the plan's IOKit route was unnecessary).
- Focus via the `axuielement` crate's system-wide element: focused
  application (pid + name via `proc_pidpath`) plus the AX window title.
  Needs the Accessibility permission; the service log and install script
  say exactly where to grant it (no System Settings pane is auto-opened).
- `axtext` backend: `core/src/mac.rs::focus_text()` gathers app/title/focused
  element text, and `ollama.rs::analyze_text` posts it through the same
  `/api/chat` structured path (text-adapted prompt, no images), so events
  and inference rows flow end to end without any screen image. Works with
  both `ollama` and `openai` API styles.

### Phase M2 — screen backend (Later)
- ScreenCaptureKit backend, opt-in via `CaptureBackend: "screen"`, with a
  permission preflight (`CGPreflightScreenCaptureAccess`) and the mandatory
  recording indicator plus the macOS 15+ periodic re-prompt documented up
  front. `axtext` stays the default.
- Done early from this phase: launchd LaunchAgent plus `install-macos.sh`
  (replaces `install-service.ps1` on macOS). The Ollama bootstrap on macOS
  is probe-only, not identical to Windows: the service never downloads or
  starts anything on a Mac, it logs `brew install ollama` / `ollama serve` /
  `ollama pull <model>` and keeps monitoring without inference.

### Phase M3 — distribution (Later; gated on the Apple Developer account)
- Done early from this phase: every macOS release binary is ad-hoc signed in
  CI (`codesign --force --sign -`; unsigned binaries cannot pass the Screen
  Recording preflight on Sequoia+), and the release page carries the macOS zip
  (`publish2-macos-v1.4.0.zip`) next to the Windows `publish2` zip plus
  SHA256SUMS. README macOS section shipped.
- Remaining: Developer ID signing + notarization once the paid Apple
  Developer account exists (ad-hoc signatures hash per build, so TCC grants
  do not carry across releases). Homebrew tap only after notarization works.

---

## v1.5 candidates (research scan 2026-11-20)

Sources read: Ollama release notes 0.40.0 to 0.40.17, the Gemma 4 model card and
Ollama library page, rusqlite releases, the jiff changelog, MCP spec and registry
docs, Windows AI APIs docs, and macOS AX implementation notes. Each item names
the evidence and the files it would touch.

### Now, small and low risk

1. macOS AX hang guard. A frozen target app blocks our service loop because
   every AX read is a synchronous cross-process call. axuielement 0.10.2
   provides `AXUIElement::set_timeout(f32)`; set about one second on the focused
   app before reading title or value, and fetch both in one
   `copy_multiple_attribute_values` call. File:
   `crates/activity-monitor-core/src/mac.rs`. The macos-15 runner has no
   Accessibility grant, so CI covers compile plus the clean-failure path.
2. jiff 0.2.43 to 0.2.46. Same API. It fixes an unsound internal API (#707) and
   ships tzdb 2026f. Do not move to 1.0 yet: `Zoned` became `ZonedDateTime`,
   `to_zoned` became `to_zoned_datetime`, and `Timestamp - Timestamp` now
   returns `SignedDuration`; six call sites change for no functional gain.
3. server.json hygiene. Registry examples use schema `2025-12-11` (ours says
   `2025-10-17`) and limit `description` to 100 characters (ours is about 250,
   so a publish would fail validation). This is metadata hygiene only. The
   registry also needs an npm or PyPI package or a public remote URL; this
   server is local-only, so submission stays out of scope.
4. Document macOS AX limits. Electron and Chromium apps (VS Code, Slack,
   Chrome) expose no accessibility tree until an assistive client is detected
   and enabled, so axtext returns app and title with empty text. Add this to the
   README and troubleshooting, and point to `ExcludedProcesses` or the future
   screen backend.

### Next, evaluate then decide

5. Gemma 4 edge models as an optional vision model. `gemma4:e2b` (2.3B
   effective, 5.1B total, 4.6 to 7.5 GB, 128K context, text, image and audio,
   with an MLX variant) and `gemma4:e4b` are Apache 2.0 and built for on-device
   use. Ollama runs MLX by default on Apple Silicon since 0.40. Build a fixed
   20-frame fixture (editor, browser, terminal, docs, chat, video), compare
   label, summary and `visible_text` accuracy plus latency and RAM against
   `qwen3-vl:2b`, then decide. Model choice is already config, so the code
   change is documentation plus a preset.
6. SQLite export format. Add `cli export --format sqlite` via `VACUUM INTO`
   (SQLite 3.53+). One consistent file for backup and portability, next to the
   existing md and json. Small change in `crates/activity-monitor-cli`.

### Later, watch only

7. Windows AI APIs (NPU). Image Description and Text Recognition (OCR) are
   NPU-only and need Windows App SDK bindings. Phi Silica is being replaced by
   Aion Instruct (Insiders December 2026, retail January 2027), so work now
   would target a model with a removal date. Revisit after Aion lands and a
   Copilot+ test machine exists.
8. ScreenCaptureKit screen backend (macOS M2). Still Later. macOS 26 Tahoe has
   open SCK regressions (some captures return the desktop instead of app
   windows) and the recording indicator remains mandatory.
9. MCP protocol. 2026-07-28 is current. The next release is 2027-03-30 on the
   new six-month cycle, and the registry is still preview. No work until then.

### Rejected

- sqlite-vec and SQLite vec1. vec1 is pre-1.0 and not in the amalgamation or any
  distribution package; sqlite-vector is a third-party C extension. Both need an
  embedding model at runtime, which v1.2 deliberately removed. Hybrid FTS5 plus
  trigram stays.
- Phi Silica integration now. The model is removed from Windows in January 2027
  and the Windows App SDK binding cost is high.
- FTS5 CVE response. CVE-2026-11822 and CVE-2026-11824 are fixed in SQLite
  3.53.2, and rusqlite 0.40.2 already bundles that version (0.40.1 bumped it).
  The fixes in rusqlite 0.43 target Session, column_metadata and Filter paths
  this project does not use. A bump is optional maintenance, not a security
  need.

---

## Note on the GitHub release pages

Releases exist on `rust-release`: v1.0.0, v1.1.0 (2026-11-17), v1.2.0,
v1.3.0, v1.4.0 (2026-11-19). Each page
carries only the `publish2/` zip (service + CLI + MCP + `appsettings.json`
+ `install-service.ps1`) — never the legacy .NET `publish/` — plus
`SHA256SUMS.txt`, and from v1.4.0 also the `publish2-macos-*.zip` (3 binaries
+ `appsettings.json` + `install-macos.sh`, ad-hoc signed on the macos-15 CI
runner). Release body = `RELEASE_NOTES.md`. `main` stays the .NET original.
