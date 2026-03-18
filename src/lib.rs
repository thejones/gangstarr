mod cli;
mod consolidate;
mod correlate;
mod storage;
mod detect;
mod fingerprint;
mod group;
mod models;
mod normalize;
mod reporter;
mod resolver_index;
mod static_analysis;

use pyo3::prelude::*;

use crate::consolidate::consolidate_by_callsite;
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
        let consolidated = consolidate_by_callsite(&groups);

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
            consolidated,
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

    /// Initialise (or migrate) the gangstarr SQLite database.
    #[pyfunction]
    fn init_gangstarr_db(db_path: &str) -> PyResult<()> {
        storage::ensure_db(db_path)
            .map(|_| ())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Store a static-analysis run in the DB.
    ///
    /// `findings_json` is a JSON array of StaticFinding objects.
    #[pyfunction]
    fn store_static_run(
        db_path: &str,
        run_id: &str,
        created_at: &str,
        project_root: &str,
        findings_json: &str,
    ) -> PyResult<()> {
        let findings: Vec<serde_json::Value> = serde_json::from_str(findings_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let conn = storage::ensure_db(db_path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        storage::insert_run(&conn, run_id, created_at, "static", project_root)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        storage::insert_static_findings(&conn, run_id, &findings)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Store a runtime-analysis run in the DB.
    ///
    /// `analysis_json` is the full dict returned by `analyze_events` (has
    /// `findings` and `groups` keys).
    #[pyfunction]
    fn store_runtime_run(
        db_path: &str,
        run_id: &str,
        created_at: &str,
        project_root: &str,
        analysis_json: &str,
    ) -> PyResult<()> {
        let analysis: serde_json::Value = serde_json::from_str(analysis_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let conn = storage::ensure_db(db_path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        storage::insert_run(&conn, run_id, created_at, "runtime", project_root)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let empty = vec![];
        let findings = analysis["findings"].as_array().unwrap_or(&empty);
        storage::insert_runtime_findings(&conn, run_id, findings)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let groups = analysis["groups"].as_array().unwrap_or(&empty);
        storage::insert_query_fingerprints(&conn, run_id, groups)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Store field-usage records collected by FieldUsageTrackerMixin.
    ///
    /// `usage_json` is a JSON array of {model, field, endpoint, serializer}.
    #[pyfunction]
    fn store_field_usage(db_path: &str, run_id: &str, usage_json: &str) -> PyResult<()> {
        let records: Vec<serde_json::Value> = serde_json::from_str(usage_json)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        let conn = storage::ensure_db(db_path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        storage::insert_field_usage(&conn, run_id, &records)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Return the last `limit` runs as a JSON string.
    #[pyfunction]
    fn get_run_history(db_path: &str, limit: usize) -> PyResult<String> {
        let history = storage::fetch_run_history(db_path, limit)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        serde_json::to_string(&history)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Cross-reference the static findings for `run_id` against all runtime
    /// data in the DB and return a JSON array of correlated findings.
    #[pyfunction]
    fn correlate_run(db_path: &str, run_id: &str) -> PyResult<String> {
        let correlations = correlate::correlate_run(db_path, run_id)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        serde_json::to_string(&correlations)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Run the gangstarr static analysis CLI.
    ///
    /// Entry point registered in [project.scripts] as `gangstarr`.
    /// Reads sys.argv for subcommand and path, runs analysis, exits with code.
    #[pyfunction]
    fn gangstarr_check(py: Python<'_>) -> PyResult<()> {
        let argv: Vec<String> = py.import("sys")?.getattr("argv")?.extract()?;
        let exit_code = cli::run_check(&argv);
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        std::process::exit(exit_code);
    }

    /// Formats the sum of two numbers as string (legacy, kept for backward compat).
    #[pyfunction]
    fn sum_as_string(a: usize, b: usize) -> PyResult<String> {
        Ok((a + b).to_string())
    }
}
