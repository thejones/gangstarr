use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

/// A single static analysis finding at a specific file/line.
#[derive(Debug, Clone, Serialize)]
pub struct StaticFinding {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
    pub file: String,
    /// 1-indexed line number.
    pub line: usize,
    /// 0-indexed column (indentation level).
    pub col: usize,
    pub suggestion: Option<String>,
}
