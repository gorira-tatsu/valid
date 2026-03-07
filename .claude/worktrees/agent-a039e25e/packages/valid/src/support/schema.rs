//! Lightweight schema-like validation without external dependencies.

pub fn require_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be empty"))
    } else {
        Ok(())
    }
}

pub fn require_schema_version(value: &str) -> Result<(), String> {
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| part.parse::<u64>().is_err()) {
        return Err("schema_version must use major.minor.patch".to_string());
    }
    Ok(())
}

pub fn require_len_match(
    left: usize,
    right: usize,
    left_name: &str,
    right_name: &str,
) -> Result<(), String> {
    if left != right {
        Err(format!(
            "{left_name} and {right_name} must have the same length"
        ))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{require_len_match, require_non_empty, require_schema_version};

    #[test]
    fn validates_semver_like_versions() {
        assert!(require_schema_version("1.0.0").is_ok());
        assert!(require_schema_version("1.0").is_err());
    }

    #[test]
    fn validates_non_empty_strings() {
        assert!(require_non_empty("x", "field").is_ok());
        assert!(require_non_empty("   ", "field").is_err());
    }

    #[test]
    fn validates_equal_lengths() {
        assert!(require_len_match(1, 1, "a", "b").is_ok());
        assert!(require_len_match(1, 2, "a", "b").is_err());
    }
}
