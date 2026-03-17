/// AboveTheClouds — cross-references static and runtime findings to produce
/// higher-confidence, actionable recommendations.
///
/// Named after the Gang Starr song "Above the Clouds" — it sees the bigger
/// picture by combining static code analysis with live runtime evidence.
use rusqlite::Result;
use serde_json::Value;

use crate::storage;

/// Cross-reference the static findings recorded for `run_id` against all
/// available runtime data in the database.
///
/// Returns a (possibly empty) list of correlated findings.  Each entry
/// describes a static finding that was confirmed or escalated by runtime data:
///
/// - `kind: "n1_confirmed"` — G101 static finding + runtime repeated-query
///   evidence at the same callsite. Severity is escalated to "error" when
///   the runtime count reaches the threshold.
///
/// - `kind: "runtime_confirmed"` — any other static finding where the same
///   callsite showed high query activity at runtime.
///
/// - `kind: "field_narrowing"` — G102 static finding (`.all()` without
///   `.only()`) in a file where field_usage data indicates only a subset of
///   fields are ever serialized. Suggests a concrete `.only()` call.
pub fn correlate_run(db_path: &str, run_id: &str) -> Result<Vec<Value>> {
    let conn = storage::ensure_db(db_path)?;

    let static_findings = storage::fetch_static_findings(&conn, run_id)?;
    let field_usage = storage::fetch_field_usage_by_model(&conn)?;

    let mut correlations: Vec<Value> = Vec::new();

    for sf in &static_findings {
        let file = sf["file"].as_str().unwrap_or("");
        let line = sf["line"].as_i64().unwrap_or(0);
        let rule = sf["rule"].as_str().unwrap_or("");
        let orig_sev = sf["severity"].as_str().unwrap_or("warning");

        // ── Runtime callsite evidence ─────────────────────────────────────
        let runtime_matches = storage::fetch_runtime_at_callsite(&conn, file, line)?;

        if let Some(best) = runtime_matches.first() {
            let count = best["runtime_count"].as_i64().unwrap_or(0);
            let dur_ms = best["runtime_duration_ms"].as_f64().unwrap_or(0.0);

            if count > 0 {
                let (kind, escalated_sev, msg) = match rule {
                    "G101" => {
                        let sev = if count >= 10 { "error" } else { orig_sev };
                        let msg = format!(
                            "N+1 confirmed at runtime: {}x queries from {}:{} ({:.1}ms total)",
                            count, file, line, dur_ms
                        );
                        ("n1_confirmed", sev, msg)
                    }
                    _ => {
                        let msg = format!(
                            "Runtime confirms {}:{} — {} queries observed ({:.1}ms)",
                            file, line, count, dur_ms
                        );
                        ("runtime_confirmed", orig_sev, msg)
                    }
                };

                correlations.push(serde_json::json!({
                    "kind":               kind,
                    "static_rule":        rule,
                    "file":               file,
                    "line":               line,
                    "runtime_count":      count,
                    "runtime_duration_ms": dur_ms,
                    "original_severity":  orig_sev,
                    "escalated_severity": escalated_sev,
                    "escalated":          escalated_sev != orig_sev,
                    "message":            msg,
                    "suggestion":         sf["suggestion"],
                }));
            }
        }

        // ── Field narrowing hint (G102 only) ─────────────────────────────
        if rule == "G102" {
            // Extract the model name heuristic: the filename without extension
            // (e.g. api_views.py → try to find any field_usage from that context).
            // We can't know the exact model without schema introspection, so we
            // surface ALL field_usage data as a reference alongside this finding.
            if !field_usage.is_empty() {
                let models: Vec<Value> = field_usage
                    .iter()
                    .map(|fu| {
                        let fields = fu["fields"]
                            .as_array()
                            .map(|a| {
                                a.iter()
                                    .filter_map(|v| v.as_str())
                                    .collect::<Vec<_>>()
                                    .join("', '")
                            })
                            .unwrap_or_default();
                        serde_json::json!({
                            "model":      fu["model"],
                            "fields":     fu["fields"],
                            "only_hint":  format!(".only('{}')", fields),
                        })
                    })
                    .collect();

                correlations.push(serde_json::json!({
                    "kind":              "field_narrowing",
                    "static_rule":       rule,
                    "file":              file,
                    "line":              line,
                    "original_severity": orig_sev,
                    "escalated_severity": orig_sev,
                    "escalated":         false,
                    "message":           format!(
                        "Field usage data available — consider .only() at {}:{}",
                        file, line
                    ),
                    "field_usage":       models,
                    "suggestion":        "Match the queryset fields to what the serializer actually returns",
                }));
            }
        }
    }

    Ok(correlations)
}

/// Produce a human-readable summary of correlations for CLI display.
pub fn format_correlations(correlations: &[Value]) -> String {
    if correlations.is_empty() {
        return String::new();
    }

    let escalations: Vec<&Value> = correlations
        .iter()
        .filter(|c| c["escalated"].as_bool().unwrap_or(false))
        .collect();
    let confirmations: Vec<&Value> = correlations
        .iter()
        .filter(|c| !c["escalated"].as_bool().unwrap_or(false))
        .collect();

    let mut lines: Vec<String> = Vec::new();
    lines.push("\n── Cross-reference (static × runtime) ──────────────────────────────────".to_string());

    for c in &escalations {
        let rule = c["static_rule"].as_str().unwrap_or("?");
        let file = c["file"].as_str().unwrap_or("?");
        let line = c["line"].as_i64().unwrap_or(0);
        let orig = c["original_severity"].as_str().unwrap_or("?");
        let esc = c["escalated_severity"].as_str().unwrap_or("?");
        let msg = c["message"].as_str().unwrap_or("");
        lines.push(format!(
            "  \x1b[31m↑ ESCALATED\x1b[0m  {}:{}:0  {}  {} → {}",
            file, line, rule, orig, esc
        ));
        lines.push(format!("             {}", msg));
    }

    for c in confirmations.iter().take(5) {
        let rule = c["static_rule"].as_str().unwrap_or("?");
        let file = c["file"].as_str().unwrap_or("?");
        let line = c["line"].as_i64().unwrap_or(0);
        let msg = c["message"].as_str().unwrap_or("");
        lines.push(format!("  ✓ confirmed  {}:{}:0  {}  {}", file, line, rule, msg));
    }

    if !escalations.is_empty() || !confirmations.is_empty() {
        let total = correlations.len();
        let esc_count = escalations.len();
        lines.push(format!(
            "\n  {} correlation(s): {} escalation(s)",
            total, esc_count
        ));
    }

    lines.join("\n")
}
