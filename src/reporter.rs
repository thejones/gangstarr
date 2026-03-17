use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::static_analysis::StaticFinding;

/// Print Ruff-style findings to stdout, print summary, and write JSON to
/// `{output_dir}/static/findings.json`.
pub fn report(findings: &[StaticFinding], output_dir: &str) {
    for f in findings {
        println!("{}:{}:{}  {}  {}", f.file, f.line, f.col, f.rule, f.message);
    }

    let file_set: HashSet<&str> = findings.iter().map(|f| f.file.as_str()).collect();

    println!();
    if findings.is_empty() {
        println!("All clear — no issues found.");
    } else {
        println!(
            "Found {} {} in {} {}",
            findings.len(),
            if findings.len() == 1 { "issue" } else { "issues" },
            file_set.len(),
            if file_set.len() == 1 { "file" } else { "files" },
        );
    }

    write_json(findings, output_dir);
}

fn write_json(findings: &[StaticFinding], output_dir: &str) {
    let static_dir = Path::new(output_dir).join("static");
    if let Err(e) = fs::create_dir_all(&static_dir) {
        eprintln!("warning: could not create output directory: {}", e);
        return;
    }

    let output_path = static_dir.join("findings.json");

    let payload = serde_json::json!({
        "total": findings.len(),
        "files": findings.iter().map(|f| f.file.as_str()).collect::<HashSet<_>>().len(),
        "findings": findings,
    });

    match serde_json::to_string_pretty(&payload) {
        Ok(json) => match fs::write(&output_path, json) {
            Ok(_) => eprintln!("  → {}", output_path.display()),
            Err(e) => eprintln!("warning: could not write findings.json: {}", e),
        },
        Err(e) => eprintln!("warning: could not serialize findings: {}", e),
    }
}
