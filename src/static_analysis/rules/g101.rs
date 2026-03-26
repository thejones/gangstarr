use std::collections::HashSet;

use crate::static_analysis::models::{Severity, StaticFinding};
use crate::static_analysis::rules::{Rule, indent_of, is_comment_or_blank, is_queryset_expr};

pub struct G101;

impl G101 {
    pub fn new() -> Self {
        G101
    }
}

impl Rule for G101 {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding> {
        let mut findings = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        // Variables assigned from queryset expressions in this file.
        // We track file-wide to handle common cases (same function scope).
        let mut qs_vars: HashSet<String> = HashSet::new();

        // Active for-loops: (indent_of_for_line, loop_var, line_no, is_qs_loop)
        let mut loop_stack: Vec<(usize, String, usize, bool)> = Vec::new();

        for (i, &line) in lines.iter().enumerate() {
            if is_comment_or_blank(line) {
                continue;
            }

            let indent = indent_of(line);
            let trimmed = line.trim_start();

            // Pop loops whose body we've exited (current indent <= loop indent).
            loop_stack.retain(|(loop_indent, _, _, _)| indent > *loop_indent);

            // Track queryset variable assignments:  VAR = <queryset expr>
            if let Some((var, rhs)) = parse_simple_assignment(trimmed) {
                if is_queryset_expr(&rhs) {
                    qs_vars.insert(var);
                }
            }

            // Detect `for VAR in EXPR:`
            if trimmed.starts_with("for ") && trimmed.trim_end_matches(|c: char| c == ':' || c.is_whitespace()).ends_with(':') || trimmed.starts_with("for ") && trimmed.contains(':') {
                if let Some((var, iter_expr)) = parse_for_loop(trimmed) {
                    // Is the iterable a queryset expression OR a known queryset variable?
                    let is_qs = is_queryset_expr(&iter_expr)
                        || qs_vars.contains(iter_expr.trim());
                    loop_stack.push((indent, var, i + 1, is_qs));
                }
            }

            // Checks inside a loop body.
            for (loop_indent, loop_var, _loop_line, is_qs_loop) in &loop_stack {
                if indent <= *loop_indent {
                    continue; // not actually in the body yet
                }

                // Sub-check A: related field access VAR.X.Y in a queryset loop.
                // E.g. `album.artist.name` — `artist` is a FK, `name` is its field.
                if *is_qs_loop && has_related_attr_access(trimmed, loop_var) {
                    findings.push(StaticFinding {
                        rule: "G101".to_string(),
                        message: format!(
                            "Possible N+1: `{}` accesses a related field in a loop — add select_related() or prefetch_related()",
                            loop_var
                        ),
                        severity: Severity::Warning,
                        file: file.to_string(),
                        line: i + 1,
                        col: indent,
                        suggestion: Some(
                            "Add select_related() or prefetch_related() to the outer queryset".to_string(),
                        ),
                    });
                    break; // one finding per line is enough
                }

                // Sub-check B: explicit query (.objects.get / .objects.filter) inside any loop.
                // This is always an N+1 regardless of whether the loop var is a queryset.
                if trimmed.contains(".objects.get(") || trimmed.contains(".objects.filter(") {
                    findings.push(StaticFinding {
                        rule: "G101".to_string(),
                        message: "Query inside loop issues one SQL per iteration — use select_related() or a bulk lookup".to_string(),
                        severity: Severity::Error,
                        file: file.to_string(),
                        line: i + 1,
                        col: indent,
                        suggestion: Some(
                            "Move the query outside the loop or use select_related() / prefetch_related()".to_string(),
                        ),
                    });
                    break;
                }
            }
        }

        findings
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Parse `for VAR in EXPR:` → (var, iter_expr).
/// Handles simple cases; ignores tuple unpacking (takes first variable).
fn parse_for_loop(trimmed: &str) -> Option<(String, String)> {
    let without_for = trimmed.strip_prefix("for ")?;
    let in_pos = without_for.find(" in ")?;
    let var_part = without_for[..in_pos].trim();
    let rest = &without_for[in_pos + 4..];
    // Strip trailing colon (and any trailing comment / whitespace)
    let iter_expr = rest
        .split('#')
        .next()
        .unwrap_or(rest)
        .trim_end_matches(|c: char| c == ':' || c.is_whitespace());

    // Take just the first variable (handles `for k, v in ...`)
    let var = var_part
        .trim_start_matches('(')
        .split(',')
        .next()?
        .trim()
        .to_string();

    if var.is_empty() || iter_expr.is_empty() {
        return None;
    }
    Some((var, iter_expr.to_string()))
}

/// Parse `VAR = EXPR` → (var, rhs).  Ignores augmented assignments, comparisons.
fn parse_simple_assignment(trimmed: &str) -> Option<(String, String)> {
    // Must not be a keyword statement
    if trimmed.starts_with("if ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("assert ")
    {
        return None;
    }

    let eq_pos = trimmed.find('=')?;
    if eq_pos == 0 {
        return None;
    }

    // Reject ==, !=, <=, >=, +=, -=, *=, /=
    let prev = trimmed.as_bytes().get(eq_pos - 1).copied().unwrap_or(0) as char;
    let next = trimmed.as_bytes().get(eq_pos + 1).copied().unwrap_or(0) as char;
    if next == '=' || matches!(prev, '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '&' | '|' | '^') {
        return None;
    }

    let var = trimmed[..eq_pos].trim();
    // Must be a plain identifier (no spaces, dots, brackets)
    if var.contains(|c: char| !c.is_alphanumeric() && c != '_') {
        return None;
    }

    let rhs = trimmed[eq_pos + 1..].trim().to_string();
    Some((var.to_string(), rhs))
}

/// True if `line` contains `VAR.X.Y` where X is likely a related model field.
///
/// Heuristic: after `VAR.`, there is `WORD.WORD` where the second access is
/// NOT a method call on a primitive field (lower(), strip(), strftime(), etc.).
fn has_related_attr_access(line: &str, var: &str) -> bool {
    let needle = format!("{}.", var);
    let mut search_from = 0;

    while search_from < line.len() {
        let slice = &line[search_from..];
        let Some(pos) = slice.find(&needle) else {
            break;
        };
        let abs_pos = search_from + pos;

        // Ensure word-boundary before VAR (not part of a longer identifier).
        let is_word_boundary = abs_pos == 0 || {
            let prev = line.as_bytes()[abs_pos - 1] as char;
            !prev.is_alphanumeric() && prev != '_'
        };

        if is_word_boundary {
            let after_dot = &line[abs_pos + needle.len()..];

            // Grab WORD (the first attribute after VAR.)
            let first_attr_end = after_dot
                .find(|c: char| !c.is_alphanumeric() && c != '_')
                .unwrap_or(after_dot.len());
            let first_attr = &after_dot[..first_attr_end];
            let after_first = &after_dot[first_attr_end..];

            // Must have a second dot access: VAR.first_attr.something
            if after_first.starts_with('.') {
                let after_second_dot = &after_first[1..];
                let second_attr_end = after_second_dot
                    .find(|c: char| !c.is_alphanumeric() && c != '_')
                    .unwrap_or(after_second_dot.len());
                let second_attr = &after_second_dot[..second_attr_end];
                let after_second = &after_second_dot[second_attr_end..];

                // If the second access is a queryset method, it's a related manager → flag.
                let is_qs_method = matches!(
                    second_attr,
                    "all" | "filter" | "exclude" | "get" | "first" | "last" | "count" | "exists"
                );

                // If the char after the second attribute is '(' it's a method call on a
                // simple field (e.g. album.title.lower()) — skip unless it's a queryset method.
                let is_plain_method_call =
                    !is_qs_method && after_second.starts_with('(');

                if !first_attr.is_empty()
                    && !second_attr.is_empty()
                    && !is_plain_method_call
                {
                    return true;
                }
            }
        }

        search_from = abs_pos + 1;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_related_field_in_loop() {
        let src = r#"
albums = Album.objects.all()
for album in albums:
    artists.append(album.artist.name)
"#;
        let findings = G101::new().check("views.py", src);
        assert!(!findings.is_empty(), "should flag album.artist.name in loop");
        assert_eq!(findings[0].rule, "G101");
    }

    #[test]
    fn test_direct_queryset_loop() {
        let src = r#"
for album in Album.objects.all():
    print(album.artist.name)
"#;
        let findings = G101::new().check("views.py", src);
        assert!(!findings.is_empty());
    }

    #[test]
    fn test_query_inside_loop() {
        let src = r#"
for artist_id in ids:
    artist = Artist.objects.get(pk=artist_id)
    print(artist.name)
"#;
        let findings = G101::new().check("views.py", src);
        assert!(!findings.is_empty());
        assert_eq!(findings[0].rule, "G101");
    }

    #[test]
    fn test_no_false_positive_string_method() {
        // album.title.lower() — title is a plain string field, not a related model
        let src = r#"
for album in Album.objects.all():
    print(album.title.lower())
"#;
        let findings = G101::new().check("views.py", src);
        // G102 might fire but G101 should NOT fire for album.title.lower()
        let g101_findings: Vec<_> = findings.iter().filter(|f| f.rule == "G101").collect();
        assert!(g101_findings.is_empty(), "should not flag plain string method chains");
    }
}
