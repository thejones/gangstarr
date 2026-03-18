use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::correlate;
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

/// Walk upward from `start` to find the project root — the nearest directory
/// that contains `manage.py`, `pyproject.toml`, `setup.py`, or `.git`.
/// Falls back to `start` itself if no root marker is found.
fn find_project_root(start: &Path) -> std::path::PathBuf {
    // Resolve to an absolute path so parent traversal works correctly.
    let base = match start.canonicalize() {
        Ok(p) => p,
        Err(_) => start.to_path_buf(),
    };
    // Start from a directory (if given a file, use its parent).
    let dir = if base.is_dir() {
        base.clone()
    } else {
        base.parent().unwrap_or(&base).to_path_buf()
    };

    const MARKERS: &[&str] = &["manage.py", "pyproject.toml", "setup.py", ".git"];

    let mut current = dir.as_path();
    loop {
        for marker in MARKERS {
            if current.join(marker).exists() {
                return current.to_path_buf();
            }
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    // No marker found — use the original starting directory.
    dir
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

            // Find the project root for .gangstarr/ placement.
            // Always walk upward from the scanned path so that
            // `gangstarr check mfr/apps/community` and
            // `gangstarr check mfr/apps` both land in the same root folder.
            let project_root = find_project_root(path);
            let output_dir = parse_flag(argv, "--output-dir").unwrap_or_else(|| {
                project_root.join(".gangstarr").to_string_lossy().into_owned()
            });

            // Merge --exclude flags with [tool.gangstarr] exclude from pyproject.toml.
            let mut excludes = parse_flags(argv, "--exclude");
            excludes.extend(read_project_excludes(&project_root));

            let findings = static_analysis::step_in_the_arena(path, &excludes);
            reporter::report(&findings, &output_dir);

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

            if let Ok(conn) = storage::ensure_db(&db_path) {
                let _ = storage::insert_run(&conn, &run_id, &created_at, "static", path_str);
                let _ = storage::insert_static_findings(&conn, &run_id, &findings_json);
            }

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
            let limit: usize = parse_flag(argv, "--limit")
                .and_then(|s| s.parse().ok())
                .unwrap_or(20);

            let project_root = find_project_root(path);
            let output_dir = project_root.join(".gangstarr").to_string_lossy().into_owned();
            let db_path = format!("{}/gangstarr.db", output_dir);

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

fn print_usage() {
    println!("gangstarr — Django ORM static analysis");
    println!();
    println!("USAGE:");
    println!("    gangstarr check <path>              Scan Python files for ORM anti-patterns");
    println!("    gangstarr history [path]             Show analysis run history");
    println!();
    println!("OPTIONS:");
    println!("    --output-dir <dir>                  Output directory (default: project root/.gangstarr)");
    println!("                                        Project root is auto-detected via manage.py /");
    println!("                                        pyproject.toml / .git walking up from <path>.");
    println!("    --exclude <pattern>                 Skip files/dirs matching pattern (repeatable).");
    println!("                                        Patterns match directory names exactly or");
    println!("                                        file names as a substring.  Leading/trailing");
    println!("                                        slashes are stripped, so '/tests/' = 'tests'.");
    println!("                                        Also reads [tool.gangstarr] exclude from");
    println!("                                        pyproject.toml in the scanned directory.");
    println!("    --limit N                           Max history rows to show (default: 20)");
    println!();
    println!("RULES:");
    println!("    G101  Possible N+1 — related field accessed in loop or query inside loop");
    println!("    G102  .all() without .only()/.values() — over-fetching fields");
    println!("    G103  Python-side filtering — use .filter() instead of list comprehension");
    println!("    G104  len(queryset) — use .count() for a SQL COUNT");
    println!("    G105  Queryset truthiness check — use .exists()");
    println!("    G106  Python-side aggregation — use .aggregate() or .annotate()");
    println!("    G107  .save() in a loop — use bulk_create() or bulk_update()");
    println!();
    println!("EXIT CODES:");
    println!("    0  No issues found");
    println!("    1  Issues found");
    println!("    2  Usage error");
}
