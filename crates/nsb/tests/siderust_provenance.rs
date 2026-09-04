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

fn declared_siderust_version(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let trimmed = line.trim();
        // siderust = "0.11.1"
        if let Some(rest) = trimmed.strip_prefix("siderust = \"") {
            return Some(rest.trim_end_matches('"').to_string());
        }
        // siderust = { version = "0.11.1", ... }
        if trimmed.starts_with("siderust = {") {
            let Some(after) = trimmed.split("version = \"").nth(1) else {
                continue;
            };
            let version = after.split('"').next()?.to_string();
            if !version.is_empty() {
                return Some(version);
            }
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

    let nsb = declared_siderust_version(&nsb_manifest).expect("nsb siderust declaration");
    let cli = declared_siderust_version(&cli_manifest).expect("cli siderust declaration");
    let tools = declared_siderust_version(&tools_manifest).expect("tools siderust declaration");
    let locked = locked_siderust_version(&lockfile).expect("locked siderust package");

    assert_eq!(
        nsb, cli,
        "workspace crates must declare the same Siderust version"
    );
    assert_eq!(
        nsb, tools,
        "workspace crates must declare the same Siderust version"
    );
    assert_eq!(
        nsb, locked,
        "Cargo.lock siderust package must match crates/nsb/Cargo.toml"
    );
    assert_eq!(
        SIDERUST_VERSION,
        locked.as_str(),
        "nsb::SIDERUST_VERSION must match the locked siderust package version"
    );
    assert_eq!(
        SIDERUST_SOURCE,
        format!("crates.io:siderust:{locked}"),
        "nsb::SIDERUST_SOURCE must be the crates.io identity for the locked package"
    );
}
