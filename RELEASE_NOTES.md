# Release Notes — Rust port (publish2/)

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
- 3-minute periodic capture and the 10 s event-capture throttle remain
  hardcoded, as in C#.
- `DistinctTopics`/`DistinctApplications` in the standard report are sorted
  ordinally in Rust vs culture-aware in C# (`OrderBy(t => t)`), so ties or
  case-differing free-text topics can appear in a different order (found by
  the Phase 11 soak; cosmetic only — same set, same counts).
- The Rust service loads `appsettings.json` from the exe directory; the .NET
  host loads it from the working directory. Identical in the standard
  deployment (service runs from its publish folder); the soak had to place a
  copy of the config in the CWD for the .NET side.
