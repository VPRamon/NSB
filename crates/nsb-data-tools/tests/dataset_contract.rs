use nsb_data_tools::dataset::{DatasetName, RunConfig};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn four_versioned_configs_are_portable_and_complete() {
    let cases = [
        ("airglow-continuum.toml", DatasetName::AirglowContinuum, 1),
        ("solar-spectrum.toml", DatasetName::SolarSpectrum, 1),
        (
            "moonlight-scattering.toml",
            DatasetName::MoonlightScattering,
            2,
        ),
        ("starlight.toml", DatasetName::Starlight, 1),
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
        assert!(config
            .sources
            .iter()
            .all(|source| source.path.as_ref().is_some_and(|path| path.is_absolute())));
    }
}

#[test]
fn production_starlight_config_declares_the_complete_gaia_pair() {
    let config = RunConfig::load(&crate_root().join("config/starlight-production.toml")).unwrap();
    let starlight = config.starlight.expect("Starlight production policy");
    assert_eq!(
        starlight.mode,
        nsb_data_tools::starlight::config::StarlightMode::Production
    );
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
    for forbidden in ["XpContinuous", "AcquireCommand", "usb", "pilot", "gaiaxpy"] {
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
            } else if matches!(
                path.extension().and_then(|v| v.to_str()),
                Some("py" | "sh" | "bash" | "ipynb")
            ) {
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
    let source = temporary.path().join("solar.csv");
    fs::write(&source, "# wavelength,irradiance\n300,1.0\n400,2.0\n").unwrap();
    let checksum = nsb_data_tools::platform::checksum_io::sha256_file(&source).unwrap();
    let repository = temporary.path().join("repository");
    fs::create_dir_all(repository.join("crates/nsb/data")).unwrap();
    fs::write(
        repository.join("crates/nsb/data/manifest.toml"),
        format!(
            "schema_version = 1\n\n[[assets]]\npath = \"solar_spectrum.dat\"\nsha256 = \"{checksum}\"\ngenerator = \"fixture\"\ngeneration_command = \"fixture\"\n"
        ),
    )
    .unwrap();
    let config = temporary.path().join("run.toml");
    fs::write(
        &config,
        format!(
            "schema_version = 1\ndataset = \"solar-spectrum\"\n\n[workspace]\nroot = \"work\"\n\n[[sources]]\nname = \"solar_spectrum.dat\"\npath = \"{}\"\nsha256 = \"{checksum}\"\n\n[publish]\nrepository_root = \"{}\"\n",
            source.display(),
            repository.display()
        ),
    )
    .unwrap();

    for operation in ["update", "build", "validate", "publish"] {
        command(&config, operation).assert_success();
    }
    assert_eq!(
        fs::read(repository.join("crates/nsb/data/solar_spectrum.dat")).unwrap(),
        fs::read(&source).unwrap()
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
