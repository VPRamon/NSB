//! Siderust dependency provenance must stay truthful with the locked graph.
//!
//! Hard-coded `SIDERUST_VERSION` / `SIDERUST_SOURCE` are intentional public
//! provenance exports (see `docs/developer-guide/public-api.md`). This contract
//! fails if a maintainer bumps the Siderust dependency without updating those
//! constants.

use nsb::{SIDERUST_SOURCE, SIDERUST_VERSION};
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn declared_siderust_field(manifest: &str, field: &str) -> Option<String> {
    let marker = format!("{field} = \"");
    for line in manifest.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("siderust = {") {
            continue;
        }
        let after = trimmed.split(&marker).nth(1)?;
        let value = after.split('"').next()?.to_string();
        if !value.is_empty() {
            return Some(value);
        }
    }
    None
}

fn locked_siderust_version(lockfile: &str) -> Option<String> {
    let mut lines = lockfile.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "name = \"siderust\"" {
            continue;
        }
        let version_line = lines.next()?.trim();
        let version = version_line
            .strip_prefix("version = \"")?
            .strip_suffix('"')?
            .to_string();
        return Some(version);
    }
    None
}

fn locked_siderust_source(lockfile: &str) -> Option<String> {
    let mut lines = lockfile.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim() != "name = \"siderust\"" {
            continue;
        }
        lines.next()?;
        return lines
            .next()?
            .trim()
            .strip_prefix("source = \"")?
            .strip_suffix('"')
            .map(str::to_owned);
    }
    None
}

#[test]
fn siderust_provenance_matches_manifest_and_lockfile() {
    let root = workspace_root();
    let nsb_manifest =
        fs::read_to_string(root.join("crates/nsb/Cargo.toml")).expect("nsb Cargo.toml");
    let cli_manifest =
        fs::read_to_string(root.join("crates/nsb-cli/Cargo.toml")).expect("cli Cargo.toml");
    let tools_manifest = fs::read_to_string(root.join("crates/nsb-data-tools/Cargo.toml"))
        .expect("data-tools Cargo.toml");
    let lockfile = fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");

    let nsb_version =
        declared_siderust_field(&nsb_manifest, "version").expect("nsb siderust version");
    let cli_version =
        declared_siderust_field(&cli_manifest, "version").expect("cli siderust version");
    let tools_version =
        declared_siderust_field(&tools_manifest, "version").expect("tools siderust version");
    let nsb_rev = declared_siderust_field(&nsb_manifest, "rev").expect("nsb siderust revision");
    let cli_rev = declared_siderust_field(&cli_manifest, "rev").expect("cli siderust revision");
    let tools_rev =
        declared_siderust_field(&tools_manifest, "rev").expect("tools siderust revision");
    let locked_version = locked_siderust_version(&lockfile).expect("locked siderust package");
    let locked_source = locked_siderust_source(&lockfile).expect("locked siderust source");

    assert_eq!(nsb_version, cli_version, "workspace crates must declare the same Siderust version");
    assert_eq!(nsb_version, tools_version, "workspace crates must declare the same Siderust version");
    assert_eq!(nsb_version, locked_version, "Cargo.lock Siderust version must match the manifests");
    assert_eq!(SIDERUST_VERSION, nsb_version, "nsb::SIDERUST_VERSION must match the declared Siderust version");

    assert_eq!(nsb_rev, cli_rev, "workspace crates must pin the same Siderust revision");
    assert_eq!(nsb_rev, tools_rev, "workspace crates must pin the same Siderust revision");
    assert_eq!(
        locked_source,
        format!("git+https://github.com/Siderust/siderust?rev={nsb_rev}#{nsb_rev}"),
        "Cargo.lock must pin the declared Siderust git revision"
    );
    assert_eq!(
        SIDERUST_SOURCE,
        format!("git:https://github.com/Siderust/siderust?rev={nsb_rev}"),
        "nsb::SIDERUST_SOURCE must identify the pinned git revision"
    );
}
