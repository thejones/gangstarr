/// Steeze — AI briefing builder and CLI handler for gangstarr.
///
/// Named after hip-hop slang for effortless style — Steeze reads the
/// gangstarr.db, builds a prioritized context briefing, and hands it
/// to an AI agent (Kiro, etc.) so the human can walk away.
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Result, params};
use serde_json::Value;

use crate::storage;

// ── Briefing builder ──────────────────────────────────────────────────────────

/// Build a structured AI briefing from all data in the gangstarr DB.
///
/// The briefing is a JSON object with sections prioritized for AI consumption:
/// 1. correlated findings (static + runtime evidence at same callsite)
/// 2. high-count runtime findings
/// 3. static-only findings
/// 4. pg findings
/// 5. field usage summary
/// 6. top repeated query fingerprints
/// 7. recent runs metadata
pub fn build_briefing(db_path: &str) -> Result<Value> {
    let conn = storage::ensure_db(db_path)?;

    // ── Recent runs ───────────────────────────────────────────────────────
    let runs = fetch_recent_runs(&conn, 5)?;
    let run_ids: Vec<String> = runs
        .iter()
        .filter_map(|r| r["run_id"].as_str().map(String::from))
        .collect();

    // ── Static findings (all runs) ────────────────────────────────────────
    let static_findings = fetch_all_static(&conn)?;

    // ── Runtime findings (all runs) ───────────────────────────────────────
    let runtime_findings = fetch_all_runtime(&conn)?;

    // ── PG findings (all runs) ────────────────────────────────────────────
    let pg_findings = fetch_all_pg(&conn)?;

    // ── Correlations: match static findings against runtime evidence ──────
    let mut correlated: Vec<Value> = Vec::new();
    let mut uncorrelated_static: Vec<Value> = Vec::new();

    for sf in &static_findings {
        let file = sf["file"].as_str().unwrap_or("");
        let line = sf["line"].as_i64().unwrap_or(0);

        let runtime_matches = storage::fetch_runtime_at_callsite(&conn, file, line)?;

        if let Some(best) = runtime_matches.first() {
            let count = best["runtime_count"].as_i64().unwrap_or(0);
            if count > 0 {
                let mut entry = sf.clone();
                entry["runtime_evidence"] = serde_json::json!({
                    "query_count":  count,
                    "duration_ms":  best["runtime_duration_ms"].as_f64().unwrap_or(0.0),
                    "runtime_code": best["code"],
                });
                correlated.push(entry);
                continue;
            }
        }
        uncorrelated_static.push(sf.clone());
    }

    // ── Sort runtime findings by count desc ───────────────────────────────
    let mut runtime_sorted = runtime_findings;
    runtime_sorted.sort_by(|a, b| {
        let ca = a["count"].as_i64().unwrap_or(0);
        let cb = b["count"].as_i64().unwrap_or(0);
        cb.cmp(&ca)
    });

    // ── Query fingerprints (top 20 by count) ──────────────────────────────
    let fingerprints = fetch_top_fingerprints(&conn, 20)?;

    // ── Field usage ───────────────────────────────────────────────────────
    let field_usage = storage::fetch_field_usage_by_model(&conn)?;

    // ── Assemble briefing ─────────────────────────────────────────────────
    let briefing = serde_json::json!({
"gangstarr_version": "0.7.0",
        "generated_at": now_iso(),
        "db_path": db_path,
        "runs": runs,
        "run_ids": run_ids,
        "sections": {
            "correlated_findings": {
                "description": "Static findings confirmed by runtime evidence at the same callsite. Highest priority.",
                "count": correlated.len(),
                "items": correlated,
            },
            "runtime_findings": {
                "description": "Runtime-detected issues sorted by query count (descending).",
                "count": runtime_sorted.len(),
                "items": runtime_sorted,
            },
            "static_findings": {
                "description": "Static-only findings with no runtime confirmation yet.",
                "count": uncorrelated_static.len(),
                "items": uncorrelated_static,
            },
            "pg_findings": {
                "description": "PostgreSQL schema/stats findings from pg-royalty.",
                "count": pg_findings.len(),
                "items": pg_findings,
            },
            "field_usage": {
                "description": "Model fields actually accessed at runtime (useful for .only() suggestions).",
                "count": field_usage.len(),
                "items": field_usage,
            },
            "query_fingerprints": {
                "description": "Top repeated SQL queries by execution count.",
                "count": fingerprints.len(),
                "items": fingerprints,
            },
        },
    });

    Ok(briefing)
}

/// Store a briefing in the ai_briefings table.
pub fn store_briefing(db_path: &str, briefing: &Value) -> Result<()> {
    let conn = storage::ensure_db(db_path)?;
    let created_at = now_iso();
    let run_ids = serde_json::to_string(&briefing["run_ids"]).unwrap_or_else(|_| "[]".into());
    let json = serde_json::to_string(briefing).unwrap_or_else(|_| "{}".into());
    storage::insert_ai_briefing(&conn, &created_at, &run_ids, &json)
}

// ── CLI handler ───────────────────────────────────────────────────────────────

/// Run the `gangstarr steeze` subcommand.
///
/// `argv` is the full argument vector: `["gangstarr", "steeze", ...]`
pub fn run(argv: &[String]) -> i32 {
    // Parse optional path and --kiro flag.
    let path_str = argv
        .get(2)
        .map(String::as_str)
        .filter(|p| !p.starts_with('-'))
        .unwrap_or(".");

    let use_kiro = argv.iter().any(|a| a == "--kiro");

    let path = Path::new(path_str);
    let output_dir = if path.is_dir() {
        path.join(".gangstarr").to_string_lossy().into_owned()
    } else {
        path.parent()
            .unwrap_or(Path::new("."))
            .join(".gangstarr")
            .to_string_lossy()
            .into_owned()
    };
    let db_path = format!("{}/gangstarr.db", output_dir);

    // Check that the DB exists.
    if !Path::new(&db_path).exists() {
        eprintln!("error: no gangstarr database found at {}", db_path);
        eprintln!("Run `gangstarr check <path>` first to create findings.");
        return 2;
    }

    // Build the briefing.
    let briefing = match build_briefing(&db_path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: could not build briefing: {}", e);
            return 2;
        }
    };

    // Check if there's anything worth briefing about.
    let sections = &briefing["sections"];
    let total_findings = [
        "correlated_findings",
        "runtime_findings",
        "static_findings",
        "pg_findings",
    ]
    .iter()
    .filter_map(|k| sections[*k]["count"].as_u64())
    .sum::<u64>();

    if total_findings == 0 {
        println!("No findings in the database. Nothing to brief.");
        return 0;
    }

    // Always print the human-readable summary.
    print_briefing_summary(&briefing);

    if use_kiro {
        // Store briefing in the DB so the Kiro agent can read it.
        if let Err(e) = store_briefing(&db_path, &briefing) {
            eprintln!("warning: could not store briefing in DB: {}", e);
        } else {
            println!("\n\x1b[2mBriefing stored in {}\x1b[0m", db_path);
        }

        // Launch kiro-cli.
        launch_kiro();
    }

    0
}

// ── Console output ────────────────────────────────────────────────────────────

fn print_briefing_summary(briefing: &Value) {
    let sections = &briefing["sections"];

    let corr_count = sections["correlated_findings"]["count"]
        .as_u64()
        .unwrap_or(0);
    let rt_count = sections["runtime_findings"]["count"].as_u64().unwrap_or(0);
    let st_count = sections["static_findings"]["count"].as_u64().unwrap_or(0);
    let pg_count = sections["pg_findings"]["count"].as_u64().unwrap_or(0);
    let fp_count = sections["query_fingerprints"]["count"]
        .as_u64()
        .unwrap_or(0);
    let fu_count = sections["field_usage"]["count"].as_u64().unwrap_or(0);

    println!();
    println!("── Steeze Briefing ──────────────────────────────────────────────────────");
    println!(
        "  \x1b[1mCorrelated\x1b[0m (static + runtime):   {}",
        corr_count
    );
    println!("  Runtime findings:                {}", rt_count);
    println!("  Static-only findings:            {}", st_count);
    println!("  Postgres findings:               {}", pg_count);
    println!("  Query fingerprints:              {}", fp_count);
    println!("  Field usage models:              {}", fu_count);
    println!("─────────────────────────────────────────────────────────────────────────");

    // Show top correlated findings.
    if let Some(items) = sections["correlated_findings"]["items"].as_array() {
        if !items.is_empty() {
            println!("\n  \x1b[1mTop correlated findings:\x1b[0m");
            for (i, item) in items.iter().take(10).enumerate() {
                let rule = item["rule"].as_str().unwrap_or("?");
                let file = item["file"].as_str().unwrap_or("?");
                let line = item["line"].as_i64().unwrap_or(0);
                let msg = item["message"].as_str().unwrap_or("");
                let rt_count = item["runtime_evidence"]["query_count"]
                    .as_i64()
                    .unwrap_or(0);

                let msg_short = if msg.chars().count() > 60 {
                    let truncated: String = msg.chars().take(59).collect();
                    format!("{}…", truncated)
                } else {
                    msg.to_string()
                };

                println!(
                    "  {}. \x1b[33m{}\x1b[0m  {}:{}  [{}x runtime]  {}",
                    i + 1,
                    rule,
                    file,
                    line,
                    rt_count,
                    msg_short
                );
            }
        }
    }

    // Show top static-only findings.
    if let Some(items) = sections["static_findings"]["items"].as_array() {
        if !items.is_empty() {
            println!("\n  \x1b[1mTop static-only findings:\x1b[0m");
            for (i, item) in items.iter().take(5).enumerate() {
                let rule = item["rule"].as_str().unwrap_or("?");
                let file = item["file"].as_str().unwrap_or("?");
                let line = item["line"].as_i64().unwrap_or(0);
                let msg = item["message"].as_str().unwrap_or("");

                let msg_short = if msg.chars().count() > 60 {
                    let truncated: String = msg.chars().take(59).collect();
                    format!("{}…", truncated)
                } else {
                    msg.to_string()
                };

                println!(
                    "  {}. \x1b[33m{}\x1b[0m  {}:{}  {}",
                    i + 1,
                    rule,
                    file,
                    line,
                    msg_short
                );
            }
        }
    }

    println!();
}

fn launch_kiro() {
    println!("\n\x1b[1mLaunching Kiro steeze agent…\x1b[0m\n");

    let status = std::process::Command::new("kiro-cli")
        .args(["--agent", "steeze"])
        .status();

    match status {
        Ok(s) if s.success() => {}
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            eprintln!("kiro-cli exited with code {}", code);
        }
        Err(e) => {
            eprintln!("error: could not launch kiro-cli: {}", e);
            eprintln!();
            eprintln!("Install Kiro CLI:");
            eprintln!("  curl -fsSL https://cli.kiro.dev/install | bash");
            eprintln!();
            eprintln!("Or run without --kiro to just print the briefing:");
            eprintln!("  gangstarr steeze");
        }
    }
}

// ── DB helpers ────────────────────────────────────────────────────────────────

fn fetch_recent_runs(conn: &rusqlite::Connection, limit: usize) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT run_id, created_at, run_type, project_root FROM runs
         ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(serde_json::json!({
            "run_id":       row.get::<_, String>(0)?,
            "created_at":   row.get::<_, String>(1)?,
            "run_type":     row.get::<_, String>(2)?,
            "project_root": row.get::<_, String>(3)?,
        }))
    })?;
    rows.collect()
}

fn fetch_all_static(conn: &rusqlite::Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT sf.run_id, sf.rule, sf.message, sf.severity, sf.file, sf.line, sf.col, sf.suggestion
         FROM static_findings sf
         JOIN runs r ON r.run_id = sf.run_id
         ORDER BY r.created_at DESC, sf.file, sf.line",
    )?;
    let rows = stmt.query_map([], |row| {
        let suggestion: Option<String> = row.get(7)?;
        Ok(serde_json::json!({
            "run_id":     row.get::<_, String>(0)?,
            "rule":       row.get::<_, String>(1)?,
            "message":    row.get::<_, String>(2)?,
            "severity":   row.get::<_, String>(3)?,
            "file":       row.get::<_, String>(4)?,
            "line":       row.get::<_, i64>(5)?,
            "col":        row.get::<_, i64>(6)?,
            "suggestion": suggestion,
        }))
    })?;
    rows.collect()
}

fn fetch_all_runtime(conn: &rusqlite::Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT rf.run_id, rf.code, rf.title, rf.severity, rf.message,
                rf.fingerprint, rf.file, rf.line, rf.suggestion
         FROM runtime_findings rf
         JOIN runs r ON r.run_id = rf.run_id
         ORDER BY r.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "run_id":      row.get::<_, String>(0)?,
            "code":        row.get::<_, String>(1)?,
            "title":       row.get::<_, String>(2)?,
            "severity":    row.get::<_, String>(3)?,
            "message":     row.get::<_, String>(4)?,
            "fingerprint": row.get::<_, Option<String>>(5)?,
            "file":        row.get::<_, Option<String>>(6)?,
            "line":        row.get::<_, Option<i64>>(7)?,
            "suggestion":  row.get::<_, Option<String>>(8)?,
        }))
    })?;
    rows.collect()
}

fn fetch_all_pg(conn: &rusqlite::Connection) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT pf.run_id, pf.code, pf.severity, pf.table_name, pf.column_name,
                pf.message, pf.suggestion, pf.created_at
         FROM pg_findings pf
         ORDER BY pf.created_at DESC",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(serde_json::json!({
            "run_id":      row.get::<_, String>(0)?,
            "code":        row.get::<_, String>(1)?,
            "severity":    row.get::<_, String>(2)?,
            "table_name":  row.get::<_, Option<String>>(3)?,
            "column_name": row.get::<_, Option<String>>(4)?,
            "message":     row.get::<_, String>(5)?,
            "suggestion":  row.get::<_, Option<String>>(6)?,
            "created_at":  row.get::<_, String>(7)?,
        }))
    })?;
    rows.collect()
}

fn fetch_top_fingerprints(conn: &rusqlite::Connection, limit: usize) -> Result<Vec<Value>> {
    let mut stmt = conn.prepare(
        "SELECT fingerprint, normalized_sql, SUM(count) as total_count,
                SUM(total_duration_ms) as total_ms, file, line
         FROM query_fingerprints
         GROUP BY fingerprint
         ORDER BY total_count DESC
         LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit as i64], |row| {
        Ok(serde_json::json!({
            "fingerprint":    row.get::<_, String>(0)?,
            "normalized_sql": row.get::<_, String>(1)?,
            "total_count":    row.get::<_, i64>(2)?,
            "total_ms":       row.get::<_, f64>(3)?,
            "file":           row.get::<_, Option<String>>(4)?,
            "line":           row.get::<_, Option<i64>>(5)?,
        }))
    })?;
    rows.collect()
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn now_iso() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    // Reuse the civil calendar algorithm from cli.rs.
    let secs = (millis / 1000) as u64;
    let ms = millis % 1000;
    let days = secs / 86_400;
    let time = secs % 86_400;
    let h = time / 3_600;
    let mi = (time % 3_600) / 60;
    let s = time % 60;

    let z = days as i64 + 719_468;
    let era = z / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y_base = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y_base + 1 } else { y_base };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, d, h, mi, s, ms
    )
}
