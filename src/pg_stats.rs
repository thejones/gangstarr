/// JazzThing — `gangstarr pg-royalty --stat-findings`
///
/// Queries `pg_stat_statements` to surface the most expensive, most repeated,
/// and most unstable queries in the connected Postgres database, then
/// cross-references them with gangstarr's own runtime fingerprints.
///
/// All SQL executed here is read-only (SELECT only).
use postgres::{Client, NoTls};

use crate::storage::{self, PgFinding};

// ── ANSI colours ─────────────────────────────────────────────────────────────

const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const DOUBLE_LINE: &str = "══════════════════════════════════════════════════════════════════════════════";
const SINGLE_LINE: &str = "──────────────────────────────────────────────────────────────────────────────";

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run pg_stat_statements analysis. Returns (findings, exit_code).
pub fn run_stat_findings(db_url: &str, gangstarr_db: &str) -> (Vec<PgFinding>, i32) {
    let mut client = match Client::connect(db_url, NoTls) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}error:{} could not connect to database: {}", BOLD, RESET, e);
            return (vec![], 2);
        }
    };

    println!("{}{}{}", BOLD, DOUBLE_LINE, RESET);
    println!("{}STAT FINDINGS{}", BOLD, RESET);
    println!("{}{}", DIM, DOUBLE_LINE);
    println!("Read-only pg_stat_statements analysis — no writes performed.{}", RESET);
    println!();

    // 1. Check extension is available
    if !check_extension(&mut client) {
        eprintln!(
            "{}error:{} pg_stat_statements extension is not installed.\n\
             {}hint:{}  Ask your DBA to run: CREATE EXTENSION pg_stat_statements;\n\
             {}        Then add shared_preload_libraries = 'pg_stat_statements' to postgresql.conf.{}",
            BOLD, RESET, DIM, RESET, DIM, RESET
        );
        return (vec![], 2);
    }

    let mut all_findings: Vec<PgFinding> = Vec::new();

    // 2. Top queries by total execution time
    print_top_queries(&mut client);

    // 3. High-stddev queries (unstable execution plans) → G204
    let unstable = check_unstable_queries(&mut client);
    all_findings.extend(unstable);

    // 4. High rows/call ratio (over-fetching) → G202
    let over_fetch = check_high_row_ratio(&mut client);
    all_findings.extend(over_fetch);

    // 5. Cross-reference with gangstarr fingerprints
    cross_reference_fingerprints(&mut client, gangstarr_db, &mut all_findings);

    // ── Print summary ────────────────────────────────────────────────────────
    println!();
    println!("{}{}{}", BOLD, SINGLE_LINE, RESET);
    if all_findings.is_empty() {
        println!("{}✓  No stat findings. Database query patterns look healthy.{}", "\x1b[32m", RESET);
    } else {
        println!(
            "{}Found {} finding(s) from pg_stat_statements.{}",
            BOLD, all_findings.len(), RESET
        );
    }
    println!("{}{}{}", DIM, DOUBLE_LINE, RESET);

    let exit_code = if all_findings.is_empty() { 0 } else { 1 };
    (all_findings, exit_code)
}

// ── Checks ────────────────────────────────────────────────────────────────────

fn check_extension(client: &mut Client) -> bool {
    match client.query_one(
        "SELECT EXISTS(SELECT 1 FROM pg_extension WHERE extname = 'pg_stat_statements')",
        &[],
    ) {
        Ok(row) => row.get::<_, bool>(0),
        Err(_) => false,
    }
}

fn print_top_queries(client: &mut Client) {
    // Postgres 13+ uses total_exec_time; older versions use total_time.
    // Try the newer column name first.
    let sql = "SELECT query,
                      calls,
                      total_exec_time,
                      mean_exec_time,
                      stddev_exec_time,
                      rows
               FROM pg_stat_statements
               ORDER BY total_exec_time DESC
               LIMIT 10";

    let rows = match client.query(sql, &[]) {
        Ok(r) => r,
        Err(_) => {
            // Fall back to pre-13 column name
            match client.query(
                "SELECT query, calls, total_time AS total_exec_time,
                        mean_time AS mean_exec_time,
                        stddev_time AS stddev_exec_time, rows
                 FROM pg_stat_statements
                 ORDER BY total_time DESC LIMIT 10",
                &[],
            ) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{}warning:{} could not query pg_stat_statements: {}", YELLOW, RESET, e);
                    return;
                }
            }
        }
    };

    if rows.is_empty() {
        println!("{}No query statistics collected yet. Run some queries first.{}", DIM, RESET);
        return;
    }

    println!("{}Top 10 Queries by Total Execution Time{}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    println!(
        "{:<8} {:>10} {:>10} {:>10}  {}",
        "Calls", "Total(ms)", "Mean(ms)", "Stddev", "Query"
    );
    println!("{}", "─".repeat(90));

    for row in &rows {
        let query: String = row.get(0);
        let calls: i64 = row.get(1);
        let total: f64 = row.get(2);
        let mean: f64 = row.get(3);
        let stddev: f64 = row.get(4);
        let q_short = truncate_query(&query, 50);
        let color = if total > 10_000.0 { RED } else if total > 1_000.0 { YELLOW } else { "" };
        println!(
            "{}{:<8} {:>10.1} {:>10.2} {:>10.2}  {}{}",
            color, calls, total, mean, stddev, q_short, RESET
        );
    }
    println!();
}

fn check_unstable_queries(client: &mut Client) -> Vec<PgFinding> {
    // Coefficient of variation > 1.0 AND at least 20 calls = unstable plan.
    let sql = "SELECT query, calls, mean_exec_time, stddev_exec_time,
                      stddev_exec_time / NULLIF(mean_exec_time, 0) AS cv
               FROM pg_stat_statements
               WHERE calls >= 20
                 AND mean_exec_time > 1.0
                 AND stddev_exec_time / NULLIF(mean_exec_time, 0) > 1.0
               ORDER BY cv DESC
               LIMIT 15";

    let rows = match client.query(sql, &[]) {
        Ok(r) => r,
        Err(_) => {
            // Pre-13 fallback
            match client.query(
                "SELECT query, calls, mean_time, stddev_time,
                        stddev_time / NULLIF(mean_time, 0) AS cv
                 FROM pg_stat_statements
                 WHERE calls >= 20 AND mean_time > 1.0
                   AND stddev_time / NULLIF(mean_time, 0) > 1.0
                 ORDER BY cv DESC LIMIT 15",
                &[],
            ) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{}warning:{} could not check unstable queries: {}", YELLOW, RESET, e);
                    return vec![];
                }
            }
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}G204 — Unstable Query Plans (high stddev/mean){}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let query: String = row.get(0);
        let calls: i64 = row.get(1);
        let mean: f64 = row.get(2);
        let cv: f64 = row.get(4);
        println!(
            "  {}● cv={:.1}x  mean={:.1}ms  calls={}{}",
            YELLOW, cv, mean, calls, RESET
        );
        println!("    {}{}{}", DIM, truncate_query(&query, 80), RESET);
    }
    println!();

    rows.iter()
        .map(|row| {
            let query: String = row.get(0);
            let calls: i64 = row.get(1);
            let mean: f64 = row.get(2);
            let cv: f64 = row.get(4);
            PgFinding {
                code: "G204".to_string(),
                severity: "warning".to_string(),
                table_name: None,
                column_name: None,
                message: format!(
                    "Unstable query plan (cv={:.1}x, mean={:.1}ms, {} calls) — {}",
                    cv, mean, calls,
                    truncate_query(&query, 60)
                ),
                suggestion: Some(
                    "Run EXPLAIN (ANALYZE, BUFFERS) on this query. Check for parameter sniffing, \
                     missing statistics (ANALYZE <table>), or plan cache issues."
                        .to_string(),
                ),
            }
        })
        .collect()
}

fn check_high_row_ratio(client: &mut Client) -> Vec<PgFinding> {
    // Over 10,000 rows returned per call on average = likely over-fetching.
    let sql = "SELECT query, calls, rows,
                      rows / NULLIF(calls, 0) AS rows_per_call,
                      total_exec_time
               FROM pg_stat_statements
               WHERE calls >= 5
                 AND rows / NULLIF(calls, 0) > 10000
               ORDER BY rows_per_call DESC
               LIMIT 15";

    let rows = match client.query(sql, &[]) {
        Ok(r) => r,
        Err(_) => {
            match client.query(
                "SELECT query, calls, rows,
                        rows / NULLIF(calls, 0) AS rows_per_call, total_time
                 FROM pg_stat_statements
                 WHERE calls >= 5 AND rows / NULLIF(calls, 0) > 10000
                 ORDER BY rows_per_call DESC LIMIT 15",
                &[],
            ) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("{}warning:{} could not check row ratios: {}", YELLOW, RESET, e);
                    return vec![];
                }
            }
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}G202 — High Rows/Call Ratio (potential over-fetching){}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let query: String = row.get(0);
        let calls: i64 = row.get(1);
        let rpc: i64 = row.get(3);
        println!(
            "  {}● ~{} rows/call  {} calls{}",
            YELLOW, fmt_number(rpc), calls, RESET
        );
        println!("    {}{}{}", DIM, truncate_query(&query, 80), RESET);
    }
    println!();

    rows.iter()
        .map(|row| {
            let query: String = row.get(0);
            let calls: i64 = row.get(1);
            let rpc: i64 = row.get(3);
            PgFinding {
                code: "G202".to_string(),
                severity: "warning".to_string(),
                table_name: None,
                column_name: None,
                message: format!(
                    "Query returns ~{} rows per call avg ({} calls) — possible .all() or missing LIMIT: {}",
                    fmt_number(rpc), calls,
                    truncate_query(&query, 60)
                ),
                suggestion: Some(
                    "Add pagination (LIMIT/OFFSET), use .only() to narrow fields, \
                     or confirm this volume is intentional."
                        .to_string(),
                ),
            }
        })
        .collect()
}

fn cross_reference_fingerprints(
    client: &mut Client,
    gangstarr_db: &str,
    findings: &mut Vec<PgFinding>,
) {
    // Load normalized SQL fingerprints from gangstarr's SQLite DB.
    let conn = match storage::ensure_db(gangstarr_db) {
        Ok(c) => c,
        Err(_) => return, // No gangstarr DB yet — skip silently.
    };

    let stored: Vec<(String, String, i64)> = {
        let mut stmt = match conn.prepare(
            "SELECT fingerprint, normalized_sql, SUM(count) AS total_count
             FROM query_fingerprints
             GROUP BY fingerprint
             ORDER BY total_count DESC
             LIMIT 100",
        ) {
            Ok(s) => s,
            Err(_) => return,
        };
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        });
        match rows {
            Ok(r) => r.filter_map(|v| v.ok()).collect(),
            Err(_) => return,
        }
    };

    if stored.is_empty() {
        return;
    }

    // Fetch pg_stat_statements for cross-reference.
    let pg_stats = match client.query(
        "SELECT query, calls, total_exec_time, mean_exec_time
         FROM pg_stat_statements
         ORDER BY total_exec_time DESC
         LIMIT 200",
        &[],
    ) {
        Ok(r) => r,
        Err(_) => return,
    };

    let mut matched = 0usize;
    for pg_row in &pg_stats {
        let pg_query: String = pg_row.get(0);
        let pg_calls: i64 = pg_row.get(1);
        let _pg_total: f64 = pg_row.get(2);
        let pg_mean: f64 = pg_row.get(3);

        // Simple heuristic match: check if any stored normalized SQL shares
        // tables with the pg query. A full fingerprint match would require
        // normalizing the pg query, which needs pg_query crate here.
        // For now: match on first 40 chars of normalized form.
        for (fp, norm_sql, count) in &stored {
            let norm_prefix = &norm_sql[..norm_sql.len().min(40)];
            // Strip parameters for rough comparison
            let pg_prefix: String = pg_query.chars().take(40).collect();
            if pg_prefix.to_lowercase().contains(&norm_prefix[..norm_prefix.len().min(20)].to_lowercase()) {
                findings.push(PgFinding {
                    code: "G201".to_string(),
                    severity: "info".to_string(),
                    table_name: None,
                    column_name: Some(fp.clone()),
                    message: format!(
                        "Gangstarr fingerprint {} matched pg_stat_statements: {} calls \
                         in-app, {:.1}ms mean in PG (gangstarr saw {}x)",
                        &fp[..8.min(fp.len())], pg_calls, pg_mean, count
                    ),
                    suggestion: None,
                });
                matched += 1;
                break;
            }
        }
        if matched >= 5 {
            break;
        }
    }

    if matched > 0 {
        println!(
            "{}Cross-reference:{} matched {} gangstarr runtime fingerprint(s) to pg_stat_statements.",
            BOLD, RESET, matched
        );
        println!();
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn truncate_query(q: &str, max: usize) -> String {
    let cleaned: String = q.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.len() > max {
        format!("{}…", &cleaned[..max])
    } else {
        cleaned
    }
}

fn fmt_number(n: i64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}
