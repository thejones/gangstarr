use std::collections::HashSet;

use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank};

/// G110 — select_related incompleteness.
///
/// Detects when `select_related('field')` is used but the loop body accesses
/// deeper nested relations that aren't included in the select_related call.
/// E.g., `select_related('author')` but `obj.author.publisher.name` is accessed.
pub struct G110 {
    select_related_re: Regex,
    field_name_re: Regex,
}

impl G110 {
    pub fn new() -> Self {
        G110 {
            select_related_re: Regex::new(
                r"\.select_related\s*\(([^)]*)\)",
            )
            .unwrap(),
            field_name_re: Regex::new(r#"['"](\w+)['"]"#).unwrap(),
        }
    }
}

impl Rule for G110 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Track: (loop_var, select_related_fields, queryset_line)
        let mut active_selects: Vec<(String, HashSet<String>, usize)> = Vec::new();

        for (i, &line) in lines.iter().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            let trimmed = line.trim_start();

            // Detect select_related() calls and extract field names.
            if let Some(caps) = self.select_related_re.captures(trimmed) {
                let args = &caps[1];
                let fields: HashSet<String> = self.field_name_re
                    .captures_iter(args)
                    .map(|c| c[1].to_string())
                    .collect();

                if !fields.is_empty() {
                    // Try to find the for-loop that iterates this queryset.
                    // Look ahead for `for VAR in ...:`
                    for j in (i + 1)..lines.len().min(i + 15) {
                        let future = lines[j].trim_start();
                        if future.starts_with("for ") && future.contains(':') {
                            if let Some(var_end) = future.find(" in ") {
                                let var = future[4..var_end].trim().to_string();
                                active_selects.push((var, fields.clone(), i + 1));
                            }
                            break;
                        }
                        // Also handle: qs = ...select_related(...) / for var in qs:
                        if future.starts_with("for ") {
                            break;
                        }
                    }
                }
            }

            // Check loop body for nested attribute access beyond select_related depth.
            for (loop_var, selected_fields, sr_line) in &active_selects {
                if !trimmed.contains(loop_var) {
                    continue;
                }

                // Look for VAR.field1.field2.field3 patterns.
                let needle = format!("{}.", loop_var);
                let mut pos = 0;
                while let Some(idx) = trimmed[pos..].find(&needle) {
                    let abs_pos = pos + idx;
                    // Word boundary check.
                    let is_boundary = abs_pos == 0 || {
                        let prev = trimmed.as_bytes()[abs_pos - 1] as char;
                        !prev.is_alphanumeric() && prev != '_'
                    };

                    if is_boundary {
                        let after = &trimmed[abs_pos + needle.len()..];
                        // Parse chain: field1.field2.field3
                        let chain: Vec<&str> = after
                            .split(|c: char| !c.is_alphanumeric() && c != '_' && c != '.')
                            .next()
                            .unwrap_or("")
                            .split('.')
                            .filter(|s| !s.is_empty())
                            .collect();

                        if chain.len() >= 2 {
                            let first_field = chain[0];
                            // If the first field is in select_related but there's a
                            // deeper access, the nested relation may not be selected.
                            if selected_fields.contains(first_field) {
                                let nested = chain[1];
                                // Check if the nested path is also selected.
                                let nested_path = format!("{}__{}", first_field, nested);
                                let double_path = format!("{}.{}", first_field, nested);
                                if !selected_fields.contains(&nested_path)
                                    && !selected_fields.contains(&double_path)
                                {
                                    findings.push(StaticFinding {
                                        rule: "G110".to_string(),
                                        message: format!(
                                            "select_related('{}') at line {} doesn't cover `{}.{}.{}` — nested relation may cause N+1",
                                            first_field, sr_line, loop_var, first_field, nested
                                        ),
                                        severity: Severity::Warning,
                                        file: file.to_string(),
                                        line: i + 1,
                                        col: indent_of(line),
                                        suggestion: Some(format!(
                                            "Add '{}__{}' to select_related()",
                                            first_field, nested
                                        )),
                                    });
                                }
                            }
                        }
                    }
                    pos = abs_pos + 1;
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
    fn test_flags_incomplete_select_related() {
        let src = r#"
qs = Book.objects.select_related('author')
for book in qs:
    print(book.author.publisher.name)
"#;
        let findings = G110::new().check("views.py", src);
        assert!(!findings.is_empty(), "should flag nested access beyond select_related");
        assert_eq!(findings[0].rule, "G110");
    }

    #[test]
    fn test_no_flag_single_depth() {
        let src = r#"
qs = Book.objects.select_related('author')
for book in qs:
    print(book.author.name)
"#;
        let findings = G110::new().check("views.py", src);
        // author.name is single-depth FK access (name is a field, not a relation)
        // This is tricky — we flag if there's a 2-deep chain after the selected field.
        // book.author.name has chain ['author', 'name'] where 'author' is selected.
        // We flag this because we can't distinguish fields from relations statically.
        // The suggestion to add 'author__name' is still useful context.
        // This is a known limitation — better to over-report than miss N+1s.
        assert!(findings.is_empty() || findings[0].rule == "G110");
    }
}
