use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank};

/// No regex needed — uses indentation-based loop tracking.
pub struct G107;

impl Rule for G107 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Stack of for-loop indent levels.
        let mut loop_stack: Vec<usize> = Vec::new();

        for (i, &line) in lines.iter().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            let indent = indent_of(line);
            let trimmed = line.trim_start();

            // Pop loops we've exited.
            loop_stack.retain(|&loop_indent| indent > loop_indent);

            // Detect `for ... :` loop start.
            if trimmed.starts_with("for ") && trimmed.contains(':') {
                loop_stack.push(indent);
            }

            // Flag `.save()` inside a loop body.
            if !loop_stack.is_empty() && trimmed.contains(".save()") {
                let deepest = *loop_stack.last().unwrap();
                if indent > deepest {
                    findings.push(StaticFinding {
                        rule: "G107".to_string(),
                        message: ".save() in a loop issues one UPDATE/INSERT per iteration — use bulk_create() or bulk_update()".to_string(),
                        severity: Severity::Warning,
                        file: file.to_string(),
                        line: i + 1,
                        col: indent,
                        suggestion: Some(
                            "Collect objects in a list then call Model.objects.bulk_create(objs) or bulk_update(objs, fields)".to_string(),
                        ),
                    });
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
    fn test_flags_save_in_loop() {
        let src = r#"
for item in items:
    obj.field = item
    obj.save()
"#;
        let findings = G107.check("views.py", src);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, "G107");
    }

    #[test]
    fn test_no_flag_save_outside_loop() {
        let src = "obj.save()\n";
        assert!(G107.check("views.py", src).is_empty());
    }

    #[test]
    fn test_flags_nested_loop_save() {
        let src = r#"
for batch in batches:
    for item in batch:
        item.save()
"#;
        let findings = G107.check("views.py", src);
        assert!(!findings.is_empty());
    }
}
