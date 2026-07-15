use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn read_toml(path: &Path) -> toml::Value {
    let raw =
        fs::read_to_string(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    toml::from_str(&raw).unwrap_or_else(|error| panic!("parse {}: {error}", path.display()))
}

fn required_string<'a>(table: &'a toml::value::Table, key: &str, context: &str) -> &'a str {
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
        let table = value.as_table().expect("each [[bin]] must be a table");
        let name = required_string(table, "name", "Cargo binary");
        let path = required_string(table, "path", name);
        assert!(
            expected.insert(name.to_owned(), path.to_owned()).is_none(),
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
fn repository_contains_no_python_or_shell_tools() {
    let repo = repository_root();
    fn visit(path: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).expect("read repository directory") {
            let path = entry.expect("directory entry").path();
            if matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("target" | ".git")
            ) {
                continue;
            }
            if path.is_dir() {
                visit(&path, found);
            } else if matches!(
                path.extension().and_then(|ext| ext.to_str()),
                Some("py" | "sh")
            ) {
                found.push(path);
            }
        }
    }

    let mut non_rust_tools = Vec::new();
    visit(&repo, &mut non_rust_tools);
    assert!(
        non_rust_tools.is_empty(),
        "the repository must not retain Python or shell tools: {non_rust_tools:?}"
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
