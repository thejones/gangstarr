use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::correlate;
use crate::pg_royalty;
use crate::reporter;
use crate::static_analysis;
use crate::storage;

// ── pyproject.toml config ───────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Default)]
struct GangstarrConfig {
    exclude: Option<Vec<String>>,
}

#[derive(serde::Deserialize, Default)]
struct ToolSection {
    gangstarr: Option<GangstarrConfig>,
}

#[derive(serde::Deserialize, Default)]
struct Pyproject {
    tool: Option<ToolSection>,
}

/// Read `[tool.gangstarr]` `exclude` from `pyproject.toml` in `dir`, if present.
fn read_project_excludes(dir: &Path) -> Vec<String> {
    let toml_path = dir.join("pyproject.toml");
    let content = match std::fs::read_to_string(&toml_path) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let cfg: Pyproject = toml::from_str(&content).unwrap_or_default();
    cfg.tool
        .and_then(|t| t.gangstarr)
        .and_then(|g| g.exclude)
        .unwrap_or_default()
}

/// Convert Unix milliseconds to an ISO 8601 UTC string.
/// Uses Howard Hinnant's civil-calendar algorithm — no external crates needed.
fn millis_to_iso(millis: u128) -> String {
    let secs = (millis / 1000) as u64;
    let ms = millis % 1000;
    let days = secs / 86_400;
    let time = secs % 86_400;
    let h = time / 3_600;
    let mi = (time % 3_600) / 60;
    let s = time % 60;

    // Civil calendar: Howard Hinnant's algorithm (epoch-era decomposition).
    let z = days as i64 + 719_468;
    let era = z / 146_097; // always ≥ 0 for post-1970 dates
    let doe = (z - era * 146_097) as u64; // day of era [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // year of era [0, 399]
    let y_base = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of year (Mar-based)
    let mp = (5 * doy + 2) / 153; // month (Mar=0 … Feb=11)
    let d = doy - (153 * mp + 2) / 5 + 1; // day [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // month [1, 12]
    let y = if m <= 2 { y_base + 1 } else { y_base };

    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z", y, m, d, h, mi, s, ms)
}

/// Run the gangstarr CLI. Returns a POSIX exit code (0 = clean, 1 = findings found, 2 = usage error).
pub fn run_check(argv: &[String]) -> i32 {
    // argv[0] = script name ("gangstarr")
    // argv[1] = subcommand ("check")
    // argv[2] = path
    // argv[3+] = optional flags

    let subcommand = argv.get(1).map(String::as_str).unwrap_or("help");

    match subcommand {
        "check" => {
            let path_str = match argv.get(2) {
                Some(p) if !p.starts_with('-') => p,
                _ => {
                    eprintln!("Usage: gangstarr check <path> [--output-dir <dir>]");
                    return 2;
                }
            };

            let path = Path::new(path_str);
            if !path.exists() {
                eprintln!("error: path '{}' does not exist", path_str);
                return 2;
            }

            let output_dir = parse_flag(argv, "--output-dir").unwrap_or_else(|| {
                if path.is_dir() {
                    path.join(".gangstarr").to_string_lossy().into_owned()
                } else {
                    // Single file: put .gangstarr next to it
                    path.parent()
                        .unwrap_or(Path::new("."))
                        .join(".gangstarr")
                        .to_string_lossy()
                        .into_owned()
                }
            });

            // Merge --exclude flags with [tool.gangstarr] exclude from pyproject.toml.
            let mut excludes = parse_flags(argv, "--exclude");
            let project_root = if path.is_dir() { path } else { path.parent().unwrap_or(Path::new(".")) };
            excludes.extend(read_project_excludes(project_root));

            let findings = static_analysis::step_in_the_arena(path, &excludes);

            // ── Persist to SQLite ─────────────────────────────────────────
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            let run_id = format!("{:016x}", millis);
            let created_at = millis_to_iso(millis);
            let db_path = format!("{}/gangstarr.db", output_dir);

            let findings_json: Vec<serde_json::Value> = findings
                .iter()
                .filter_map(|f| serde_json::to_value(f).ok())
                .collect();

            let db_stored = if let Ok(conn) = storage::ensure_db(&db_path) {
                let _ = storage::insert_run(&conn, &run_id, &created_at, "static", path_str);
                let _ = storage::insert_static_findings(&conn, &run_id, &findings_json);
                true
            } else {
                eprintln!("warning: could not create {}", db_path);
                false
            };

            reporter::report(&findings, db_stored);

            // ── Cross-reference with any existing runtime evidence ────────
            if let Ok(correlations) = correlate::correlate_run(&db_path, &run_id) {
                let summary = correlate::format_correlations(&correlations);
                if !summary.is_empty() {
                    println!("{}", summary);
                }
            }

            if findings.is_empty() { 0 } else { 1 }
        }
        "history" => {
            let path_str = argv
                .get(2)
                .map(String::as_str)
                .filter(|p| !p.starts_with('-'))
                .unwrap_or(".");

            let path = Path::new(path_str);
            let limit: usize = parse_flag(argv, "--count")
                .or_else(|| parse_flag(argv, "--limit"))
                .and_then(|s| s.parse().ok())
                .unwrap_or(20);

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

            // --findings: unified per-finding view across all sources
            if argv.iter().any(|a| a == "--findings") {
                match storage::fetch_all_findings(&db_path, limit) {
                    Ok(findings) if findings.is_empty() => {
                        println!("No findings recorded yet.");
                        0
                    }
                    Ok(findings) => {
                        print_findings_list(&findings);
                        0
                    }
                    Err(e) => {
                        eprintln!("error: could not read findings: {}", e);
                        2
                    }
                }
            } else {
                match storage::fetch_run_history(&db_path, limit) {
                    Ok(runs) if runs.is_empty() => {
                        println!("No runs recorded. Run `gangstarr check <path>` first.");
                        0
                    }
                    Ok(runs) => {
                        print_history_table(&runs);
                        0
                    }
                    Err(e) => {
                        eprintln!("error: could not read history: {}", e);
                        2
                    }
                }
            }
        }
        "pg-royalty" => {
            pg_royalty::run(argv)
        }
        "help" | "--help" | "-h" => {
            print_usage();
            0
        }
        other => {
            eprintln!("error: unknown subcommand '{}'", other);
            eprintln!("Run `gangstarr --help` for usage.");
            2
        }
    }
}

fn parse_flag(argv: &[String], flag: &str) -> Option<String> {
    for i in 0..argv.len() {
        if argv[i] == flag {
            return argv.get(i + 1).cloned();
        }
    }
    None
}

/// Collect every value associated with a repeatable flag, e.g.
/// `--exclude tests --exclude test_` → `["tests", "test_"]`.
fn parse_flags(argv: &[String], flag: &str) -> Vec<String> {
    let mut results = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == flag {
            if let Some(val) = argv.get(i + 1) {
                results.push(val.clone());
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    results
}

fn print_history_table(runs: &[serde_json::Value]) {
    println!(
        "{:<18}  {:<8}  {:<26}  {:>7}  {:>8}",
        "Run ID", "Type", "When", "Static", "Runtime"
    );
    println!("{}", "─".repeat(75));
    for run in runs {
        let run_id = run["run_id"].as_str().unwrap_or("?");
        let run_type = run["run_type"].as_str().unwrap_or("?");
        let created_at = run["created_at"].as_str().unwrap_or("?");
        let static_count = run["static_count"].as_i64().unwrap_or(0);
        let runtime_count = run["runtime_count"].as_i64().unwrap_or(0);
        println!(
            "{:<18}  {:<8}  {:<26}  {:>7}  {:>8}",
            run_id, run_type, created_at, static_count, runtime_count
        );
    }
}

fn print_findings_list(findings: &[serde_json::Value]) {
    println!(
        "{:<8} {:<8} {:<10} {:<28} {}",
        "Source", "Code", "Severity", "Location", "Message"
    );
    println!("{}", "─".repeat(100));
    for f in findings {
        let source = f["source"].as_str().unwrap_or("?");
        let code = f["code"].as_str().unwrap_or("?");
        let severity = f["severity"].as_str().unwrap_or("?");
        let message = f["message"].as_str().unwrap_or("?");

        let location = if let Some(file) = f["file"].as_str() {
            let line = f["line"].as_i64().unwrap_or(0);
            format!("{}:{}", file.rsplit('/').next().unwrap_or(file), line)
        } else if let Some(table) = f["table_name"].as_str() {
            let col = f["column_name"].as_str().unwrap_or("");
            if col.is_empty() { table.to_string() } else { format!("{}.{}", table, col) }
        } else {
            "—".to_string()
        };

        let color = match severity {
            "error" => "\x1b[31m",
            "warning" => "\x1b[33m",
            _ => "\x1b[2m",
        };
        let msg_short = if message.len() > 55 {
            format!("{}…", &message[..54])
        } else {
            message.to_string()
        };
        println!(
            "{}{:<8} {:<8} {:<10} {:<28} {}\x1b[0m",
            color, source, code, severity, location, msg_short
        );
    }
}

fn print_usage() {
    println!("gangstarr — Django ORM performance profiler");
    println!();
    println!("USAGE:");
    println!("    gangstarr check <path>              Scan Python files for ORM anti-patterns");
    println!("    gangstarr history [path]             Show analysis run history");
    println!("    gangstarr pg-royalty                 Analyze a live Postgres DB (see --help)");
    println!();
    println!("OPTIONS (check):");
    println!("    --output-dir <dir>                  Output directory (default: <path>/.gangstarr)");
    println!("    --exclude <pattern>                 Skip files/dirs matching pattern (repeatable)");
    println!();
    println!("OPTIONS (history):");
    println!("    --findings                          Show per-finding detail (all sources)");
    println!("    --count N / --limit N               Max rows to show (default: 20)");
    println!();
    println!("STATIC RULES:");
    println!("    G101  Possible N+1 — related field accessed in loop or query inside loop");
    println!("    G102  .all() without .only()/.values() — over-fetching fields");
    println!("    G103  Python-side filtering — use .filter() instead of list comprehension");
    println!("    G104  len(queryset) — use .count() for a SQL COUNT");
    println!("    G105  Queryset truthiness check — use .exists()");
    println!("    G106  Python-side aggregation — use .aggregate() or .annotate()");
    println!("    G107  .save() in a loop — use bulk_create() or bulk_update()");
    println!();
    println!("POSTGRES RULES (pg-royalty):");
    println!("    G201  Missing index / missing PK / wide table");
    println!("    G202  High rows/call ratio — possible .all() or missing LIMIT");
    println!("    G203  Unused index");
    println!("    G204  Unstable query plan — high stddev/mean execution time");
    println!();
    println!("EXIT CODES:");
    println!("    0  No issues found");
    println!("    1  Issues found");
    println!("    2  Usage error");
}
