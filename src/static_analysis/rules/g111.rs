use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank, is_queryset_expr};

/// G111 — count() + iterate anti-pattern.
///
/// Detects when `.count()` is called on a queryset and then the same queryset
/// is iterated (e.g., `for x in qs:`). This issues two separate SQL queries
/// when one suffices.
pub struct G111;

impl G111 {
    pub fn new() -> Self {
        G111
    }
}

impl Rule for G111 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Track: variable name -> (assign_line, has_count_been_called, count_line)
        let mut qs_vars: std::collections::HashMap<String, (usize, bool, usize)> =
            std::collections::HashMap::new();

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
                    qs_vars.insert(var.to_string(), (i + 1, false, 0));
                }
            }

            // Check for .count() on tracked variables.
            for (var, (_, has_count, count_line)) in qs_vars.iter_mut() {
                if !*has_count && trimmed.contains(&format!("{}.count()", var)) {
                    *has_count = true;
                    *count_line = i + 1;
                }
            }

            // Check for iteration of a variable that already had .count() called.
            if trimmed.starts_with("for ") && trimmed.contains(':') {
                for (var, (_assign_line, has_count, count_line)) in &qs_vars {
                    if *has_count
                        && (trimmed.contains(&format!(" in {}:", var))
                            || trimmed.contains(&format!(" in {}.", var)))
                    {
                        findings.push(StaticFinding {
                            rule: "G111".to_string(),
                            message: format!(
                                "`{}.count()` at line {} then iterated here — two SQL queries instead of one",
                                var, count_line
                            ),
                            severity: Severity::Warning,
                            file: file.to_string(),
                            line: i + 1,
                            col: indent_of(line),
                            suggestion: Some(format!(
                                "Iterate first: `{var}_list = list({var})`, then use `len({var}_list)` for the count"
                            )),
                        });
                    }
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
    fn test_flags_count_then_iterate() {
        let src = r#"
qs = Artist.objects.filter(active=True)
total = qs.count()
for artist in qs:
    print(artist.name)
"#;
        let findings = G111::new().check("views.py", src);
        assert!(!findings.is_empty(), "should flag count+iterate");
        assert_eq!(findings[0].rule, "G111");
    }

    #[test]
    fn test_no_flag_count_only() {
        let src = r#"
qs = Artist.objects.filter(active=True)
total = qs.count()
print(total)
"#;
        let findings = G111::new().check("views.py", src);
        assert!(findings.is_empty(), "count-only should not flag");
    }

    #[test]
    fn test_no_flag_iterate_only() {
        let src = r#"
qs = Artist.objects.filter(active=True)
for artist in qs:
    print(artist.name)
"#;
        let findings = G111::new().check("views.py", src);
        assert!(findings.is_empty(), "iterate-only should not flag");
    }
}
