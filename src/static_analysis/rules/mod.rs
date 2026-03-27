mod g101;
mod g102;
mod g103;
mod g104;
mod g105;
mod g106;
mod g107;
mod g108;
mod g109;
mod g110;
mod g111;

use crate::static_analysis::models::StaticFinding;

pub trait Rule: Send + Sync {
    fn check(&self, file: &str, source: &str) -> Vec<StaticFinding>;
}

pub fn all_rules() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(g101::G101::new()),
        Box::new(g102::G102::new()),
        Box::new(g103::G103::new()),
        Box::new(g104::G104::new()),
        Box::new(g105::G105::new()),
        Box::new(g106::G106::new()),
        Box::new(g107::G107),
        Box::new(g108::G108::new()),
        Box::new(g109::G109::new()),
        Box::new(g110::G110::new()),
        Box::new(g111::G111::new()),
    ]
}

// ── Shared helpers ──────────────────────────────────────────────────────────────────

/// Number of leading spaces/tabs on a line (0-indexed indentation level).
pub(super) fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// True if the (stripped) line is a comment or blank.
pub(super) fn is_comment_or_blank(line: &str) -> bool {
    let t = line.trim_start();
    t.is_empty() || t.starts_with('#')
}

/// True if the expression looks like it produces a Django queryset.
/// Covers both direct ORM calls and expressions that chain queryset methods.
pub(super) fn is_queryset_expr(expr: &str) -> bool {
    let e = expr.trim();
    e.contains(".objects.")
        || e.contains(".objects")
        || e.ends_with(".all()")
        || e.contains(".filter(")
        || e.contains(".exclude(")
        || e.contains(".annotate(")
        || e.contains(".select_related(")
        || e.contains(".prefetch_related(")
        || e.contains(".values(")
        || e.contains(".values_list(")
}

/// Join physical lines into a logical statement by looking ahead from `start`
/// for continuation lines (lines inside unclosed parens/brackets, or lines
/// ending with `\`).  Returns the joined text and how many extra physical
/// lines were consumed (0 = no continuation).
pub(super) fn join_logical_line(lines: &[&str], start: usize) -> (String, usize) {
    let first = lines[start];
    let mut joined = first.to_string();
    let mut depth: i32 = 0;

    // Count open/close parens on the first line.
    for ch in first.chars() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '#' => break,  // stop at comment
            _ => {}
        }
    }

    // Explicit backslash continuation.
    let mut has_backslash = first.trim_end().ends_with('\\');

    if depth <= 0 && !has_backslash {
        return (joined, 0);
    }

    let mut extra = 0usize;
    for i in (start + 1)..lines.len() {
        let line = lines[i];
        extra += 1;
        joined.push(' ');
        joined.push_str(line.trim());

        for ch in line.chars() {
            match ch {
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                '#' => break,
                _ => {}
            }
        }

        has_backslash = line.trim_end().ends_with('\\');

        if depth <= 0 && !has_backslash {
            break;
        }
    }

    (joined, extra)
}

/// Check if lines following `start` (within the same chain expression) contain
/// any of the given method calls.  Looks for continuation lines that begin with
/// `.` at the same or deeper indentation.
pub(super) fn chain_contains_method(lines: &[&str], start: usize, methods: &[&str]) -> bool {
    let base_indent = indent_of(lines[start]);
    for i in (start + 1)..lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let ind = indent_of(line);
        // Chain continuation: starts with . or ) at same/deeper indent
        if ind >= base_indent && (trimmed.starts_with('.') || trimmed.starts_with(')')) {
            for method in methods {
                if trimmed.contains(method) {
                    return true;
                }
            }
        } else {
            break;  // Left the chain
        }
    }
    false
}
