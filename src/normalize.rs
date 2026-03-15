/// Normalize a SQL query by replacing literal values with parameter placeholders.
///
/// Uses the actual Postgres parser via pg_query to produce stable, comparable SQL.
/// Example: `SELECT * FROM song WHERE id = 1` → `SELECT $1 FROM song WHERE id = $2`
pub fn normalize(sql: &str) -> String {
    match pg_query::normalize(sql) {
        Ok(normalized) => normalized,
        // If pg_query can't parse it (e.g. SQLite dialect quirks), return as-is
        Err(_) => sql.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_replaces_literals() {
        let result = normalize("SELECT * FROM song WHERE id = 1");
        assert!(result.contains("$1") || result.contains("$"));
    }

    #[test]
    fn test_normalize_unparseable_returns_original() {
        let weird = "NOT VALID SQL AT ALL %%%";
        assert_eq!(normalize(weird), weird);
    }
}
