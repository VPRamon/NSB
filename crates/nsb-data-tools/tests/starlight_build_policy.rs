use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

#[test]
fn bundled_starlight_checksums_are_verified_during_build_script() -> Result<()> {
    let build_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/build_support");
    let validate = fs::read_to_string(build_dir.join("validate.rs"))
        .with_context(|| format!("failed to read {}", build_dir.join("validate.rs").display()))?;
    let generate = fs::read_to_string(build_dir.join("generate.rs"))
        .with_context(|| format!("failed to read {}", build_dir.join("generate.rs").display()))?;
    let entry = fs::read_to_string(build_dir.join("mod.rs"))
        .with_context(|| format!("failed to read {}", build_dir.join("mod.rs").display()))?;

    for (label, source) in [
        ("validate.rs", validate.as_str()),
        ("generate.rs", generate.as_str()),
        ("mod.rs", entry.as_str()),
    ] {
        if source.contains("siderust::assert_data_checksum!") {
            bail!(
                "crates/nsb/build_support/{label} must verify bundled Starlight checksums during build-script execution, not through rustc const evaluation"
            );
        }
    }

    for required in [
        "validate_runtime_embedded_files",
        "hex_sha256",
        "Sha256::digest",
        "select_production_starlight",
        "generate_bundled_assets_rs",
    ] {
        let present =
            validate.contains(required) || generate.contains(required) || entry.contains(required);
        if !present {
            bail!("crates/nsb/build_support is missing the build-time checksum guard: {required}");
        }
    }

    Ok(())
}
