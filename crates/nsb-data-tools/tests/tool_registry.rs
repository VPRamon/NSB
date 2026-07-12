use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn read_toml(path: &Path) -> toml::Value {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    toml::from_str(&raw)
        .unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn required_string<'a>(
    table: &'a toml::value::Table,
    key: &str,
    context: &str,
) -> &'a str {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| panic!("{context} must define non-empty `{key}`"))
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn repository_root() -> PathBuf {
    manifest_dir()
        .parent()
        .and_then(Path::parent)
        .expect("nsb-data-tools must live under <repo>/crates")
        .to_path_buf()
}

#[test]
fn every_compiled_binary_is_registered_and_documented() {
    let crate_root = manifest_dir();
    let cargo = read_toml(&crate_root.join("Cargo.toml"));
    let registry = read_toml(&crate_root.join("tool-registry.toml"));

    assert_eq!(
        registry
            .get("schema_version")
            .and_then(toml::Value::as_integer),
        Some(1),
        "unsupported tool registry schema"
    );

    let cargo_bins = cargo
        .get("bin")
        .and_then(toml::Value::as_array)
        .expect("Cargo.toml must declare explicit [[bin]] entries");
    let mut expected = BTreeMap::new();
    for value in cargo_bins {
        let table = value
            .as_table()
            .expect("each [[bin]] must be a table");
        let name = required_string(table, "name", "Cargo binary");
        let path = required_string(table, "path", name);
        assert!(
            expected
                .insert(name.to_owned(), path.to_owned())
                .is_none(),
            "duplicate Cargo binary `{name}`"
        );
    }

    let entries = registry
        .get("binaries")
        .and_then(toml::Value::as_array)
        .expect("tool-registry.toml must define [[binaries]]");
    let mut documented = BTreeMap::new();
    for value in entries {
        let table = value
            .as_table()
            .expect("each [[binaries]] entry must be a table");
        let name = required_string(table, "name", "registry binary");
        let path = required_string(table, "path", name);
        let status = required_string(table, "status", name);
        assert!(
            matches!(status, "supported" | "experimental"),
            "compiled binary `{name}` has unsupported status `{status}`"
        );
        assert!(
            !name.contains("phase5") && !name.contains("phase5b"),
            "compiled commands must describe durable capabilities, not historical phases: `{name}`"
        );
        for key in [
            "owner",
            "audience",
            "purpose",
            "inputs",
            "outputs",
            "resume",
            "exit_codes",
            "documentation",
        ] {
            required_string(table, key, name);
        }
        assert!(
            crate_root.join(path).is_file(),
            "registered binary `{name}` points to missing file `{path}`"
        );
        assert!(
            documented
                .insert(name.to_owned(), path.to_owned())
                .is_none(),
            "duplicate registry entry for `{name}`"
        );
    }

    assert_eq!(
        documented, expected,
        "Cargo binary declarations and the normative tool registry differ"
    );
}

#[test]
fn every_python_or_shell_program_is_explicitly_temporary() {
    let repo = repository_root();
    let registry = read_toml(&manifest_dir().join("tool-registry.toml"));
    let script_dir = repo.join("tools/starlight-xp-continuous");

    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(&script_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", script_dir.display()))
    {
        let path = entry.expect("read script directory entry").path();
        let extension = path.extension().and_then(|value| value.to_str());
        if matches!(extension, Some("py" | "sh")) {
            let relative = path
                .strip_prefix(&repo)
                .expect("script must be below repository root")
                .to_string_lossy()
                .replace('\\', "/");
            actual.insert(relative);
        }
    }

    assert!(
        actual.iter().all(|path| !path.ends_with(".sh")),
        "supported data-product orchestration must not depend on shell wrappers: {actual:?}"
    );

    let entries = registry
        .get("scripts")
        .and_then(toml::Value::as_array)
        .expect("tool-registry.toml must define [[scripts]]");
    let mut documented = BTreeSet::new();
    for value in entries {
        let table = value
            .as_table()
            .expect("each [[scripts]] entry must be a table");
        let path = required_string(table, "path", "registry script");
        let status = required_string(table, "status", path);
        assert!(
            matches!(status, "migration-only" | "test-only"),
            "non-Rust program `{path}` must be temporary, got `{status}`"
        );
        assert_eq!(
            required_string(table, "removal_issue", path),
            "#61",
            "every retained non-Rust program must have the pure-Rust removal issue"
        );
        required_string(table, "purpose", path);
        assert!(
            repo.join(path).is_file(),
            "registered script `{path}` is missing"
        );
        assert!(
            documented.insert(path.to_owned()),
            "duplicate registry entry for `{path}`"
        );

        let source = fs::read_to_string(repo.join(path))
            .expect("read registered script");
        assert!(
            !source.contains("/home/valles/") && !source.contains("C:\\Users\\"),
            "registered script `{path}` contains a developer-specific absolute path"
        );
    }

    assert_eq!(
        documented, actual,
        "Python/shell programs and the normative tool registry differ"
    );
}

#[test]
fn generated_operational_reports_are_not_repository_source() {
    let repo = repository_root();
    for path in [
        "pipeline_report.json",
        "session_manifest.json",
        "storage_plan.json",
        "storage_plan.md",
    ] {
        assert!(
            !repo.join(path).exists(),
            "generated machine-specific operational report must not be committed: {path}"
        );
    }
}
