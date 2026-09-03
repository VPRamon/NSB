use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;

#[test]
fn bundled_starlight_checksums_are_not_const_evaluated() -> Result<()> {
    let build_script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/build.rs");
    let source = fs::read_to_string(&build_script)
        .with_context(|| format!("failed to read {}", build_script.display()))?;

    if source.contains("siderust::assert_data_checksum!") {
        bail!(
            "crates/nsb/build.rs must verify bundled Starlight checksums during build-script execution, not through rustc const evaluation"
        );
    }

    for required in [
        "verify_registered_file_checksum(&data_dir, map)",
        "verify_registered_file_checksum(&data_dir, manifest)",
        "Sha256::digest",
    ] {
        if !source.contains(required) {
            bail!("crates/nsb/build.rs is missing the build-time checksum guard: {required}");
        }
    }

    Ok(())
}
