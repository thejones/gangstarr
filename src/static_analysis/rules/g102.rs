use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank};

pub struct G102 {
    pattern: Regex,
}

impl G102 {
    pub fn new() -> Self {
        G102 {
            // Match `.all()` anywhere on the line.
            pattern: Regex::new(r"\.all\s*\(\s*\)").unwrap(),
        }
    }
}

impl Rule for G102 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();

        for (i, line) in source.lines().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            if self.pattern.is_match(line) {
                // Already narrowed on the same logical line → not a problem.
                if line.contains(".only(")
                    || line.contains(".values(")
                    || line.contains(".values_list(")
                    || line.contains(".defer(")
                {
                    continue;
                }

                findings.push(StaticFinding {
                    rule: "G102".to_string(),
                    message: ".all() fetches every field — consider .only() or .values() to narrow".to_string(),
                    severity: Severity::Warning,
                    file: file.to_string(),
                    line: i + 1,
                    col: indent_of(line),
                    suggestion: Some(
                        "Use .only('field1', 'field2') or .values('field1') to fetch only needed fields".to_string(),
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
    fn test_flags_plain_all() {
        let src = "artists = Artist.objects.all()\n";
        let findings = G102::new().check("views.py", src);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "G102");
    }

    #[test]
    fn test_no_flag_with_only() {
        let src = "artists = Artist.objects.all().only('name')\n";
        assert!(G102::new().check("views.py", src).is_empty());
    }

    #[test]
    fn test_no_flag_with_values() {
        let src = "names = Artist.objects.all().values('name')\n";
        assert!(G102::new().check("views.py", src).is_empty());
    }
}
