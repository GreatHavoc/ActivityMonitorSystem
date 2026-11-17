//! SQLite storage layer. DDL and SQL ported verbatim from ActivityDatabase.cs so
//! the database file stays byte-compatible (same tables, indexes, "O" TEXT
//! timestamps). One long-lived connection replaces the C# per-operation churn.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use rusqlite::{params, Connection};

use crate::models::{ActivityEvent, InferenceResult};
use crate::timefmt::{format_o, parse_o};

const DDL: &str = "
    CREATE TABLE IF NOT EXISTS ActivityEvents (
        Id INTEGER PRIMARY KEY AUTOINCREMENT,
        Timestamp TEXT NOT NULL,
        EventType TEXT NOT NULL,
        ProcessId INTEGER,
        ProcessName TEXT,
        WindowTitle TEXT,
        IsIdle INTEGER NOT NULL,
        IdleDurationSeconds INTEGER,
        Metadata TEXT
    );
    CREATE INDEX IF NOT EXISTS IX_ActivityEvents_Timestamp ON ActivityEvents(Timestamp);
    CREATE INDEX IF NOT EXISTS IX_ActivityEvents_EventType ON ActivityEvents(EventType);
    CREATE INDEX IF NOT EXISTS IX_ActivityEvents_ProcessName ON ActivityEvents(ProcessName);
    CREATE TABLE IF NOT EXISTS InferenceResults (
        Id INTEGER PRIMARY KEY AUTOINCREMENT,
        RequestId TEXT NOT NULL UNIQUE,
        ProcessedAt TEXT NOT NULL,
        ActivityLabel TEXT NOT NULL,
        Application TEXT,
        ContentType TEXT,
        Topic TEXT,
        Action TEXT,
        Summary TEXT NOT NULL,
        VisibleText TEXT,
        Confidence REAL NOT NULL,
        DetectedObjects TEXT,
        RawResponse TEXT
    );
    CREATE INDEX IF NOT EXISTS IX_InferenceResults_ProcessedAt ON InferenceResults(ProcessedAt);
    CREATE INDEX IF NOT EXISTS IX_InferenceResults_ActivityLabel ON InferenceResults(ActivityLabel);
";

/// Row shape of the CLI `query` command's SELECT (ported verbatim).
pub struct EventRow {
    pub timestamp: String,
    pub event_type: String,
    pub process_name: Option<String>,
    pub window_title: Option<String>,
    pub is_idle: bool,
}

/// One keyword-search hit over the AI-written fields of `InferenceResults`
/// (v1.2 `search_activity`; keyword-only until the sqlite-vec index lands).
pub struct SearchHitRow {
    pub processed_at: String,
    pub activity_label: String,
    pub application: Option<String>,
    pub topic: Option<String>,
    pub summary: String,
    pub visible_text: Option<String>,
}

/// Keyword search over `InferenceResults` in a time window, newest first.
/// Case-insensitive LIKE with the caller escaping `%`, `_` and `\`.
pub fn search_inference_results(
    conn: &Connection,
    pattern: &str,
    from_o: &str,
    to_o: &str,
    limit: i64,
) -> rusqlite::Result<Vec<SearchHitRow>> {
    let mut stmt = conn.prepare(
        "SELECT ProcessedAt, ActivityLabel, Application, Topic, Summary, VisibleText
         FROM InferenceResults
         WHERE ProcessedAt >= ?1 AND ProcessedAt < ?2
           AND (COALESCE(Application,'') || ' ' || COALESCE(Topic,'') || ' '
                || Summary || ' ' || COALESCE(VisibleText,'')) LIKE ?3 ESCAPE '\\'
         ORDER BY ProcessedAt DESC LIMIT ?4",
    )?;
    let rows = stmt
        .query_map(params![from_o, to_o, pattern, limit], |r| {
            Ok(SearchHitRow {
                processed_at: r.get(0)?,
                activity_label: r.get(1)?,
                application: r.get(2)?,
                topic: r.get(3)?,
                summary: r.get(4)?,
                visible_text: r.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

/// Escapes LIKE wildcards in a user query so `search_inference_results`
/// matches literal text only.
pub fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c == '%' || c == '_' || c == '\\' {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub struct ActivityDatabase {
    conn: Connection,
}

impl ActivityDatabase {
    pub fn open(path: &Path) -> rusqlite::Result<Self> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)
                    .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
            }
        }
        Ok(Self { conn: Connection::open(path)? })
    }

    /// Read-only open for the MCP server: never creates the file and never
    /// blocks the service's writes (SQLite readers don't lock writers in WAL,
    /// and even in rollback mode a reader only takes a SHARED lock).
    pub fn open_read_only(path: &Path) -> rusqlite::Result<Self> {
        Ok(Self {
            conn: Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?,
        })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn initialize(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(DDL)?;
        self.migrate_schema()
    }

    /// Auto-migration from ActivityDatabase.cs: ALTER TABLE ADD COLUMN for the
    /// five columns that were added to an existing database after first release.
    fn migrate_schema(&self) -> rusqlite::Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(InferenceResults)")?;
        let existing: HashSet<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))?
            .collect::<rusqlite::Result<_>>()?;
        for (col, ty) in [
            ("Application", "TEXT"),
            ("ContentType", "TEXT"),
            ("Topic", "TEXT"),
            ("Action", "TEXT"),
            ("VisibleText", "TEXT"),
        ] {
            if !existing.contains(col) {
                self.conn
                    .execute_batch(&format!("ALTER TABLE InferenceResults ADD COLUMN {col} {ty}"))?;
            }
        }
        Ok(())
    }

    pub fn save_activity_event(&self, event: &ActivityEvent) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO ActivityEvents
                (Timestamp, EventType, ProcessId, ProcessName, WindowTitle, IsIdle, IdleDurationSeconds, Metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format_o(&event.timestamp),
                event.event_type,
                event.process_id,
                event.process_name,
                event.window_title,
                event.is_idle,
                event.idle_duration_seconds,
                event.metadata,
            ],
        )?;
        Ok(())
    }

    pub fn save_inference_result(&self, result: &InferenceResult) -> rusqlite::Result<()> {
        let detected = result
            .detected_objects
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        self.conn.execute(
            "INSERT INTO InferenceResults
                (RequestId, ProcessedAt, ActivityLabel, Application, ContentType, Topic, Action, Summary, VisibleText, Confidence, DetectedObjects, RawResponse)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                result.request_id.to_string(),
                format_o(&result.processed_at),
                result.activity_label,
                result.application,
                result.content_type,
                result.topic,
                result.action,
                result.summary,
                result.visible_text,
                result.confidence,
                detected,
                result.raw_response,
            ],
        )?;
        Ok(())
    }

    /// Storage-level query from ActivityDatabase.QueryEventsAsync (verbatim SQL).
    pub fn query_events(&self, start: &Timestamp, end: &Timestamp) -> rusqlite::Result<Vec<ActivityEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT Id, Timestamp, EventType, ProcessId, ProcessName, WindowTitle, IsIdle, IdleDurationSeconds, Metadata
             FROM ActivityEvents
             WHERE Timestamp BETWEEN ?1 AND ?2
             ORDER BY Timestamp DESC",
        )?;
        let rows = stmt.query_map(params![format_o(start), format_o(end)], |row| {
            let ts_text: String = row.get(1)?;
            let ts = parse_o(&ts_text).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
            })?;
            Ok(ActivityEvent {
                id: row.get(0)?,
                timestamp: ts,
                event_type: row.get(2)?,
                process_id: row.get(3)?,
                process_name: row.get(4)?,
                window_title: row.get(5)?,
                is_idle: row.get::<_, i64>(6)? == 1,
                idle_duration_seconds: row.get(7)?,
                metadata: row.get(8)?,
            })
        })?;
        rows.collect()
    }

    /// CLI `query` command rows (verbatim SELECT from Program.cs).
    pub fn query_event_rows(&self, from_o: &str, to_o: &str, limit: i64) -> rusqlite::Result<Vec<EventRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
             FROM ActivityEvents
             WHERE Timestamp BETWEEN ?1 AND ?2
             ORDER BY Timestamp DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![from_o, to_o, limit], |r| {
            Ok(EventRow {
                timestamp: r.get(0)?,
                event_type: r.get(1)?,
                process_name: r.get(2)?,
                window_title: r.get(3)?,
                is_idle: r.get::<_, i64>(4)? == 1,
            })
        })?;
        rows.collect()
    }

    pub fn count(&self, sql: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(sql, [], |r| r.get::<_, i64>(0))
    }

    pub fn count_events_since(&self, since_o: &str) -> rusqlite::Result<i64> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM ActivityEvents WHERE Timestamp >= ?1",
            params![since_o],
            |r| r.get::<_, i64>(0),
        )
    }

    /// Top processes, verbatim SQL from Program.cs ShowStatsAsync.
    pub fn top_processes(&self, since_o: &str) -> rusqlite::Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT ProcessName, COUNT(*) as Count
             FROM ActivityEvents
             WHERE ProcessName IS NOT NULL AND Timestamp >= ?1
             GROUP BY ProcessName
             ORDER BY Count DESC
             LIMIT 10",
        )?;
        let rows = stmt.query_map(params![since_o], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect()
    }

    /// Deletes events older than `max_age_days`, then VACUUMs (verbatim from
    /// CompactOldEventsAsync). The C# method was never called; the Rust service
    /// wires it into the daily loop (Phase 9).
    pub fn compact_old_events(&self, max_age_days: i64) -> rusqlite::Result<i64> {
        let secs = max_age_days.max(0) as u64 * 86_400;
        let cutoff = Timestamp::now() - std::time::Duration::from_secs(secs);
        let deleted = self.conn.execute(
            "DELETE FROM ActivityEvents WHERE Timestamp < ?1",
            params![format_o(&cutoff)],
        )?;
        self.conn.execute_batch("VACUUM")?;
        Ok(deleted as i64)
    }
}

/// CLI database resolution, same order as Program.cs GetDatabasePath():
/// ./Data/ActivityData.db, then %ProgramData%\ActivityMonitor\Data\ActivityData.db.
pub fn resolve_cli_db_path() -> Option<PathBuf> {
    let local = std::env::current_dir().ok()?.join("Data").join("ActivityData.db");
    if local.exists() {
        return Some(local);
    }
    let program_data =
        std::env::var_os("ProgramData").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    let pd = program_data.join("ActivityMonitor").join("Data").join("ActivityData.db");
    pd.exists().then_some(pd)
}

/// Row shape of the CLI `timeline` command's JOIN query (SQL verbatim from ShowTimelineAsync).
pub struct TimelineRow {
    pub timestamp: String,
    pub event_type: String,
    pub process_name: Option<String>,
    pub window_title: Option<String>,
    pub is_idle: bool,
    pub activity_label: Option<String>,
    pub application: Option<String>,
    pub content_type: Option<String>,
    pub topic: Option<String>,
    pub action: Option<String>,
    pub summary: Option<String>,
    pub visible_text: Option<String>,
}

/// Row shape of the CLI `detailed` command (SQL verbatim from ShowDetailedActivityAsync).
pub struct DetailedRow {
    pub processed_at: String,
    pub activity_label: Option<String>,
    pub application: Option<String>,
    pub content_type: Option<String>,
    pub topic: Option<String>,
    pub action: Option<String>,
    pub summary: Option<String>,
    pub visible_text: Option<String>,
    pub confidence: f64,
}

impl ActivityDatabase {
    pub fn timeline_entries(&self, from_o: &str, to_o: &str) -> rusqlite::Result<Vec<TimelineRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT ae.Timestamp, ae.EventType, ae.ProcessName, ae.WindowTitle, ae.IsIdle,
                    ir.ActivityLabel, ir.Application, ir.ContentType, ir.Topic, ir.Action, ir.Summary, ir.VisibleText
             FROM ActivityEvents ae
             LEFT JOIN InferenceResults ir ON DATE(ae.Timestamp) = DATE(ir.ProcessedAt)
                 AND ABS((JULIANDAY(ae.Timestamp) - JULIANDAY(ir.ProcessedAt)) * 86400.0) < 60
             WHERE ae.Timestamp BETWEEN ?1 AND ?2
             ORDER BY ae.Timestamp DESC
             LIMIT 500",
        )?;
        let rows = stmt.query_map(params![from_o, to_o], |r| {
            Ok(TimelineRow {
                timestamp: r.get(0)?,
                event_type: r.get(1)?,
                process_name: r.get(2)?,
                window_title: r.get(3)?,
                is_idle: r.get::<_, i64>(4)? == 1,
                activity_label: r.get(5)?,
                application: r.get(6)?,
                content_type: r.get(7)?,
                topic: r.get(8)?,
                action: r.get(9)?,
                summary: r.get(10)?,
                visible_text: r.get(11)?,
            })
        })?;
        rows.collect()
    }

    pub fn detailed_rows(&self, from_o: &str, to_o: &str, limit: i64) -> rusqlite::Result<Vec<DetailedRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT ProcessedAt, ActivityLabel, Application, ContentType, Topic, Action, Summary, VisibleText, Confidence
             FROM InferenceResults
             WHERE ProcessedAt BETWEEN ?1 AND ?2
             ORDER BY ProcessedAt DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![from_o, to_o, limit], |r| {
            Ok(DetailedRow {
                processed_at: r.get(0)?,
                activity_label: r.get(1)?,
                application: r.get(2)?,
                content_type: r.get(3)?,
                topic: r.get(4)?,
                action: r.get(5)?,
                summary: r.get(6)?,
                visible_text: r.get(7)?,
                confidence: r.get(8)?,
            })
        })?;
        rows.collect()
    }

    /// summary: activity by content type (SQL verbatim from ShowDailySummaryAsync).
    pub fn summary_categories(&self, from_o: &str, to_o: &str) -> rusqlite::Result<Vec<(String, i64, Option<String>)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(ContentType, 'Unknown') as Category, COUNT(*) as Count,
                    GROUP_CONCAT(Topic, ', ') as Topics
             FROM InferenceResults
             WHERE ProcessedAt BETWEEN ?1 AND ?2
             GROUP BY ContentType
             ORDER BY Count DESC",
        )?;
        let rows = stmt.query_map(params![from_o, to_o], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, Option<String>>(2)?))
        })?;
        rows.collect()
    }

    /// summary: top applications (SQL verbatim).
    pub fn summary_top_apps(&self, from_o: &str, to_o: &str) -> rusqlite::Result<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare(
            "SELECT COALESCE(Application, 'Unknown') as App, COUNT(*) as Count
             FROM InferenceResults
             WHERE ProcessedAt BETWEEN ?1 AND ?2
             GROUP BY Application
             ORDER BY Count DESC
             LIMIT 10",
        )?;
        let rows = stmt.query_map(params![from_o, to_o], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        rows.collect()
    }

    /// summary: distribution counts (SQL verbatim). (total, coding, web, documents, videos, communication)
    pub fn summary_distribution(&self, from_o: &str, to_o: &str) -> rusqlite::Result<(i64, i64, i64, i64, i64, i64)> {
        self.conn.query_row(
            "SELECT COUNT(*) as TotalActivities,
                    SUM(CASE WHEN ContentType = 'code' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN ContentType LIKE '%web%' OR ContentType = 'article' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN ContentType = 'document' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN ContentType = 'video' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN ContentType IN ('email', 'chat') THEN 1 ELSE 0 END)
             FROM InferenceResults
             WHERE ProcessedAt BETWEEN ?1 AND ?2",
            params![from_o, to_o],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, Option<i64>>(1)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(2)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(3)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(4)?.unwrap_or(0),
                    r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                ))
            },
        )
    }
}

/// Row shape for the report event queries (SQL verbatim from ExportActivityReportAsync).
#[derive(Clone)]
pub struct EventSnapshotRow {
    pub timestamp: String,
    pub event_type: String,
    pub process_name: Option<String>,
    pub window_title: Option<String>,
    pub is_idle: bool,
}

/// Row shape for the report insight query (SQL verbatim, ORDER BY ProcessedAt ASC).
pub struct InsightRow {
    pub processed_at: String,
    pub activity_label: Option<String>,
    pub application: Option<String>,
    pub content_type: Option<String>,
    pub topic: Option<String>,
    pub action: Option<String>,
    pub summary: Option<String>,
    pub visible_text: Option<String>,
    pub confidence: f64,
}

impl ActivityDatabase {
    fn map_snapshot(r: &rusqlite::Row<'_>) -> rusqlite::Result<EventSnapshotRow> {
        Ok(EventSnapshotRow {
            timestamp: r.get(0)?,
            event_type: r.get(1)?,
            process_name: r.get(2)?,
            window_title: r.get(3)?,
            is_idle: r.get::<_, i64>(4)? == 1,
        })
    }

    pub fn event_snapshots(&self, from_o: &str, to_o: &str) -> rusqlite::Result<Vec<EventSnapshotRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
             FROM ActivityEvents
             WHERE Timestamp BETWEEN ?1 AND ?2
             ORDER BY Timestamp ASC",
        )?;
        let rows = stmt.query_map(params![from_o, to_o], Self::map_snapshot)?;
        rows.collect()
    }

    pub fn previous_event(&self, before_o: &str) -> rusqlite::Result<Option<EventSnapshotRow>> {
        self.conn
            .query_row(
                "SELECT Timestamp, EventType, ProcessName, WindowTitle, IsIdle
                 FROM ActivityEvents
                 WHERE Timestamp < ?1
                 ORDER BY Timestamp DESC
                 LIMIT 1",
                params![before_o],
                Self::map_snapshot,
            )
            .optional()
    }

    pub fn insight_rows(&self, from_o: &str, to_o: &str) -> rusqlite::Result<Vec<InsightRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT ProcessedAt, ActivityLabel, Application, ContentType, Topic, Action, Summary, VisibleText, Confidence
             FROM InferenceResults
             WHERE ProcessedAt BETWEEN ?1 AND ?2
             ORDER BY ProcessedAt ASC",
        )?;
        let rows = stmt.query_map(params![from_o, to_o], |r| {
            Ok(InsightRow {
                processed_at: r.get(0)?,
                activity_label: r.get(1)?,
                application: r.get(2)?,
                content_type: r.get(3)?,
                topic: r.get(4)?,
                action: r.get(5)?,
                summary: r.get(6)?,
                visible_text: r.get(7)?,
                confidence: r.get(8)?,
            })
        })?;
        rows.collect()
    }
}

use rusqlite::OptionalExtension;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{BoundingBox, DetectedObject};

    fn temp_db(tag: &str) -> (PathBuf, ActivityDatabase) {
        let dir = std::env::temp_dir().join(format!("am-db-test-{tag}-{}", uuid::Uuid::new_v4()));
        let db = ActivityDatabase::open(&dir.join("test.db")).unwrap();
        db.initialize().unwrap();
        (dir, db)
    }

    fn event(ts: &str, process: Option<&str>) -> ActivityEvent {
        ActivityEvent {
            id: 0,
            timestamp: parse_o(ts).unwrap(),
            event_type: "FocusChange".into(),
            process_id: Some(101),
            process_name: process.map(String::from),
            window_title: Some("main.rs".into()),
            is_idle: false,
            idle_duration_seconds: None,
            metadata: None,
        }
    }

    #[test]
    fn stored_timestamp_is_dotnet_o_format() {
        let (dir, db) = temp_db("fmt");
        db.save_activity_event(&event("2026-11-13T08:05:00.0000000Z", Some("code"))).unwrap();
        let raw: String = db
            .conn
            .query_row("SELECT Timestamp FROM ActivityEvents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(raw, "2026-11-13T08:05:00.0000000Z");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn query_range_and_compact() {
        let (dir, db) = temp_db("range");
        db.save_activity_event(&event("2020-01-01T00:00:00.0000000Z", Some("oldproc"))).unwrap();
        db.save_activity_event(&event("2026-11-13T08:05:00.0000000Z", Some("code"))).unwrap();

        let all = db
            .query_events(
                &parse_o("2020-01-01T00:00:00.0000000Z").unwrap(),
                &parse_o("2026-12-31T00:00:00.0000000Z").unwrap(),
            )
            .unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].process_name.as_deref(), Some("code")); // DESC order

        let none = db
            .query_events(
                &parse_o("2026-12-01T00:00:00.0000000Z").unwrap(),
                &parse_o("2026-12-31T00:00:00.0000000Z").unwrap(),
            )
            .unwrap();
        assert!(none.is_empty());

        // 90-day cutoff deletes only the 2020 row
        let deleted = db.compact_old_events(90).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(db.count("SELECT COUNT(*) FROM ActivityEvents").unwrap(), 1);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migration_adds_missing_columns_to_legacy_db() {
        let dir = std::env::temp_dir().join(format!("am-db-test-mig-{}", uuid::Uuid::new_v4()));
        let path = dir.join("old.db");
        std::fs::create_dir_all(&dir).unwrap();
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                "CREATE TABLE InferenceResults (
                    Id INTEGER PRIMARY KEY AUTOINCREMENT,
                    RequestId TEXT NOT NULL UNIQUE,
                    ProcessedAt TEXT NOT NULL,
                    ActivityLabel TEXT NOT NULL,
                    Summary TEXT NOT NULL,
                    Confidence REAL NOT NULL,
                    DetectedObjects TEXT,
                    RawResponse TEXT
                );",
            )
            .unwrap();
        }
        let db = ActivityDatabase::open(&path).unwrap();
        db.initialize().unwrap();
        let mut stmt = db.conn.prepare("PRAGMA table_info(InferenceResults)").unwrap();
        let cols: HashSet<String> = stmt
            .query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        for c in ["Application", "ContentType", "Topic", "Action", "VisibleText"] {
            assert!(cols.contains(c), "missing column {c}");
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn inference_result_stores_pascal_case_json() {
        let (dir, db) = temp_db("inf");
        let res = InferenceResult {
            request_id: uuid::Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap(),
            processed_at: parse_o("2026-11-13T08:06:00.0000000Z").unwrap(),
            activity_label: "coding".into(),
            application: "code".into(),
            content_type: "development".into(),
            topic: "rust".into(),
            action: "writing".into(),
            summary: "Editing main.rs".into(),
            visible_text: "fn main()".into(),
            detected_objects: Some(vec![DetectedObject {
                label: "editor".into(),
                confidence: 0.9,
                bounding_box: Some(BoundingBox { x: 1, y: 2, width: 3, height: 4 }),
            }]),
            confidence: 0.91,
            raw_response: None,
        };
        db.save_inference_result(&res).unwrap();
        let (rid, det): (String, Option<String>) = db
            .conn
            .query_row("SELECT RequestId, DetectedObjects FROM InferenceResults", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(rid, "11111111-2222-3333-4444-555555555555");
        let det = det.unwrap();
        assert!(det.contains("\"Label\":\"editor\""), "got: {det}");
        assert!(det.contains("\"BoundingBox\":{\"X\":1"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn search_inference_results_matches_keywords_and_escapes_wildcards() {
        let (dir, db) = temp_db("search");
        let mk = |ts: &str, label: &str, app: &str, topic: &str| InferenceResult {
            request_id: uuid::Uuid::new_v4(),
            processed_at: parse_o(ts).unwrap(),
            activity_label: label.into(),
            application: app.into(),
            content_type: "image".into(),
            topic: topic.into(),
            action: "writing".into(),
            summary: format!("Working on {topic}"),
            visible_text: format!("window {topic}"),
            detected_objects: None,
            confidence: 0.9,
            raw_response: None,
        };
        db.save_inference_result(&mk(
            "2026-11-13T08:06:00.0000000Z",
            "coding",
            "code",
            "rust project",
        ))
        .unwrap();
        db.save_inference_result(&mk(
            "2026-11-13T09:06:00.0000000Z",
            "writing",
            "obsidian",
            "notes",
        ))
        .unwrap();

        let hits = search_inference_results(
            &db.conn,
            &format!("%{}%", escape_like("rust")),
            "2026-11-13T00:00:00.0000000Z",
            "2026-11-14T00:00:00.0000000Z",
            10,
        )
        .unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].topic.as_deref(), Some("rust project"));

        // a bare `%` in the query must be literal, not a wildcard
        let wildcard = search_inference_results(
            &db.conn,
            &format!("%{}%", escape_like("%")),
            "2026-11-13T00:00:00.0000000Z",
            "2026-11-14T00:00:00.0000000Z",
            10,
        )
        .unwrap();
        assert!(wildcard.is_empty());

        // out-of-window query misses
        let outside = search_inference_results(
            &db.conn,
            "%notes%",
            "2026-11-14T00:00:00.0000000Z",
            "2026-11-15T00:00:00.0000000Z",
            10,
        )
        .unwrap();
        assert!(outside.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
