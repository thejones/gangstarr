use std::collections::HashMap;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank, is_queryset_expr};

/// G109 — Queryset re-evaluation.
///
/// Flags when the same queryset variable is consumed in two different contexts
/// (e.g., `len(qs)` then `for x in qs:`), which causes Django to evaluate
/// the SQL query twice.
pub struct G109;

impl G109 {
    pub fn new() -> Self {
        G109
    }
}

/// Consuming operations that trigger queryset evaluation.
fn is_consuming_use(line: &str, var: &str) -> bool {
    let t = line.trim_start();
    // len(var)
    if t.contains(&format!("len({})", var)) {
        return true;
    }
    // for ... in var:
    if t.starts_with("for ") && t.contains(&format!(" in {}:", var)) {
        return true;
    }
    if t.starts_with("for ") && t.contains(&format!(" in {}.", var)) {
        return true;
    }
    // list(var)
    if t.contains(&format!("list({})", var)) {
        return true;
    }
    // if var:  (truthiness check)
    if (t.starts_with("if ") || t.starts_with("elif ")) && t.contains(&format!(" {}:", var)) {
        return true;
    }
    // var.count()
    if t.contains(&format!("{}.count()", var)) {
        return true;
    }
    // var.exists()
    if t.contains(&format!("{}.exists()", var)) {
        return true;
    }
    false
}

impl Rule for G109 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Track queryset variable assignments: var -> (line_no, first_use_line)
        let mut qs_vars: HashMap<String, (usize, Option<usize>)> = HashMap::new();

        for (i, &line) in lines.iter().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            let trimmed = line.trim_start();

            // Track queryset assignments: VAR = <queryset expr>
            if let Some(eq_pos) = trimmed.find(" = ") {
                let var = trimmed[..eq_pos].trim();
                let rhs = &trimmed[eq_pos + 3..];
                if !var.contains('.') && !var.contains('[') && is_queryset_expr(rhs) {
                    qs_vars.insert(var.to_string(), (i + 1, None));
                }
            }

            // Check for consuming uses of tracked queryset variables.
            for (var, (assign_line, first_use)) in qs_vars.iter_mut() {
                if is_consuming_use(trimmed, var) {
                    match first_use {
                        None => {
                            // First consuming use — just record it.
                            *first_use = Some(i + 1);
                        }
                        Some(prev_line) => {
                            // Second consuming use — flag it!
                            findings.push(StaticFinding {
                                rule: "G109".to_string(),
                                message: format!(
                                    "Queryset `{}` evaluated twice (first at line {}, again here) — causes duplicate SQL",
                                    var, prev_line
                                ),
                                severity: Severity::Warning,
                                file: file.to_string(),
                                line: i + 1,
                                col: indent_of(line),
                                suggestion: Some(format!(
                                    "Cache the result: `{}_list = list({})` at line {}, then reuse `{}_list`",
                                    var, var, assign_line, var
                                )),
                            });
                        }
                    }
                }
            }

            // Reset tracking when variable is reassigned.
            if let Some(eq_pos) = trimmed.find(" = ") {
                let var = trimmed[..eq_pos].trim().to_string();
                if qs_vars.contains_key(&var) && !is_queryset_expr(&trimmed[eq_pos + 3..]) {
                    qs_vars.remove(&var);
                }
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_double_evaluation() {
        let src = r#"
qs = Artist.objects.filter(active=True)
count = len(qs)
for item in qs:
    print(item)
"#;
        let findings = G109::new().check("views.py", src);
        assert!(!findings.is_empty(), "should flag double evaluation");
        assert_eq!(findings[0].rule, "G109");
    }

    #[test]
    fn test_no_flag_single_use() {
        let src = r#"
qs = Artist.objects.filter(active=True)
for item in qs:
    print(item)
"#;
        let findings = G109::new().check("views.py", src);
        assert!(findings.is_empty(), "single use should not flag");
    }
}
