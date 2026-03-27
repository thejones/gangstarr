/// JazzThing — `gangstarr pg-royalty` CLI entry point.
///
/// Dispatches to `pg_schema` (--review) or `pg_stats` (--stat-findings),
/// stores results in the local SQLite DB, and exits with a POSIX code.
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{pg_schema, pg_stats, storage};

// ── Parse helpers (mirror cli.rs) ─────────────────────────────────────────────

fn parse_flag(argv: &[String], flag: &str) -> Option<String> {
    for i in 0..argv.len() {
        if argv[i] == flag {
            return argv.get(i + 1).cloned();
        }
    }
    None
}


// ── Timestamp helper (reuse logic from cli.rs) ────────────────────────────────

fn now_iso() -> (String, String) {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let run_id = format!("pg{:014x}", millis);
    // Simple ISO-8601 via millis (same algorithm as cli.rs)
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
    let created_at = format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, d, h, mi, s, ms
    );
    (run_id, created_at)
}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run the `pg-royalty` subcommand. Returns a POSIX exit code.
pub fn run(argv: &[String]) -> i32 {
    let db_url = match parse_flag(argv, "--db-url") {
        Some(url) => url,
        None => {
            eprintln!(
                "\x1b[1merror:\x1b[0m no database URL provided.\n\
                 \x1b[2mhint:\x1b[0m  Run from a Django project directory (gangstarr will read\n\
                 \x1b[2m        DATABASES['default'] from your settings automatically), or\n\
                 \x1b[2m        pass --db-url postgresql://user:pass@host:5432/dbname\x1b[0m"
            );
            return 2;
        }
    };

    let output_dir = parse_flag(argv, "--output-dir").unwrap_or_else(|| ".gangstarr".to_string());
    let db_path = format!("{}/gangstarr.db", output_dir);

    let (run_id, created_at) = now_iso();

    // Ensure SQLite DB exists before we write findings.
    if let Ok(conn) = storage::ensure_db(&db_path) {
        let _ = storage::insert_run(&conn, &run_id, &created_at, "pg", &db_url);
    }

    let mut total_exit = 0i32;

    // Always run both schema review and stat findings.
    {
        let (findings, exit_code) = pg_schema::run_review(&db_url);
        total_exit = total_exit.max(exit_code);
        if exit_code <= 1 && !findings.is_empty() {
            if let Ok(conn) = storage::ensure_db(&db_path) {
                let _ = storage::insert_pg_findings(&conn, &run_id, &created_at, &findings);
            }
        }
    }

    {
        let (findings, exit_code) = pg_stats::run_stat_findings(&db_url, &db_path);
        total_exit = total_exit.max(exit_code);
        if exit_code <= 1 && !findings.is_empty() {
            if let Ok(conn) = storage::ensure_db(&db_path) {
                let _ = storage::insert_pg_findings(&conn, &run_id, &created_at, &findings);
            }
        }
    }

    if total_exit <= 1 {
        println!(
            "\x1b[2mFindings stored in {}/gangstarr.db  \
             (run `gangstarr history --findings` to review)\x1b[0m",
            output_dir
        );
    }

    total_exit
}

// ── Usage ─────────────────────────────────────────────────────────────────────

#[allow(dead_code)]
pub fn print_usage() {
    println!("gangstarr pg-royalty — live Postgres schema & statistics analysis");
    println!();
    println!("USAGE:");
    println!("    gangstarr pg-royalty              Run full schema review + stat analysis");
    println!();
    println!("CONNECTION:");
    println!("    --db-url <url>    PostgreSQL connection URL");
    println!("                      postgresql://user:pass@host:5432/dbname");
    println!("                      (auto-detected from Django settings if omitted)");
    println!();
    println!("OPTIONS:");
    println!("    --output-dir <dir>   Directory for gangstarr.db  (default: .gangstarr)");
    println!();
    println!("FINDINGS:");
    println!("    G201  Missing index on FK column / table without PK / wide table");
    println!("    G202  High rows/call ratio — possible .all() or missing LIMIT");
    println!("    G203  Unused index");
    println!("    G204  Unstable query plan — high stddev/mean execution time");
    println!("    G205  Sequential scans on large tables — likely missing index");
    println!("    G206  Table bloat — high dead tuple ratio");
    println!("    G207  Cache miss rate — table not fitting in shared_buffers");
    println!();
    println!("SAFETY GUARANTEE:");
    println!("    All SQL executed against your database is read-only (SELECT / catalog");
    println!("    queries only). No DDL, DML, or writes of any kind are performed.");
    println!();
    println!("EXIT CODES:");
    println!("    0  No issues found");
    println!("    1  Issues found");
    println!("    2  Usage or connection error");
}
