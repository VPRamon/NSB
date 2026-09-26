use nsb_data_tools::dataset::{DatasetName, RunConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn versioned_source_configs_are_portable_and_complete() {
    let cases = [
        ("airglow-continuum.toml", DatasetName::AirglowContinuum, 1),
        ("solar-spectrum.toml", DatasetName::SolarSpectrum, 2),
        (
            "moonlight-scattering.toml",
            DatasetName::MoonlightScattering,
            2,
        ),
    ];
    for (name, dataset, sources) in cases {
        let config =
            RunConfig::load(&crate_root().join("config").join(name)).expect("valid config");
        assert_eq!(config.dataset, dataset);
        assert_eq!(config.sources.len(), sources);
        assert!(config.workspace.root.is_absolute());
        assert!(!config
            .workspace
            .root
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir)));
        assert!(config.sources.iter().all(|source| {
            source.path.as_ref().is_some_and(|path| path.is_absolute())
                || source
                    .url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("https://"))
        }));
    }
}

#[test]
fn production_starlight_config_declares_the_complete_gaia_pair() {
    let config = RunConfig::load(&crate_root().join("config/starlight-production.toml")).unwrap();
    assert!(config.sources.is_empty());
    let starlight = config.starlight.expect("Starlight production policy");
    assert_eq!(
        starlight.product_band,
        nsb_data_tools::starlight::config::StarlightProductBand::Measured336To650
    );
    assert!(starlight.ultraviolet_correction.is_none());
    assert_eq!(
        starlight
            .gaia_products
            .iter()
            .map(|product| product.id.as_str())
            .collect::<Vec<_>>(),
        ["gaia-source", "xp-continuous"]
    );
    assert!(starlight
        .gaia_products
        .iter()
        .all(|product| product.expected_partitions == Some(3386)));
}

#[test]
fn only_dataset_oriented_cli_is_exposed() {
    let cli = fs::read_to_string(crate_root().join("src/cli/mod.rs")).unwrap();
    for required in ["Dataset", "Run", "Update", "Build", "Validate", "Publish"] {
        assert!(cli.contains(required), "missing {required}");
    }
    for forbidden in [
        "XpContinuous",
        "AcquireCommand",
        "usb",
        "pilot",
        "gaiaxpy",
        "skip_completed_from",
        "skip-completed-from",
    ] {
        assert!(!cli.to_lowercase().contains(&forbidden.to_lowercase()));
    }
}

#[test]
fn maintained_code_has_no_python_shell_usb_or_recursive_cargo_orchestration() {
    let root = crate_root().join("src");
    let maintained = [
        root.join("bin/nsb-data.rs"),
        root.join("cli/mod.rs"),
        root.join("dataset/config.rs"),
        root.join("dataset/engine.rs"),
        root.join("dataset/model.rs"),
        root.join("dataset/slurm.rs"),
    ];
    for path in maintained {
        let text = fs::read_to_string(&path).unwrap();
        for forbidden in [
            "Command::new(\"cargo\")",
            "python",
            "gaiaxpy_environment",
            "usb_mount",
        ] {
            assert!(
                !text.to_lowercase().contains(&forbidden.to_lowercase()),
                "{} contains {forbidden}",
                path.display()
            );
        }
    }
}

#[test]
fn no_tracked_python_or_shell_programs_exist() {
    fn is_forbidden_extension(path: &Path) -> bool {
        matches!(
            path.extension().and_then(|v| v.to_str()),
            Some("py" | "sh" | "bash" | "zsh" | "fish" | "ipynb" | "ps1" | "cmd" | "bat")
        )
    }

    fn has_forbidden_shebang(path: &Path) -> bool {
        let Ok(bytes) = fs::read(path) else {
            return false;
        };
        // Skip large/binary assets; shebang interpreters fit in the first line.
        if bytes.len() > 256 * 1024 || bytes.contains(&0) {
            return false;
        }
        let Ok(text) = std::str::from_utf8(&bytes) else {
            return false;
        };
        let Some(first) = text.lines().next() else {
            return false;
        };
        let Some(rest) = first.strip_prefix("#!") else {
            return false;
        };
        let rest = rest.trim();
        // `#!/usr/bin/env bash`, `#!/bin/sh`, `#!/usr/bin/python3`, …
        let interpreter = rest
            .strip_prefix("/usr/bin/env ")
            .or_else(|| rest.strip_prefix("/bin/env "))
            .unwrap_or(rest)
            .split_whitespace()
            .next()
            .unwrap_or("");
        let name = Path::new(interpreter)
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or(interpreter);
        matches!(
            name,
            "sh" | "bash" | "zsh" | "dash" | "fish" | "python" | "python2" | "python3" | "ipython"
        )
    }

    fn visit(path: &Path, found: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                if !matches!(
                    path.file_name().and_then(|v| v.to_str()),
                    Some(".git" | "target" | ".venv" | "__pycache__" | ".pytest_cache")
                ) {
                    visit(&path, found);
                }
            } else if is_forbidden_extension(&path) || has_forbidden_shebang(&path) {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    visit(&crate_root().join("../.."), &mut found);
    assert!(found.is_empty(), "non-Rust programs remain: {found:#?}");
}

#[test]
fn lifecycle_publishes_only_unchanged_validated_bytes() {
    let temporary = tempfile::tempdir().unwrap();
    let source = temporary.path().join("candidate.csv");
    let native = temporary.path().join("native.csv");
    let source_bytes = "wavelength (nm),irradiance (W/m^2/nm)\n\
300.000,1.0\n445.000,1.8\n500.000,2.0\n551.000,1.7\n650.000,1.5\n";
    fs::write(&source, source_bytes).unwrap();
    fs::write(&native, source_bytes).unwrap();
    let source_checksum = nsb_data_tools::platform::checksum_io::sha256_file(&source).unwrap();
    let native_checksum = nsb_data_tools::platform::checksum_io::sha256_file(&native).unwrap();
    let repository = temporary.path().join("repository");
    fs::create_dir_all(repository.join("crates/nsb/data")).unwrap();
    let config = temporary.path().join("run.toml");
    fs::write(
        &config,
        format!(
            "schema_version = 1\ndataset = \"solar-spectrum\"\n\n[workspace]\nroot = \"work\"\n\n[[sources]]\nname = \"tsis1_hsrs_p025nm_300_650.csv\"\npath = \"{}\"\nsha256 = \"{source_checksum}\"\nproduct_id = \"tsis1_hsrs_p025nm\"\nrelease = \"TSIS-1 HSRS Version 2\"\nmetadata_url = \"https://doi.org/10.25980/ta3f-7h90\"\nretrieved_at = \"fixture\"\nlicense = \"fixture\"\nunits = \"W m^-2 nm^-1\"\nreference_distance = \"1 AU\"\n\n[[sources]]\nname = \"tsis1_hsrs_native_300_650.csv\"\npath = \"{}\"\nsha256 = \"{native_checksum}\"\nproduct_id = \"tsis1_hsrs\"\nrelease = \"TSIS-1 HSRS Version 2\"\nmetadata_url = \"https://doi.org/10.25980/ta3f-7h90\"\nretrieved_at = \"fixture\"\nlicense = \"fixture\"\nunits = \"W m^-2 nm^-1\"\nreference_distance = \"1 AU\"\n\n[publish]\nrepository_root = \"{}\"\n",
            source.display(),
            native.display(),
            repository.display()
        ),
    )
    .unwrap();

    command(&config, "update").assert_success();
    command(&config, "build").assert_success();
    let output_path = temporary.path().join("work/outputs/solar_spectrum.dat");
    let first_build = fs::read(&output_path).unwrap();
    command(&config, "build").assert_success();
    assert_eq!(fs::read(&output_path).unwrap(), first_build);

    let expected_checksum =
        nsb_data_tools::platform::checksum_io::sha256_file(&output_path).unwrap();
    fs::write(
        repository.join("crates/nsb/data/manifest.toml"),
        format!(
            "schema_version = 1\n\n[[assets]]\npath = \"solar_spectrum.dat\"\nsha256 = \"{expected_checksum}\"\ngenerator = \"fixture\"\ngeneration_command = \"fixture\"\n"
        ),
    )
    .unwrap();
    command(&config, "validate").assert_success();
    command(&config, "publish").assert_success();
    assert_eq!(
        fs::read(repository.join("crates/nsb/data/solar_spectrum.dat")).unwrap(),
        first_build
    );

    fs::write(
        temporary.path().join("work/outputs/solar_spectrum.dat"),
        "tampered\n",
    )
    .unwrap();
    command(&config, "publish").assert_failure();
}

fn command(config: &Path, operation: &str) -> CommandResult {
    let output = Command::new(env!("CARGO_BIN_EXE_nsb-data"))
        .args([
            "dataset",
            "solar-spectrum",
            operation,
            "--config",
            config.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    CommandResult(output)
}

struct CommandResult(std::process::Output);

impl CommandResult {
    fn assert_success(self) {
        assert!(
            self.0.status.success(),
            "command failed: {}",
            String::from_utf8_lossy(&self.0.stderr)
        );
    }

    fn assert_failure(self) {
        assert!(!self.0.status.success(), "command unexpectedly succeeded");
    }
}
