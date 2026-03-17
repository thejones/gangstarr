use std::collections::HashMap;

use crate::models::{CallerFrame, ConsolidatedCallsite, GroupedQuery};

/// A builder for accumulating stats across fingerprint groups for a single callsite.
struct CallsiteBuilder {
    file: String,
    line: u32,
    function: String,
    resolver_path: String,
    total_queries: usize,
    dup_groups: usize,
    worst_repeat: usize,
    dup_duration_ms: f64,
    has_n_plus_1: bool,
    /// (count, normalized_sql) of the highest-repeat fingerprint group.
    top_group: (usize, String),
    caller_chain: Vec<CallerFrame>,
}

/// Consolidate grouped queries into one row per unique (file, line) callsite.
///
/// Each row aggregates all fingerprint groups that share a callsite, producing
/// a compact summary suitable for table display.
pub fn consolidate_by_callsite(groups: &[GroupedQuery]) -> Vec<ConsolidatedCallsite> {
    let mut builders: HashMap<(String, u32), CallsiteBuilder> = HashMap::new();

    for group in groups {
        for cs in &group.callsites {
            let key = (cs.file.clone(), cs.line);
            let entry = builders.entry(key).or_insert_with(|| CallsiteBuilder {
                file: cs.file.clone(),
                line: cs.line,
                function: cs.function.clone(),
                resolver_path: cs.resolver_path.clone(),
                total_queries: 0,
                dup_groups: 0,
                worst_repeat: 0,
                dup_duration_ms: 0.0,
                has_n_plus_1: false,
                top_group: (0, String::new()),
                caller_chain: Vec::new(),
            });

            entry.total_queries += cs.count;

            // Count this as a dup group if the query was repeated
            if cs.count > 1 {
                entry.dup_groups += 1;
                if cs.count > entry.worst_repeat {
                    entry.worst_repeat = cs.count;
                }
                entry.dup_duration_ms += cs.total_duration_ms;
                entry.has_n_plus_1 = true;
            }

            // Track the highest-repeat group's SQL for the top_sql field
            if cs.count > entry.top_group.0 {
                let sql = &group.normalized_sql;
                let truncated = if sql.len() > 120 {
                    format!("{}...", &sql[..120])
                } else {
                    sql.clone()
                };
                entry.top_group = (cs.count, truncated);
            }
        }
    }

    let mut result: Vec<ConsolidatedCallsite> = builders
        .into_values()
        .map(|b| {
            let mut flags = Vec::new();
            if b.total_queries >= 5 {
                flags.push("HOT".to_string());
            }
            if b.has_n_plus_1 {
                flags.push("N+1".to_string());
            }

            ConsolidatedCallsite {
                file: b.file,
                line: b.line,
                function: b.function,
                resolver_path: b.resolver_path,
                total_queries: b.total_queries,
                dup_groups: b.dup_groups,
                worst_repeat: b.worst_repeat,
                dup_duration_ms: b.dup_duration_ms,
                flags,
                top_sql: b.top_group.1,
                caller_chain: b.caller_chain,
            }
        })
        .collect();

    result.sort_by(|a, b| b.total_queries.cmp(&a.total_queries));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{CallsiteStats, GroupedQuery};

    fn make_group(fingerprint: &str, sql: &str, callsites: Vec<(&str, u32, usize, f64)>) -> GroupedQuery {
        let count: usize = callsites.iter().map(|c| c.2).sum();
        let total_dur: f64 = callsites.iter().map(|c| c.3).sum();
        GroupedQuery {
            fingerprint: fingerprint.to_string(),
            normalized_sql: sql.to_string(),
            count,
            total_duration_ms: total_dur,
            avg_duration_ms: if count > 0 { total_dur / count as f64 } else { 0.0 },
            min_duration_ms: 0.0,
            max_duration_ms: 0.0,
            p50_duration_ms: 0.0,
            sample_sql: sql.to_string(),
            callsites: callsites
                .into_iter()
                .map(|(file, line, cnt, dur)| CallsiteStats {
                    file: file.to_string(),
                    line,
                    function: "fn".to_string(),
                    source: "".to_string(),
                    resolver_path: String::new(),
                    count: cnt,
                    total_duration_ms: dur,
                })
                .collect(),
        }
    }

    #[test]
    fn test_single_callsite_single_group() {
        let groups = vec![make_group("fp1", "SELECT 1", vec![("foo.py", 10, 5, 10.0)])];
        let result = consolidate_by_callsite(&groups);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].total_queries, 5);
        assert_eq!(result[0].dup_groups, 1);
        assert_eq!(result[0].worst_repeat, 5);
        assert!(result[0].flags.contains(&"HOT".to_string()));
        assert!(result[0].flags.contains(&"N+1".to_string()));
    }

    #[test]
    fn test_single_callsite_multiple_groups() {
        // Same callsite triggers two different SQL fingerprints
        let groups = vec![
            make_group("fp1", "SELECT * FROM a", vec![("mixin.py", 38, 15, 14.4)]),
            make_group("fp2", "SELECT * FROM b", vec![("mixin.py", 38, 4, 2.9)]),
            make_group("fp3", "SELECT * FROM c", vec![("mixin.py", 38, 3, 1.1)]),
        ];
        let result = consolidate_by_callsite(&groups);
        assert_eq!(result.len(), 1);
        let row = &result[0];
        assert_eq!(row.total_queries, 22);
        assert_eq!(row.dup_groups, 3);
        assert_eq!(row.worst_repeat, 15);
        assert!(row.top_sql.starts_with("SELECT * FROM a"));
    }

    #[test]
    fn test_multiple_callsites_sorted_by_total() {
        let groups = vec![
            make_group("fp1", "SELECT 1", vec![
                ("a.py", 10, 3, 5.0),
                ("b.py", 20, 10, 50.0),
            ]),
        ];
        let result = consolidate_by_callsite(&groups);
        assert_eq!(result.len(), 2);
        // b.py has 10 queries, should be first
        assert_eq!(result[0].file, "b.py");
        assert_eq!(result[1].file, "a.py");
    }

    #[test]
    fn test_no_dupes_no_flags() {
        let groups = vec![make_group("fp1", "SELECT 1", vec![("foo.py", 10, 1, 1.0)])];
        let result = consolidate_by_callsite(&groups);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].dup_groups, 0);
        assert_eq!(result[0].worst_repeat, 0);
        assert!(result[0].flags.is_empty());
    }
}
