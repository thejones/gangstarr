use crate::models::{Finding, GroupedQuery, Severity};

/// Detect suspicious query patterns from grouped queries.
///
/// Returns a list of findings sorted by severity.
pub fn detect_patterns(groups: &[GroupedQuery]) -> Vec<Finding> {
    let mut findings = Vec::new();

    for group in groups {
        // G001: Duplicate queries — same fingerprint executed multiple times
        if group.count > 1 {
            let severity = if group.count >= 10 {
                Severity::Error
            } else if group.count >= 3 {
                Severity::Warning
            } else {
                Severity::Info
            };

            let top_callsite = group.callsites.first();
            findings.push(Finding {
                code: "G001".to_string(),
                title: "Duplicate queries".to_string(),
                severity,
                message: format!(
                    "Query executed {} times (total {:.1}ms)",
                    group.count, group.total_duration_ms
                ),
                fingerprint: Some(group.fingerprint.clone()),
                file: top_callsite.map(|cs| cs.file.clone()),
                line: top_callsite.map(|cs| cs.line),
                suggestion: None,
                resolver_path: top_callsite
                    .map(|cs| cs.resolver_path.clone())
                    .unwrap_or_default(),
            });
        }

        // G002: Likely N+1 — same query shape repeated from the same callsite
        for cs in &group.callsites {
            if cs.count > 1 {
                let severity = if cs.count >= 10 {
                    Severity::Error
                } else if cs.count >= 3 {
                    Severity::Warning
                } else {
                    Severity::Info
                };

                findings.push(Finding {
                    code: "G002".to_string(),
                    title: "Likely N+1 query pattern".to_string(),
                    severity,
                    message: format!(
                        "Query executed {} times from {}:{}",
                        cs.count, cs.file, cs.line
                    ),
                    fingerprint: Some(group.fingerprint.clone()),
                    file: Some(cs.file.clone()),
                    line: Some(cs.line),
                    suggestion: Some(
                        "Consider select_related() or prefetch_related()".to_string(),
                    ),
                    resolver_path: cs.resolver_path.clone(),
                });
            }
        }
    }

    // G003: Hot callsite — single source line issuing many total queries
    // Collect all callsite stats across all groups
    let mut callsite_totals: std::collections::HashMap<(String, u32), usize> =
        std::collections::HashMap::new();
    for group in groups {
        for cs in &group.callsites {
            *callsite_totals
                .entry((cs.file.clone(), cs.line))
                .or_insert(0) += cs.count;
        }
    }
    for ((file, line), total_count) in &callsite_totals {
        if *total_count >= 5 {
            findings.push(Finding {
                code: "G003".to_string(),
                title: "Hot callsite".to_string(),
                severity: if *total_count >= 20 {
                    Severity::Error
                } else {
                    Severity::Warning
                },
                message: format!(
                    "{}:{} issued {} total queries",
                    file, line, total_count
                ),
                fingerprint: None,
                file: Some(file.clone()),
                line: Some(*line),
                suggestion: Some(
                    "Review this code path for query optimization opportunities".to_string(),
                ),
                resolver_path: String::new(),
            });
        }
    }

    // Sort: errors first, then warnings, then info
    findings.sort_by_key(|f| match f.severity {
        Severity::Error => 0,
        Severity::Warning => 1,
        Severity::Info => 2,
    });

    findings
}
