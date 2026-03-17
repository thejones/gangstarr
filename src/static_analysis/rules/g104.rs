use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank};

pub struct G104 {
    /// `len( <expr containing .objects. / .filter( / .all()> )`
    pattern: Regex,
}

impl G104 {
    pub fn new() -> Self {
        G104 {
            pattern: Regex::new(
                r"\blen\s*\(\s*\w[\w.]*\.(?:objects\b|filter\s*\(|exclude\s*\(|all\s*\()",
            )
            .unwrap(),
        }
    }
}

impl Rule for G104 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();

        for (i, line) in source.lines().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            if self.pattern.is_match(line) {
                findings.push(StaticFinding {
                    rule: "G104".to_string(),
                    message: "len() on a queryset loads every row — use .count() for a SQL COUNT".to_string(),
                    severity: Severity::Warning,
                    file: file.to_string(),
                    line: i + 1,
                    col: indent_of(line),
                    suggestion: Some(
                        "Replace len(queryset) with queryset.count()".to_string(),
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
    fn test_flags_len_of_objects_all() {
        let src = "n = len(Artist.objects.all())\n";
        assert_eq!(G104::new().check("views.py", src).len(), 1);
    }

    #[test]
    fn test_flags_len_of_filter() {
        let src = "n = len(Artist.objects.filter(active=True))\n";
        assert_eq!(G104::new().check("views.py", src).len(), 1);
    }

    #[test]
    fn test_no_flag_len_of_list() {
        let src = "n = len(my_list)\n";
        assert!(G104::new().check("views.py", src).is_empty());
    }
}
