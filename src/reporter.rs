use std::collections::HashSet;

use crate::static_analysis::StaticFinding;

/// Print Ruff-style findings to stdout and a summary line.
///
/// `db_stored` indicates whether the findings were persisted to the DB;
/// the summary line reflects this.
pub fn report(findings: &[StaticFinding], db_stored: bool) {
    for f in findings {
        println!("{}:{}:{}  {}  {}", f.file, f.line, f.col, f.rule, f.message);
    }

    let file_set: HashSet<&str> = findings.iter().map(|f| f.file.as_str()).collect();

    println!();
    if findings.is_empty() {
        println!("All clear — no issues found.");
    } else {
        let suffix = if db_stored {
            "  (stored in .gangstarr/gangstarr.db)"
        } else {
            ""
        };
        println!(
            "Found {} {} in {} {}{}",
            findings.len(),
            if findings.len() == 1 { "issue" } else { "issues" },
            file_set.len(),
            if file_set.len() == 1 { "file" } else { "files" },
            suffix,
        );
    }
}
