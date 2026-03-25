use std::collections::HashSet;

use crate::static_analysis::StaticFinding;

/// Print Ruff-style findings to stdout and a summary line.
/// Findings are persisted to gangstarr.db by cli.rs; no JSON file is written.
pub fn report(findings: &[StaticFinding], _output_dir: &str) {
    for f in findings {
        println!("{}:{}:{}  {}  {}", f.file, f.line, f.col, f.rule, f.message);
    }

    let file_set: HashSet<&str> = findings.iter().map(|f| f.file.as_str()).collect();

    println!();
    if findings.is_empty() {
        println!("All clear — no issues found.");
    } else {
        println!(
            "Found {} {} in {} {}  (stored in .gangstarr/gangstarr.db)",
            findings.len(),
            if findings.len() == 1 { "issue" } else { "issues" },
            file_set.len(),
            if file_set.len() == 1 { "file" } else { "files" },
        );
    }
}
