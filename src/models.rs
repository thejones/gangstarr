use pyo3::prelude::*;
use serde::{Deserialize, Serialize};

/// A single frame in a caller chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallerFrame {
    pub file: String,
    pub line: u32,
    pub function: String,
}

/// A single captured query execution event from the Python layer.
#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct QueryEvent {
    pub sql: String,
    pub duration_ms: f64,
    pub file: String,
    pub line: u32,
    pub function: String,
    pub source: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default = "default_db_alias")]
    pub db_alias: String,
    #[serde(default)]
    pub resolver_path: String,
    #[serde(default)]
    pub caller_chain: Vec<CallerFrame>,
}

fn default_db_alias() -> String {
    "default".to_string()
}

/// A callsite — unique (file, line, function, resolver_path) tuple.
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize)]
pub struct Callsite {
    pub file: String,
    pub line: u32,
    pub function: String,
    pub source: String,
    pub resolver_path: String,
}

/// A group of queries sharing the same fingerprint.
#[derive(Debug, Clone, Serialize)]
pub struct GroupedQuery {
    pub fingerprint: String,
    pub normalized_sql: String,
    pub count: usize,
    pub total_duration_ms: f64,
    pub avg_duration_ms: f64,
    pub min_duration_ms: f64,
    pub max_duration_ms: f64,
    pub p50_duration_ms: f64,
    pub sample_sql: String,
    pub callsites: Vec<CallsiteStats>,
}

/// Stats for a callsite within a grouped query.
#[derive(Debug, Clone, Serialize)]
pub struct CallsiteStats {
    pub file: String,
    pub line: u32,
    pub function: String,
    pub source: String,
    pub resolver_path: String,
    pub count: usize,
    pub total_duration_ms: f64,
}

/// Severity levels for findings.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A single analysis finding.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub code: String,
    pub title: String,
    pub severity: Severity,
    pub message: String,
    pub fingerprint: Option<String>,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub suggestion: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub resolver_path: String,
}

/// Summary statistics for an analysis run.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisSummary {
    pub total_queries: usize,
    pub unique_queries: usize,
    pub total_duration_ms: f64,
    pub duplicate_groups: usize,
    pub reads: usize,
    pub writes: usize,
}

/// A callsite with all fingerprint groups consolidated into one row.
#[derive(Debug, Clone, Serialize)]
pub struct ConsolidatedCallsite {
    pub file: String,
    pub line: u32,
    pub function: String,
    pub resolver_path: String,
    pub total_queries: usize,
    pub dup_groups: usize,
    pub worst_repeat: usize,
    pub dup_duration_ms: f64,
    pub flags: Vec<String>,
    pub top_sql: String,
    pub caller_chain: Vec<CallerFrame>,
}

/// The complete analysis result returned to Python.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisResult {
    pub summary: AnalysisSummary,
    pub groups: Vec<GroupedQuery>,
    pub findings: Vec<Finding>,
    pub consolidated: Vec<ConsolidatedCallsite>,
}

impl<'py> IntoPyObject<'py> for AnalysisResult {
    type Target = PyAny;
    type Output = Bound<'py, PyAny>;
    type Error = PyErr;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        let json_str = serde_json::to_string(&self)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let json_mod = py.import("json")?;
        json_mod.call_method1("loads", (json_str,))
    }
}
