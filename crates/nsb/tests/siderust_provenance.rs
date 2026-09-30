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
    let nsb_branch = declared_siderust_field(&nsb_manifest, "branch").expect("nsb siderust branch");
    let cli_branch = declared_siderust_field(&cli_manifest, "branch").expect("cli siderust branch");
    let tools_branch =
        declared_siderust_field(&tools_manifest, "branch").expect("tools siderust branch");
    let locked_version = locked_siderust_version(&lockfile).expect("locked siderust package");
    let locked_source = locked_siderust_source(&lockfile).expect("locked siderust source");

    assert_eq!(
        nsb_version, cli_version,
        "workspace crates must declare the same Siderust version"
    );
    assert_eq!(
        nsb_version, tools_version,
        "workspace crates must declare the same Siderust version"
    );
    assert_eq!(
        nsb_version, locked_version,
        "Cargo.lock Siderust version must match the manifests"
    );
    assert_eq!(
        SIDERUST_VERSION, nsb_version,
        "nsb::SIDERUST_VERSION must match the declared Siderust version"
    );

    assert_eq!(
        nsb_branch, cli_branch,
        "workspace crates must track the same Siderust branch"
    );
    assert_eq!(
        nsb_branch, tools_branch,
        "workspace crates must track the same Siderust branch"
    );

    let locked_prefix = format!("git+https://github.com/Siderust/siderust?branch={nsb_branch}#");
    let locked_commit = locked_source
        .strip_prefix(&locked_prefix)
        .expect("Cargo.lock must resolve the declared Siderust branch");
    assert_eq!(
        locked_commit.len(),
        40,
        "Cargo.lock must pin a full Siderust Git commit hash"
    );
    assert!(
        locked_commit
            .chars()
            .all(|character| character.is_ascii_hexdigit()),
        "Cargo.lock Siderust commit must be hexadecimal"
    );
    assert_eq!(
        SIDERUST_SOURCE,
        format!("git:https://github.com/Siderust/siderust?branch={nsb_branch}"),
        "nsb::SIDERUST_SOURCE must identify the tracked Git branch"
    );
}
