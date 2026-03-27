/// JazzThing — `gangstarr pg-royalty --review`
///
/// Connects to a live Postgres database and performs a read-only schema audit,
/// looking for structural issues that degrade query performance.
///
/// Named after the Gang Starr song "Jazz Thing" — because good schema design
/// is an art form.
///
/// All SQL executed here is read-only (SELECT / catalog queries only).
use postgres::{Client, NoTls};

use crate::storage::PgFinding;

// ── ANSI colours ─────────────────────────────────────────────────────────────

const BOLD: &str = "\x1b[1m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";
const GREEN: &str = "\x1b[32m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const DOUBLE_LINE: &str = "══════════════════════════════════════════════════════════════════════════════";
const SINGLE_LINE: &str = "──────────────────────────────────────────────────────────────────────────────";

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run the schema review and return (findings, exit_code).
/// exit_code = 0 if clean, 1 if findings, 2 if connection failed.
pub fn run_review(db_url: &str) -> (Vec<PgFinding>, i32) {
    let mut client = match Client::connect(db_url, NoTls) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}error:{} could not connect to database: {}", BOLD, RESET, e);
            eprintln!("{}hint:{} Ensure the database is reachable and --db-url is correct.", DIM, RESET);
            return (vec![], 2);
        }
    };

    println!("{}{}{}", BOLD, DOUBLE_LINE, RESET);
    println!("{}SCHEMA REVIEW{}", BOLD, RESET);
    println!("{}{}", DIM, DOUBLE_LINE);
    println!("Read-only catalog analysis — no writes performed.{}", RESET);
    println!();

    let mut all_findings: Vec<PgFinding> = Vec::new();

    // 1. Table overview
    print_table_overview(&mut client);

    // 2. FK columns missing an index → G201
    let fk_findings = check_fk_indexes(&mut client);
    all_findings.extend(fk_findings);

    // 3. Tables without a primary key → G201 (structural)
    let pk_findings = check_missing_pks(&mut client);
    all_findings.extend(pk_findings);

    // 4. Unusually wide tables (>25 columns) → advisory
    let wide_findings = check_wide_tables(&mut client);
    all_findings.extend(wide_findings);

    // 5. Unused indexes → G203
    let unused_findings = check_unused_indexes(&mut client);
    all_findings.extend(unused_findings);

    // 6. Sequential scans on large tables → G205
    let seq_findings = check_seq_scans(&mut client);
    all_findings.extend(seq_findings);

    // 7. Table bloat (dead tuple ratio) → G206
    let bloat_findings = check_table_bloat(&mut client);
    all_findings.extend(bloat_findings);

    // 8. Cache miss rate → G207
    let cache_findings = check_cache_miss(&mut client);
    all_findings.extend(cache_findings);

    // ── Print summary ────────────────────────────────────────────────────────
    println!();
    println!("{}{}{}", BOLD, SINGLE_LINE, RESET);
    if all_findings.is_empty() {
        println!("{}✓  No schema issues found.{}", GREEN, RESET);
    } else {
        let errors = all_findings.iter().filter(|f| f.severity == "error").count();
        let warnings = all_findings.iter().filter(|f| f.severity == "warning").count();
        let infos = all_findings.iter().filter(|f| f.severity == "info").count();
        println!(
            "{}Found {} finding(s):{} {} error(s), {} warning(s), {} advisory",
            BOLD, all_findings.len(), RESET, errors, warnings, infos
        );
        println!();
        print_findings_table(&all_findings);
    }
    println!("{}{}{}", DIM, DOUBLE_LINE, RESET);

    let exit_code = if all_findings.is_empty() { 0 } else { 1 };
    (all_findings, exit_code)
}

// ── Checks ────────────────────────────────────────────────────────────────────

fn print_table_overview(client: &mut Client) {
    // Simple query that avoids pg_relation_size (can fail on tables the role
    // can't stat) and filters out internal/extension tables.
    let rows = match client.query(
        "SELECT schemaname, tablename,
                COALESCE(n_live_tup, 0) AS live_rows,
                COALESCE(pg_total_relation_size(
                    quote_ident(schemaname)||'.'||quote_ident(tablename)
                ), 0) AS table_bytes
         FROM pg_stat_user_tables
         WHERE schemaname NOT IN ('pg_catalog', 'information_schema')
           AND tablename NOT LIKE 'pg_%'
           AND tablename NOT LIKE 'sql_%'
         ORDER BY n_live_tup DESC NULLS LAST
         LIMIT 20",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not query pg_stat_user_tables: {}", YELLOW, RESET, e);
            return;
        }
    };

    if rows.is_empty() {
        println!("{}No user tables found (database may be empty or no statistics collected yet).{}", DIM, RESET);
        return;
    }

    println!("{}Tables (top 20 by estimated row count){}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    println!("{:<40} {:>12} {:>12}", "Table", "Est. Rows", "Size");
    println!("{}", "─".repeat(66));
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let live: i64 = row.get(2);
        let bytes: i64 = row.get::<_, i64>(3);
        let name = format!("{}.{}", schema, table);
        println!("{:<40} {:>12} {:>12}", name, fmt_number(live), fmt_bytes(bytes));
    }
    println!();
}

fn check_fk_indexes(client: &mut Client) -> Vec<PgFinding> {
    // Find FK columns that don't have an index starting on that column.
    let rows = match client.query(
        "WITH fk_cols AS (
             SELECT n.nspname        AS schema,
                    c.relname        AS table_name,
                    a.attname        AS column_name,
                    f.relname        AS references_table
             FROM pg_constraint ct
             JOIN pg_class c  ON c.oid = ct.conrelid
             JOIN pg_class f  ON f.oid = ct.confrelid
             JOIN pg_namespace n ON n.oid = c.relnamespace
             JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(ct.conkey)
             WHERE ct.contype = 'f'
               AND n.nspname NOT IN ('pg_catalog','information_schema')
         ),
         indexed_cols AS (
             SELECT n.nspname AS schema,
                    t.relname AS table_name,
                    a.attname AS column_name
             FROM pg_index i
             JOIN pg_class t  ON t.oid = i.indrelid
             JOIN pg_namespace n ON n.oid = t.relnamespace
             JOIN pg_attribute a ON a.attrelid = t.oid
                                AND a.attnum = i.indkey[0]
         )
         SELECT fk.schema, fk.table_name, fk.column_name, fk.references_table
         FROM fk_cols fk
         LEFT JOIN indexed_cols ic
               ON ic.schema = fk.schema
              AND ic.table_name = fk.table_name
              AND ic.column_name = fk.column_name
         WHERE ic.column_name IS NULL
         ORDER BY fk.table_name, fk.column_name",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check FK indexes: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        println!("{}✓  All FK columns have indexes.{}", GREEN, RESET);
        println!();
        return vec![];
    }

    println!("{}G201 — FK Columns Missing Indexes{}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let table: String = row.get(1);
        let col: String = row.get(2);
        let refs: String = row.get(3);
        println!(
            "  {}{}  {}.{}{}  →  references {}",
            RED, "●", table, col, RESET, refs
        );
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            let col: String = row.get(2);
            let refs: String = row.get(3);
            PgFinding {
                code: "G201".to_string(),
                severity: "warning".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: Some(col.clone()),
                message: format!(
                    "FK column `{}.{}` references `{}` but has no index — full table scan on every join",
                    table, col, refs
                ),
                suggestion: Some(format!(
                    "CREATE INDEX ON {}.{}({});",
                    schema, table, col
                )),
            }
        })
        .collect()
}

fn check_missing_pks(client: &mut Client) -> Vec<PgFinding> {
    let rows = match client.query(
        "SELECT n.nspname, c.relname
         FROM pg_class c
         JOIN pg_namespace n ON n.oid = c.relnamespace
         WHERE c.relkind = 'r'
           AND n.nspname NOT IN ('pg_catalog','information_schema')
           AND NOT EXISTS (
               SELECT 1 FROM pg_constraint ct
               WHERE ct.conrelid = c.oid AND ct.contype = 'p'
           )
         ORDER BY c.relname",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check primary keys: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        println!("{}✓  All tables have primary keys.{}", GREEN, RESET);
        println!();
        return vec![];
    }

    println!("{}G201 — Tables Without Primary Key{}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        println!("  {}●  {}.{}{}", RED, schema, table, RESET);
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            PgFinding {
                code: "G201".to_string(),
                severity: "error".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: None,
                message: format!(
                    "Table `{}.{}` has no primary key — updates and deletes require full scans",
                    schema, table
                ),
                suggestion: Some(
                    "Add a primary key: ALTER TABLE ... ADD PRIMARY KEY (...);".to_string(),
                ),
            }
        })
        .collect()
}

fn check_wide_tables(client: &mut Client) -> Vec<PgFinding> {
    // Exclude extension-created objects (e.g. pg_stat_statements) from results.
    let rows = match client.query(
        "SELECT table_schema, table_name, COUNT(*) AS col_count
         FROM information_schema.columns
         WHERE table_schema NOT IN ('pg_catalog','information_schema')
           AND table_name NOT LIKE 'pg_%'
         GROUP BY table_schema, table_name
         HAVING COUNT(*) > 25
         ORDER BY col_count DESC",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check wide tables: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}Advisory — Wide Tables (>25 columns){}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let count: i64 = row.get(2);
        println!(
            "  {}●  {}.{}  ({} columns){}",
            YELLOW, schema, table, count, RESET
        );
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            let count: i64 = row.get(2);
            PgFinding {
                code: "G201".to_string(),
                severity: "info".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: None,
                message: format!(
                    "Table `{}.{}` has {} columns — consider vertical partitioning or normalization",
                    schema, table, count
                ),
                suggestion: Some(
                    "Extract rarely-used column groups into a related table joined on the PK."
                        .to_string(),
                ),
            }
        })
        .collect()
}

fn check_unused_indexes(client: &mut Client) -> Vec<PgFinding> {
    // Only meaningful after statistics have been collected (idx_scan = 0 after
    // a stats reset is expected; filter to tables with at least some activity).
    let rows = match client.query(
        "SELECT n.nspname, t.relname, i.relname AS index_name,
                ix.idx_scan,
                pg_relation_size(i.oid) AS index_bytes
         FROM pg_stat_user_indexes ix
         JOIN pg_index pi    ON pi.indexrelid = ix.indexrelid
         JOIN pg_class i     ON i.oid = ix.indexrelid
         JOIN pg_class t     ON t.oid = ix.relid
         JOIN pg_namespace n ON n.oid = t.relnamespace
         JOIN pg_stat_user_tables st ON st.relid = t.oid
         WHERE ix.idx_scan = 0
           AND NOT pi.indisprimary
           AND NOT pi.indisunique
           AND st.seq_scan > 0
           AND pg_relation_size(i.oid) > 8192
         ORDER BY index_bytes DESC
         LIMIT 20",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check unused indexes: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}G203 — Unused Indexes{}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let idx: String = row.get(2);
        let bytes: i64 = row.get(4);
        println!(
            "  {}●  {}.{} — {}  ({}  unused){}",
            YELLOW, schema, table, idx, fmt_bytes(bytes), RESET
        );
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            let idx: String = row.get(2);
            let bytes: i64 = row.get(4);
            PgFinding {
                code: "G203".to_string(),
                severity: "info".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: Some(idx.clone()),
                message: format!(
                    "Index `{}` on `{}.{}` has never been scanned — wasting {} of write overhead",
                    idx, schema, table, fmt_bytes(bytes)
                ),
                suggestion: Some(format!("DROP INDEX IF EXISTS {};", idx)),
            }
        })
        .collect()
}

fn check_seq_scans(client: &mut Client) -> Vec<PgFinding> {
    // Large tables with lots of sequential scans = likely missing index.
    let rows = match client.query(
        "SELECT schemaname, relname, seq_scan, n_live_tup, seq_tup_read
         FROM pg_stat_user_tables
         WHERE seq_scan > 100
           AND n_live_tup > 10000
           AND relname NOT LIKE 'pg_%'
         ORDER BY seq_scan DESC
         LIMIT 15",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check sequential scans: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}G205 — Sequential Scans on Large Tables{}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let scans: i64 = row.get(2);
        let live: i64 = row.get(3);
        println!(
            "  {}●  {}.{}  {} seq scans, ~{} rows{}",
            YELLOW, schema, table, fmt_number(scans), fmt_number(live), RESET
        );
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            let scans: i64 = row.get(2);
            let live: i64 = row.get(3);
            PgFinding {
                code: "G205".to_string(),
                severity: "warning".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: None,
                message: format!(
                    "Table `{}.{}` has {} sequential scans with ~{} rows — likely missing an index",
                    schema, table, fmt_number(scans), fmt_number(live)
                ),
                suggestion: Some(
                    "Check query patterns hitting this table and add indexes for common WHERE/JOIN columns."
                        .to_string(),
                ),
            }
        })
        .collect()
}

fn check_table_bloat(client: &mut Client) -> Vec<PgFinding> {
    // High dead tuple ratio = needs VACUUM.
    let rows = match client.query(
        "SELECT schemaname, relname, n_live_tup, n_dead_tup,
                CASE WHEN n_live_tup > 0
                     THEN n_dead_tup::float / n_live_tup
                     ELSE 0
                END AS dead_ratio
         FROM pg_stat_user_tables
         WHERE n_live_tup > 1000
           AND n_dead_tup::float / NULLIF(n_live_tup, 0) > 0.2
           AND relname NOT LIKE 'pg_%'
         ORDER BY dead_ratio DESC
         LIMIT 15",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check table bloat: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}G206 — Table Bloat (Dead Tuples){}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let live: i64 = row.get(2);
        let dead: i64 = row.get(3);
        let ratio: f64 = row.get(4);
        println!(
            "  {}●  {}.{}  {:.0}% dead ({} dead / {} live){}",
            YELLOW, schema, table, ratio * 100.0, fmt_number(dead), fmt_number(live), RESET
        );
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            let dead: i64 = row.get(3);
            let ratio: f64 = row.get(4);
            PgFinding {
                code: "G206".to_string(),
                severity: "warning".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: None,
                message: format!(
                    "Table `{}.{}` has {:.0}% dead tuples ({} dead rows) — needs VACUUM",
                    schema, table, ratio * 100.0, fmt_number(dead)
                ),
                suggestion: Some(
                    "Run VACUUM ANALYZE on this table, or check autovacuum settings."
                        .to_string(),
                ),
            }
        })
        .collect()
}

fn check_cache_miss(client: &mut Client) -> Vec<PgFinding> {
    // Tables with high cache miss rate (>10% reads from disk).
    let rows = match client.query(
        "SELECT schemaname, relname,
                heap_blks_read, heap_blks_hit,
                CASE WHEN (heap_blks_hit + heap_blks_read) > 0
                     THEN heap_blks_read::float / (heap_blks_hit + heap_blks_read)
                     ELSE 0
                END AS miss_rate
         FROM pg_statio_user_tables
         WHERE (heap_blks_hit + heap_blks_read) > 1000
           AND heap_blks_read::float / NULLIF(heap_blks_hit + heap_blks_read, 0) > 0.1
           AND relname NOT LIKE 'pg_%'
         ORDER BY miss_rate DESC
         LIMIT 15",
        &[],
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}warning:{} could not check cache miss rate: {}", YELLOW, RESET, e);
            return vec![];
        }
    };

    if rows.is_empty() {
        return vec![];
    }

    println!("{}G207 — Cache Miss Rate{}", BOLD, RESET);
    println!("{}", SINGLE_LINE);
    for row in &rows {
        let schema: String = row.get(0);
        let table: String = row.get(1);
        let reads: i64 = row.get(2);
        let hits: i64 = row.get(3);
        let miss: f64 = row.get(4);
        println!(
            "  {}●  {}.{}  {:.1}% miss ({} disk reads, {} cache hits){}",
            YELLOW, schema, table, miss * 100.0, fmt_number(reads), fmt_number(hits), RESET
        );
    }
    println!();

    rows.iter()
        .map(|row| {
            let schema: String = row.get(0);
            let table: String = row.get(1);
            let miss: f64 = row.get(4);
            PgFinding {
                code: "G207".to_string(),
                severity: "warning".to_string(),
                table_name: Some(format!("{}.{}", schema, table)),
                column_name: None,
                message: format!(
                    "Table `{}.{}` has {:.1}% cache miss rate — not fitting in shared_buffers",
                    schema, table, miss * 100.0
                ),
                suggestion: Some(
                    "Increase shared_buffers, add .only() to narrow fetched fields, or review query patterns."
                        .to_string(),
                ),
            }
        })
        .collect()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

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

fn fmt_bytes(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

fn print_findings_table(findings: &[PgFinding]) {
    println!(
        "{:<6} {:<8} {:<35} {:<25} {}",
        "Code", "Severity", "Table", "Column", "Message"
    );
    println!("{}", "─".repeat(120));
    for f in findings {
        let color = match f.severity.as_str() {
            "error" => RED,
            "warning" => YELLOW,
            _ => DIM,
        };
        let table = f.table_name.as_deref().unwrap_or("—");
        let col = f.column_name.as_deref().unwrap_or("—");
        let msg_short = if f.message.len() > 60 {
            let end = f.message.floor_char_boundary(59);
            format!("{}…", &f.message[..end])
        } else {
            f.message.clone()
        };
        println!(
            "{}{:<6} {:<8} {:<35} {:<25} {}{}",
            color, f.code, f.severity, table, col, msg_short, RESET
        );
        if let Some(sug) = &f.suggestion {
            println!("       {}→  {}{}", DIM, sug, RESET);
        }
    }
}
