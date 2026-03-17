use std::collections::HashMap;

use crate::fingerprint::fingerprint;
use crate::models::{CallsiteStats, Callsite, GroupedQuery, QueryEvent};
use crate::normalize::normalize;

/// Group a list of query events by their SQL fingerprint.
///
/// Returns a vec of GroupedQuery sorted by count descending.
pub fn group_by_fingerprint(events: &[QueryEvent]) -> Vec<GroupedQuery> {
    let mut groups: HashMap<String, GroupBuilder> = HashMap::new();

    for event in events {
        let fp = fingerprint(&event.sql);
        let entry = groups.entry(fp.clone()).or_insert_with(|| GroupBuilder {
            fingerprint: fp,
            normalized_sql: normalize(&event.sql),
            sample_sql: event.sql.clone(),
            count: 0,
            total_duration_ms: 0.0,
            durations: Vec::new(),
            callsite_counts: HashMap::new(),
        });

        entry.count += 1;
        entry.total_duration_ms += event.duration_ms;
        entry.durations.push(event.duration_ms);

        let callsite = Callsite {
            file: event.file.clone(),
            line: event.line,
            function: event.function.clone(),
            source: event.source.clone(),
            resolver_path: event.resolver_path.clone(),
        };

        let cs_entry = entry.callsite_counts.entry(callsite).or_insert((0, 0.0));
        cs_entry.0 += 1;
        cs_entry.1 += event.duration_ms;
    }

    let mut result: Vec<GroupedQuery> = groups
        .into_values()
        .map(|mut b| {
            let avg = if b.count > 0 {
                b.total_duration_ms / b.count as f64
            } else {
                0.0
            };

            b.durations.sort_by(|a, c| a.partial_cmp(c).unwrap_or(std::cmp::Ordering::Equal));
            let min = b.durations.first().copied().unwrap_or(0.0);
            let max = b.durations.last().copied().unwrap_or(0.0);
            let p50 = if b.durations.is_empty() {
                0.0
            } else {
                b.durations[b.durations.len() / 2]
            };

            let mut callsites: Vec<CallsiteStats> = b
                .callsite_counts
                .into_iter()
                .map(|(cs, (count, dur))| CallsiteStats {
                    file: cs.file,
                    line: cs.line,
                    function: cs.function,
                    source: cs.source,
                    resolver_path: cs.resolver_path,
                    count,
                    total_duration_ms: dur,
                })
                .collect();
            callsites.sort_by(|a, b| b.count.cmp(&a.count));

            GroupedQuery {
                fingerprint: b.fingerprint,
                normalized_sql: b.normalized_sql,
                count: b.count,
                total_duration_ms: b.total_duration_ms,
                avg_duration_ms: avg,
                min_duration_ms: min,
                max_duration_ms: max,
                p50_duration_ms: p50,
                sample_sql: b.sample_sql,
                callsites,
            }
        })
        .collect();

    result.sort_by(|a, b| b.count.cmp(&a.count));
    result
}

struct GroupBuilder {
    fingerprint: String,
    normalized_sql: String,
    sample_sql: String,
    count: usize,
    total_duration_ms: f64,
    durations: Vec<f64>,
    callsite_counts: HashMap<Callsite, (usize, f64)>,
}
