/// TakeItPersonal — local SQLite storage for gangstarr findings.
///
/// Named after the Gang Starr song "Take It Personal" — it stores
/// YOUR personal performance findings across every run.
use rusqlite::{Connection, Result, params};
use serde_json::Value;

// ── Schema ────────────────────────────────────────────────────────────────────

const CREATE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS runs (
    run_id       TEXT PRIMARY KEY,
    created_at   TEXT NOT NULL,
    run_type     TEXT NOT NULL,
    project_root TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS static_findings (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id     TEXT NOT NULL REFERENCES runs(run_id),
    rule       TEXT NOT NULL,
    message    TEXT NOT NULL,
    severity   TEXT NOT NULL,
    file       TEXT NOT NULL,
    line       INTEGER NOT NULL,
    col        INTEGER NOT NULL,
    suggestion TEXT
);

CREATE TABLE IF NOT EXISTS runtime_findings (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id        TEXT NOT NULL REFERENCES runs(run_id),
    code          TEXT NOT NULL,
    title         TEXT NOT NULL,
    severity      TEXT NOT NULL,
    message       TEXT NOT NULL,
    fingerprint   TEXT,
    file          TEXT,
    line          INTEGER,
    suggestion    TEXT,
    resolver_path TEXT
);

CREATE TABLE IF NOT EXISTS query_fingerprints (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id           TEXT NOT NULL REFERENCES runs(run_id),
    fingerprint      TEXT NOT NULL,
    normalized_sql   TEXT NOT NULL,
    count            INTEGER NOT NULL,
    total_duration_ms REAL NOT NULL,
    avg_duration_ms  REAL NOT NULL,
    file             TEXT,
    line             INTEGER
);

CREATE TABLE IF NOT EXISTS field_usage (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id     TEXT    NOT NULL REFERENCES runs(run_id),
    model      TEXT    NOT NULL,
    field      TEXT    NOT NULL,
    endpoint   TEXT,
    serializer TEXT
);

CREATE TABLE IF NOT EXISTS pg_findings (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    run_id      TEXT    NOT NULL REFERENCES runs(run_id),
    code        TEXT    NOT NULL,
    severity    TEXT    NOT NULL,
    table_name  TEXT,
    column_name TEXT,
    message     TEXT    NOT NULL,
    suggestion  TEXT,
    created_at  TEXT    NOT NULL
);
";

// ── Connection + migration ────────────────────────────────────────────────────

/// Open (or create) the SQLite database and ensure schema is current.
pub fn ensure_db(db_path: &str) -> Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch(CREATE_SCHEMA)?;
    Ok(conn)
}

// ── Inserts ───────────────────────────────────────────────────────────────────

/// Register a new analysis run.
pub fn insert_run(
    conn: &Connection,
    run_id: &str,
    created_at: &str,
    run_type: &str,
    project_root: &str,
) -> Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO runs (run_id, created_at, run_type, project_root)
         VALUES (?1, ?2, ?3, ?4)",
        params![run_id, created_at, run_type, project_root],
    )?;
    Ok(())
}

/// Batch-insert static analysis findings for a run.
pub fn insert_static_findings(conn: &Connection, run_id: &str, findings: &[Value]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO static_findings
             (run_id, rule, message, severity, file, line, col, suggestion)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for f in findings {
        stmt.execute(params![
            run_id,
            f["rule"].as_str().unwrap_or(""),
            f["message"].as_str().unwrap_or(""),
            f["severity"].as_str().unwrap_or("warning"),
            f["file"].as_str().unwrap_or(""),
            f["line"].as_i64().unwrap_or(0),
            f["col"].as_i64().unwrap_or(0),
            f["suggestion"].as_str(),
        ])?;
    }
    Ok(())
}

/// Batch-insert runtime findings from the analysis engine.
pub fn insert_runtime_findings(conn: &Connection, run_id: &str, findings: &[Value]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO runtime_findings
             (run_id, code, title, severity, message, fingerprint, file, line,
              suggestion, resolver_path)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    for f in findings {
        stmt.execute(params![
            run_id,
            f["code"].as_str().unwrap_or(""),
            f["title"].as_str().unwrap_or(""),
            f["severity"].as_str().unwrap_or("info"),
            f["message"].as_str().unwrap_or(""),
            f["fingerprint"].as_str(),
            f["file"].as_str(),
            f["line"].as_i64(),
            f["suggestion"].as_str(),
            f["resolver_path"].as_str().filter(|s| !s.is_empty()),
        ])?;
    }
    Ok(())
}

/// Batch-insert query fingerprint execution statistics.
pub fn insert_query_fingerprints(conn: &Connection, run_id: &str, groups: &[Value]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO query_fingerprints
             (run_id, fingerprint, normalized_sql, count,
              total_duration_ms, avg_duration_ms, file, line)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for g in groups {
        // Use the top callsite for the file/line attribution.
        let top = g["callsites"].as_array().and_then(|cs| cs.first());
        stmt.execute(params![
            run_id,
            g["fingerprint"].as_str().unwrap_or(""),
            g["normalized_sql"].as_str().unwrap_or(""),
            g["count"].as_i64().unwrap_or(0),
            g["total_duration_ms"].as_f64().unwrap_or(0.0),
            g["avg_duration_ms"].as_f64().unwrap_or(0.0),
            top.and_then(|cs| cs["file"].as_str()),
            top.and_then(|cs| cs["line"].as_i64()),
        ])?;
    }
    Ok(())
}

/// Batch-insert field usage records from a serializer run.
pub fn insert_field_usage(conn: &Connection, run_id: &str, records: &[Value]) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO field_usage (run_id, model, field, endpoint, serializer)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for r in records {
        stmt.execute(params![
            run_id,
            r["model"].as_str().unwrap_or(""),
            r["field"].as_str().unwrap_or(""),
            r["endpoint"].as_str(),
            r["serializer"].as_str(),
        ])?;
    }
    Ok(())
}

// ── Queries ───────────────────────────────────────────────────────────────────

/// Fetch the most recent `limit` runs with aggregate finding counts.
pub fn fetch_run_history(db_path: &str, limit: usize) -> Result<Vec<Value>> {
    let conn = ensure_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT r.run_id, r.created_at, r.run_type, r.project_root,
                COUNT(DISTINCT sf.id)  AS static_count,
                COUNT(DISTINCT rf.id)  AS runtime_count
         FROM runs r
         LEFT JOIN static_findings  sf ON sf.run_id = r.run_id
         LEFT JOIN runtime_findings rf ON rf.run_id = r.run_id
         GROUP BY r.run_id
         ORDER BY r.created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(serde_json::json!({
            "run_id":        row.get::<_, String>(0)?,
            "created_at":    row.get::<_, String>(1)?,
            "run_type":      row.get::<_, String>(2)?,
            "project_root":  row.get::<_, String>(3)?,
            "static_count":  row.get::<_, i64>(4)?,
            "runtime_count": row.get::<_, i64>(5)?,
        }))
    })?;
    rows.collect()
}

/// Fetch all static findings stored for a specific run.
pub fn fetch_static_findings(conn: &Connection, run_id: &str) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT rule, message, severity, file, line, col, suggestion
         FROM static_findings
         WHERE run_id = ?1
         ORDER BY file, line",
    )?;
    let rows = stmt.query_map(params![run_id], |row| {
        let suggestion: Option<String> = row.get(6)?;
        Ok(serde_json::json!({
            "rule":       row.get::<_, String>(0)?,
            "message":    row.get::<_, String>(1)?,
            "severity":   row.get::<_, String>(2)?,
            "file":       row.get::<_, String>(3)?,
            "line":       row.get::<_, i64>(4)?,
            "col":        row.get::<_, i64>(5)?,
            "suggestion": suggestion,
        }))
    })?;
    rows.collect()
}

/// Fetch all runtime findings (across all runs) that are attributed to a
/// matching file/line combination.  Uses basename matching to bridge the
/// gap between static (relative) and runtime (possibly absolute) paths.
pub fn fetch_runtime_at_callsite(
    conn: &Connection,
    static_file: &str,
    line: i64,
) -> Result<Vec<Value>> {
    // Pull all runtime findings + the peak execution count for their fingerprint.
    let mut stmt = conn.prepare(
        "SELECT rf.code, rf.severity, rf.message, rf.file, rf.line,
                COALESCE(MAX(qf.count), 0)             AS runtime_count,
                COALESCE(MAX(qf.total_duration_ms), 0) AS duration_ms
         FROM runtime_findings rf
         LEFT JOIN query_fingerprints qf
               ON qf.run_id = rf.run_id
              AND qf.file   = rf.file
         GROUP BY rf.id
         ORDER BY runtime_count DESC",
    )?;

    let basename = static_file
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(static_file);

    let mut results: Vec<Value> = Vec::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,          // code
            row.get::<_, String>(1)?,          // severity
            row.get::<_, String>(2)?,          // message
            row.get::<_, Option<String>>(3)?,  // file
            row.get::<_, Option<i64>>(4)?,     // line
            row.get::<_, i64>(5)?,             // runtime_count
            row.get::<_, f64>(6)?,             // duration_ms
        ))
    })?;

    for row in rows {
        let (code, sev, msg, r_file, r_line, count, dur) = row?;

        let file_matches = r_file.as_deref().map_or(false, |rf| {
            let rf_base = rf.rsplit(['/', '\\']).next().unwrap_or(rf);
            rf_base == basename || rf.ends_with(static_file) || static_file.ends_with(rf)
        });

        let line_matches = r_line.map_or(false, |rl| (rl - line).abs() <= 2);

        if file_matches && line_matches {
            results.push(serde_json::json!({
                "code":                code,
                "severity":            sev,
                "message":             msg,
                "runtime_count":       count,
                "runtime_duration_ms": dur,
            }));
        }
    }

    // Already ordered DESC by count from SQL; stable after filter.
    Ok(results)
}

/// Return a deduplicated list of model fields observed across all field_usage
/// records, grouped by model.  Used by AboveTheClouds to suggest .only() fields.
pub fn fetch_field_usage_by_model(conn: &Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT model, GROUP_CONCAT(DISTINCT field) AS fields
         FROM field_usage
         GROUP BY model
         ORDER BY model",
    )?;
    let rows = stmt.query_map([], |row| {
        let fields_raw: String = row.get(1).unwrap_or_default();
        let fields: Vec<&str> = fields_raw.split(',').collect();
        Ok(serde_json::json!({
            "model":  row.get::<_, String>(0)?,
            "fields": fields,
        }))
    })?;
    rows.collect()
}

// ── pg_findings ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PgFinding {
    pub code: String,
    pub severity: String,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Batch-insert Postgres introspection findings for a run.
pub fn insert_pg_findings(
    conn: &Connection,
    run_id: &str,
    created_at: &str,
    findings: &[PgFinding],
) -> Result<()> {
    let mut stmt = conn.prepare(
        "INSERT INTO pg_findings
             (run_id, code, severity, table_name, column_name, message, suggestion, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for f in findings {
        stmt.execute(params![
            run_id,
            f.code,
            f.severity,
            f.table_name,
            f.column_name,
            f.message,
            f.suggestion,
            created_at,
        ])?;
    }
    Ok(())
}

/// Fetch all findings across static, runtime, and pg sources, most recent first.
/// Used by `gangstarr history --findings`.
pub fn fetch_all_findings(db_path: &str, limit: usize) -> Result<Vec<Value>> {
    let conn = ensure_db(db_path)?;
    let mut stmt = conn.prepare(
        "SELECT 'static' AS source, r.created_at, sf.rule AS code, sf.severity,
                sf.file, sf.line, sf.message, NULL AS table_name, NULL AS column_name
         FROM static_findings sf
         JOIN runs r ON r.run_id = sf.run_id
         UNION ALL
         SELECT 'runtime', r.created_at, rf.code, rf.severity,
                rf.file, rf.line, rf.message, NULL, NULL
         FROM runtime_findings rf
         JOIN runs r ON r.run_id = rf.run_id
         UNION ALL
         SELECT 'pg', pf.created_at, pf.code, pf.severity,
                NULL, NULL, pf.message, pf.table_name, pf.column_name
         FROM pg_findings pf
         ORDER BY created_at DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        let file: Option<String> = row.get(4)?;
        let line: Option<i64> = row.get(5)?;
        let table_name: Option<String> = row.get(7)?;
        let col_name: Option<String> = row.get(8)?;
        Ok(serde_json::json!({
            "source":      row.get::<_, String>(0)?,
            "created_at":  row.get::<_, String>(1)?,
            "code":        row.get::<_, String>(2)?,
            "severity":    row.get::<_, String>(3)?,
            "file":        file,
            "line":        line,
            "message":     row.get::<_, String>(6)?,
            "table_name":  table_name,
            "column_name": col_name,
        }))
    })?;
    rows.collect()
}
