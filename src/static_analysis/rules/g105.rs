use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank, join_logical_line};

pub struct G105 {
    /// `if <queryset_expr>:` without `.exists()`
    pattern: Regex,
}

impl G105 {
    pub fn new() -> Self {
        G105 {
            // if/elif/while followed by a queryset expression (not already .exists())
            pattern: Regex::new(
                r"(?:^|\s)(?:if|elif|while)\s+\w[\w.]*\.(?:objects\b|filter\s*\(|exclude\s*\(|all\s*\()",
            )
            .unwrap(),
        }
    }
}

impl Rule for G105 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if is_comment_or_blank(line) {
                i += 1;
                continue;
            }

            // Join multi-line statements so `.filter(\n...\n).exists()` is one unit.
            let (logical, extra) = join_logical_line(&lines, i);

            // Already uses .exists() on the logical line → fine.
            if logical.contains(".exists()") {
                i += 1 + extra;
                continue;
            }

            if self.pattern.is_match(&logical) {
                findings.push(StaticFinding {
                    rule: "G105".to_string(),
                    message: "Queryset truthiness check loads rows — use .exists() instead".to_string(),
                    severity: Severity::Warning,
                    file: file.to_string(),
                    line: i + 1,
                    col: indent_of(line),
                    suggestion: Some(
                        "Replace `if queryset:` with `if queryset.exists():`".to_string(),
                    ),
                });
            }

            i += 1 + extra;
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_if_filter() {
        let src = "    if Artist.objects.filter(name='foo'):\n        pass\n";
        let findings = G105::new().check("views.py", src);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "G105");
    }

    #[test]
    fn test_no_flag_with_exists() {
        let src = "    if Artist.objects.filter(name='foo').exists():\n        pass\n";
        assert!(G105::new().check("views.py", src).is_empty());
    }
}
