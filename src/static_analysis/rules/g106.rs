use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank};

pub struct G106 {
    /// `sum(expr for var in <queryset>)`
    sum_gen: Regex,
}

impl G106 {
    pub fn new() -> Self {
        G106 {
            sum_gen: Regex::new(
                r"\bsum\s*\(.+\bfor\s+\w+\s+in\s+\w[\w.]*\.(?:objects\b|filter\s*\(|exclude\s*\(|all\s*\()",
            )
            .unwrap(),
        }
    }
}

impl Rule for G106 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();

        for (i, line) in source.lines().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            if self.sum_gen.is_match(line) {
                findings.push(StaticFinding {
                    rule: "G106".to_string(),
                    message: "Python-side aggregation over queryset — use .aggregate() or .annotate() instead".to_string(),
                    severity: Severity::Warning,
                    file: file.to_string(),
                    line: i + 1,
                    col: indent_of(line),
                    suggestion: Some(
                        "Use queryset.aggregate(total=Sum('field')) for database-level aggregation".to_string(),
                    ),
                });
            }
        }

        findings
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flags_sum_generator() {
        let src = "total = sum(e.salary for e in Employee.objects.all())\n";
        let findings = G106::new().check("views.py", src);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "G106");
    }

    #[test]
    fn test_no_flag_sum_of_list() {
        let src = "total = sum(x for x in my_list)\n";
        assert!(G106::new().check("views.py", src).is_empty());
    }
}
