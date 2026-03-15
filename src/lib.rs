mod detect;
mod fingerprint;
mod group;
mod models;
mod normalize;
mod resolver_index;

use pyo3::prelude::*;

use crate::detect::detect_patterns;
use crate::group::group_by_fingerprint;
use crate::models::{AnalysisResult, AnalysisSummary, QueryEvent};
use crate::resolver_index::FileInput;

/// A Python module implemented in Rust.
#[pymodule]
mod gangstarr {
    use super::*;

    /// Normalize a SQL query by replacing literal values with $N placeholders.
    #[pyfunction]
    fn normalize_sql(sql: &str) -> String {
        normalize::normalize(sql)
    }

    /// Generate a deterministic fingerprint for a SQL query shape.
    #[pyfunction]
    fn fingerprint_sql(sql: &str) -> String {
        fingerprint::fingerprint(sql)
    }

    /// Analyze a list of query events and return structured findings.
    ///
    /// Each event is a dict with keys: sql, duration_ms, file, line, function, source,
    /// and optionally: label, request_id, db_alias.
    ///
    /// Returns a dict with: summary, groups, findings.
    #[pyfunction]
    fn analyze_events<'py>(py: Python<'py>, events_json: &str) -> PyResult<Bound<'py, PyAny>> {
        let events: Vec<QueryEvent> = serde_json::from_str(events_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        let groups = group_by_fingerprint(&events);
        let findings = detect_patterns(&groups);

        let total_queries = events.len();
        let unique_queries = groups.len();
        let total_duration_ms: f64 = events.iter().map(|e| e.duration_ms).sum();
        let duplicate_groups = groups.iter().filter(|g| g.count > 1).count();

        let reads = events
            .iter()
            .filter(|e| {
                let upper = e.sql.trim_start().to_uppercase();
                upper.starts_with("SELECT") || upper.starts_with("WITH")
            })
            .count();
        let writes = total_queries - reads;

        let result = AnalysisResult {
            summary: AnalysisSummary {
                total_queries,
                unique_queries,
                total_duration_ms,
                duplicate_groups,
                reads,
                writes,
            },
            groups,
            findings,
        };

        Ok(result.into_pyobject(py)?)
    }

    /// Convert a camelCase GraphQL field name to snake_case Python name.
    #[pyfunction]
    fn camel_to_snake(name: &str) -> String {
        resolver_index::camel_to_snake(name)
    }

    /// Scan Python files for GraphQL resolver definitions.
    ///
    /// Accepts a JSON array of {"path": "...", "content": "..."} objects.
    /// Returns a JSON object mapping "TypeName.fieldName" to resolved locations.
    #[pyfunction]
    fn scan_resolvers(files_json: &str) -> PyResult<String> {
        let files: Vec<FileInput> = serde_json::from_str(files_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let index = resolver_index::scan_files(&files);
        serde_json::to_string(&index)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Formats the sum of two numbers as string (legacy, kept for backward compat).
    #[pyfunction]
    fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
        Ok((a + b).to_string())
    }
}
