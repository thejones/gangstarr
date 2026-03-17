use std::path::Path;

use crate::reporter;
use crate::static_analysis;

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

            let findings = static_analysis::step_in_the_arena(path);
            reporter::report(&findings, &output_dir);

            if findings.is_empty() { 0 } else { 1 }
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

fn parse_flag<'a>(argv: &'a [String], flag: &str) -> Option<String> {
    for i in 0..argv.len() {
        if argv[i] == flag {
            return argv.get(i + 1).cloned();
        }
    }
    None
}

fn print_usage() {
    println!("gangstarr — Django ORM static analysis");
    println!();
    println!("USAGE:");
    println!("    gangstarr check <path>              Scan Python files for ORM anti-patterns");
    println!();
    println!("OPTIONS:");
    println!("    --output-dir <dir>                  Output directory for findings.json");
    println!("                                        (default: <path>/.gangstarr)");
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
