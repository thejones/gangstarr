use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Generate a deterministic fingerprint for a SQL query shape.
///
/// Uses pg_query's fingerprinting (based on the actual Postgres parser) when possible,
/// falls back to hashing the normalized SQL for unparseable queries.
pub fn fingerprint(sql: &str) -> String {
    match pg_query::fingerprint(sql) {
        Ok(result) => result.hex,
        Err(_) => {
            // Fallback: hash the raw SQL for queries pg_query can't parse
            let mut hasher = DefaultHasher::new();
            sql.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_same_shape_same_fingerprint() {
        let fp1 = fingerprint("SELECT * FROM song WHERE id = 1");
        let fp2 = fingerprint("SELECT * FROM song WHERE id = 2");
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn test_different_shape_different_fingerprint() {
        let fp1 = fingerprint("SELECT * FROM song WHERE id = 1");
        let fp2 = fingerprint("SELECT * FROM artist WHERE name = 'test'");
        assert_ne!(fp1, fp2);
    }
}
