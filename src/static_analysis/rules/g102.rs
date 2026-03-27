use regex::Regex;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank, join_logical_line, chain_contains_method};

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

/// Field-narrowing methods that suppress G102 when chained after .all().
const NARROWING_METHODS: &[&str] = &[".only(", ".values(", ".values_list(", ".defer("];

impl Rule for G102 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Detect GraphQL context: the suggestion changes because .only()
        // isn't practical when the client controls field selection.
        let is_graphql = source.contains("DjangoObjectType")
            || source.contains("graphene")
            || source.contains("ObjectType");

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i];
            if is_comment_or_blank(line) {
                i += 1;
                continue;
            }

            // Join multi-line statements so `.all(\n)` is handled correctly.
            let (logical, extra) = join_logical_line(&lines, i);

            if self.pattern.is_match(&logical) {
                // Already narrowed on the same logical statement → not a problem.
                if NARROWING_METHODS.iter().any(|m| logical.contains(m)) {
                    i += 1 + extra;
                    continue;
                }

                // Check downstream chain lines for narrowing (covers multi-line style).
                if chain_contains_method(&lines, i, NARROWING_METHODS) {
                    i += 1 + extra;
                    continue;
                }

                let (message, suggestion) = if is_graphql {
                    (
                        ".all() fetches every field — in GraphQL the client controls field selection".to_string(),
                        "Use graphene-django-optimizer to auto-apply .only() based on the query, or narrow fields via info.field_nodes".to_string(),
                    )
                } else {
                    (
                        ".all() fetches every field — consider .only() or .values() to narrow".to_string(),
                        "Use .only('field1', 'field2') or .values('field1') to fetch only needed fields".to_string(),
                    )
                };

                findings.push(StaticFinding {
                    rule: "G102".to_string(),
                    message,
                    severity: Severity::Warning,
                    file: file.to_string(),
                    line: i + 1,
                    col: indent_of(line),
                    suggestion: Some(suggestion),
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
