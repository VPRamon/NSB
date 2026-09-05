use std::path::Path;

const CRATE_NAMES: [&str; 5] = [
    "nsb-public-api-gate",
    "nsb-coverage-gate",
    "nsb-data-tools",
    "nsb-cli",
    "nsb",
];

/// Normalize an llvm-cov filename to a repo-relative POSIX path.
pub fn repo_relative(filename: &str) -> String {
    let normalized = filename.replace('\\', "/");
    if let Some(index) = normalized.find("/crates/") {
        return normalized[index + 1..].to_string();
    }
    if let Some(stripped) = normalized.strip_prefix("./") {
        return stripped.to_string();
    }
    if normalized.starts_with("crates/") {
        return normalized;
    }
    Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&normalized)
        .to_string()
}

/// Workspace crate owning `relative_path`, if any.
pub fn workspace_crate(relative_path: &str) -> Option<&'static str> {
    let path = relative_path.replace('\\', "/");
    CRATE_NAMES.iter().copied().find(|name| {
        path.starts_with(&format!("crates/{name}/")) || path == format!("crates/{name}")
    })
}

/// Production Rust sources that the diff gate treats as coverage targets.
///
/// Integration tests, unit-test modules, benches, examples, and build scripts
/// are ignored as coverage *targets*. They may still appear in overall
/// workspace totals because `cargo llvm-cov` reports the collected profile.
pub fn is_production_rust_file(relative_path: &str) -> bool {
    let path = relative_path.replace('\\', "/");
    if !path.ends_with(".rs") {
        return false;
    }
    let Some(crate_name) = workspace_crate(&path) else {
        return false;
    };
    if crate_name == "nsb-coverage-gate" || crate_name == "nsb-public-api-gate" {
        return false;
    }
    let prefix = format!("crates/{crate_name}/");
    let rest = &path[prefix.len()..];
    if !(rest.starts_with("src/") || rest == "src") {
        return false;
    }
    if rest == "src/main.rs" || rest.ends_with("/main.rs") {
        return true;
    }
    if rest.ends_with("/tests.rs") || rest == "src/tests.rs" {
        return false;
    }
    if rest.contains("/tests/") {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_paths_exclude_test_modules() {
        assert!(is_production_rust_file("crates/nsb/src/lib.rs"));
        assert!(is_production_rust_file(
            "crates/nsb-cli/src/commands/window.rs"
        ));
        assert!(!is_production_rust_file(
            "crates/nsb/src/solar_activity/tests.rs"
        ));
        assert!(!is_production_rust_file(
            "crates/nsb-cli/tests/starlight_contract.rs"
        ));
        assert!(!is_production_rust_file(
            "crates/nsb/benches/threshold_window.rs"
        ));
        assert!(!is_production_rust_file(
            "crates/nsb-cli/src/commands/tests.rs"
        ));
        assert!(!is_production_rust_file(
            "crates/nsb-coverage-gate/src/check.rs"
        ));
        assert!(!is_production_rust_file(
            "crates/nsb-public-api-gate/src/base.rs"
        ));
    }

    #[test]
    fn crate_names_do_not_prefix_match() {
        assert_eq!(workspace_crate("crates/nsb/src/lib.rs"), Some("nsb"));
        assert_eq!(
            workspace_crate("crates/nsb-cli/src/lib.rs"),
            Some("nsb-cli")
        );
        assert_eq!(
            workspace_crate("crates/nsb-data-tools/src/lib.rs"),
            Some("nsb-data-tools")
        );
    }
}
