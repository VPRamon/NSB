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
fn repository_contains_no_python_code_or_shell_data_product_wrappers() {
    let repo = repository_root();
    let mut forbidden = Vec::new();
    collect_forbidden_files(&repo, &repo, &mut forbidden);
    forbidden.sort();
    assert!(
        forbidden.is_empty(),
        "Python code/tooling or shell data-product wrappers remain in the repository: {forbidden:#?}"
    );

    let registry = read_toml(&manifest_dir().join("tool-registry.toml"));
    assert!(
        registry
            .get("scripts")
            .and_then(toml::Value::as_array)
            .is_none_or(Vec::is_empty),
        "the normative registry must not retain non-Rust programs"
    );
}

fn collect_forbidden_files(root: &Path, directory: &Path, forbidden: &mut Vec<String>) {
    for entry in fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
    {
        let path = entry.expect("read repository entry").path();
        if path.is_dir() {
            let name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
            if matches!(name, ".git" | "target") {
                continue;
            }
            collect_forbidden_files(root, &path, forbidden);
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .expect("repository file must be below root")
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
        let extension = path.extension().and_then(|value| value.to_str()).unwrap_or("");
        let python_tooling = matches!(extension, "py" | "pyc" | "pyo")
            || matches!(
                file_name,
                "requirements.txt"
                    | "pyproject.toml"
                    | "Pipfile"
                    | "Pipfile.lock"
                    | "setup.py"
                    | "setup.cfg"
                    | "tox.ini"
            );
        let shell_data_product_wrapper = extension == "sh"
            && relative.starts_with("tools/starlight-xp-continuous/");
        if python_tooling || shell_data_product_wrapper {
            forbidden.push(relative);
        }
    }
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
