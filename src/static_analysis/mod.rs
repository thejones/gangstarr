pub mod models;
mod rules;

pub use models::StaticFinding;

use std::path::Path;
use walkdir::WalkDir;

use rules::all_rules;

/// Walk every Python file under `path`, apply all rules, return sorted findings.
///
/// Named after the Gang Starr album "Step in the Arena".
pub fn step_in_the_arena(path: &Path) -> Vec<StaticFinding> {
    let rules = all_rules();
    let mut all_findings: Vec<StaticFinding> = Vec::new();

    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "py"))
    {
        let file_path = entry.path();

        if should_skip(file_path) {
            continue;
        }

        let source = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Strip the scanned root prefix so reported paths are relative.
        let display_path = file_path.strip_prefix(path).unwrap_or(file_path);
        let file_str = display_path.to_string_lossy().into_owned();

        for rule in &rules {
            let mut findings = rule.check(&file_str, &source);
            all_findings.append(&mut findings);
        }
    }

    // Sort by file then line for deterministic, Ruff-style output.
    all_findings.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    all_findings
}

/// Return true if the path component is a directory we always want to skip.
fn should_skip(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        matches!(
            s.as_ref(),
            "migrations"
                | ".venv"
                | "venv"
                | "__pycache__"
                | ".git"
                | "node_modules"
                | ".tox"
                | "build"
                | "dist"
        ) || (s.starts_with('.') && s.len() > 1 && s != "..")
    })
}
