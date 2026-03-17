mod g101;
mod g102;
mod g103;
mod g104;
mod g105;
mod g106;
mod g107;

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
    ]
}

// ── Shared helpers ────────────────────────────────────────────────────────────

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
