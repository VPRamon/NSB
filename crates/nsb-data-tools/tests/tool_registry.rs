use std::collections::BTreeSet;
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
fn hierarchical_cli_and_registry_describe_only_durable_actions() {
    let crate_root = manifest_dir();
    let cargo = read_toml(&crate_root.join("Cargo.toml"));
    let registry = read_toml(&crate_root.join("tool-registry.toml"));

    assert_eq!(
        registry
            .get("schema_version")
            .and_then(toml::Value::as_integer),
        Some(2),
        "unsupported tool registry schema"
    );

    let cargo_bins = cargo
        .get("bin")
        .and_then(toml::Value::as_array)
        .expect("Cargo.toml must declare explicit [[bin]] entries");
    assert_eq!(cargo_bins.len(), 1, "nsb-data is the only supported binary");
    let binary = cargo_bins[0].as_table().expect("[[bin]] must be a table");
    assert_eq!(required_string(binary, "name", "Cargo binary"), "nsb-data");
    assert_eq!(
        required_string(binary, "path", "Cargo binary"),
        "src/bin/nsb-data.rs"
    );

    let entries = registry
        .get("actions")
        .and_then(toml::Value::as_array)
        .expect("tool-registry.toml must define [[actions]]");
    let mut commands = BTreeSet::new();
    for value in entries {
        let table = value
            .as_table()
            .expect("each [[actions]] entry must be a table");
        let command = required_string(table, "command", "registry action");
        let status = required_string(table, "status", command);
        assert!(
            status == "supported",
            "action `{command}` has unsupported status `{status}`"
        );
        assert!(
            !command.contains("phase") && !command.contains("pilot") && !command.contains('_'),
            "action must be hierarchical and durable: `{command}`"
        );
        for key in [
            "owner",
            "audience",
            "purpose",
            "inputs",
            "outputs",
            "resume",
            "exit_codes",
            "reference",
        ] {
            required_string(table, key, command);
        }
        assert!(
            commands.insert(command.to_owned()),
            "duplicate registry action `{command}`"
        );
    }
    assert!(commands.contains("assets verify"));
    assert!(commands.contains("starlight map build"));
    assert!(commands.contains("maintenance render-tool-reference"));
    assert!(commands.contains("starlight xp-continuous process-partition"));
    assert!(commands.contains("starlight xp-continuous run-bulk"));
    assert!(commands.contains("starlight product export-contributions"));
    assert_eq!(commands.len(), 21);
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
        let metadata = fs::symlink_metadata(&path)
            .unwrap_or_else(|error| panic!("inspect {}: {error}", path.display()));
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            if matches!(name, ".git" | "target") {
                continue;
            }
            collect_forbidden_files(root, &path, forbidden);
            continue;
        }
        if !metadata.is_file() {
            continue;
        }

        let relative = path
            .strip_prefix(root)
            .expect("repository file must be below root")
            .to_string_lossy()
            .replace('\\', "/");
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
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
        let shell_data_product_wrapper =
            extension == "sh" && relative.starts_with("tools/starlight-xp-continuous/");
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
