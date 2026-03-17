use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank};

pub struct G103 {
    /// `[expr for var in <queryset> if cond]`
    list_comp: Regex,
}

impl G103 {
    pub fn new() -> Self {
        G103 {
            list_comp: Regex::new(
                r"\[.+\bfor\s+\w+\s+in\s+.+\.objects\b.+\bif\b",
            )
            .unwrap(),
        }
    }
}

impl Rule for G103 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();

        for (i, line) in source.lines().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            if self.list_comp.is_match(line) {
                findings.push(StaticFinding {
                    rule: "G103".to_string(),
                    message: "Python-side filtering in list comprehension — move the `if` into a .filter() call".to_string(),
                    severity: Severity::Warning,
                    file: file.to_string(),
                    line: i + 1,
                    col: indent_of(line),
                    suggestion: Some(
                        "Replace `[x for x in qs if x.field]` with `qs.filter(field=True)`".to_string(),
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
    fn test_flags_list_comp_with_if() {
        let src = "active = [a for a in Artist.objects.all() if a.is_active]\n";
        let findings = G103::new().check("views.py", src);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "G103");
    }

    #[test]
    fn test_no_flag_without_if() {
        let src = "names = [a.name for a in Artist.objects.all()]\n";
        assert!(G103::new().check("views.py", src).is_empty());
    }
}
