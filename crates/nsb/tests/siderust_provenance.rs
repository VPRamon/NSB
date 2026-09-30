//! Siderust dependency provenance must stay truthful with the locked graph.
//!
//! Hard-coded `SIDERUST_VERSION` / `SIDERUST_SOURCE` are intentional public
//! provenance exports (see `docs/developer-guide/public-api.md`). This contract
//! fails if a maintainer bumps the Siderust dependency without updating those
//! constants.

use nsb::{SIDERUST_SOURCE, SIDERUST_VERSION};
use std::fs;
use std::path::PathBuf;

const CRATES_IO_REGISTRY_SOURCE: &str =
    "registry+https://github.com/rust-lang/crates.io-index";
const SIDERUST_0_12_0_CHECKSUM: &str =
    "9401ba8b70cfe3abd8b8a70e8ad1d2fbd8508dc487c46f4fe9a3db8e7314a938";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn declared_siderust_version(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("siderust") {
            continue;
        }

        let declaration = trimmed.split_once('=')?.1.trim();

        if let Some(version) = declaration
            .strip_prefix('"')
            .and_then(|value| value.split('"').next())
        {
            if !version.is_empty() {
                return Some(version.to_string());
            }
        }

        let marker = "version = \"";
        let after = declaration.split(marker).nth(1)?;
        let version = after.split('"').next()?;
        if !version.is_empty() {
            return Some(version.to_string());
        }
    }
    None
}

fn package_field(package: &str, field: &str) -> Option<String> {
    let prefix = format!("{field} = \"");
    package.lines().find_map(|line| {
        line.trim()
            .strip_prefix(&prefix)?
            .strip_suffix('"')
            .map(str::to_owned)
    })
}

fn locked_siderust_package(lockfile: &str) -> Option<(String, String, String)> {
    let package = lockfile.split("[[package]]").find(|package| {
        package
            .lines()
            .any(|line| line.trim() == "name = \"siderust\"")
    })?;

    Some((
        package_field(package, "version")?,
        package_field(package, "source")?,
        package_field(package, "checksum")?,
    ))
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
        declared_siderust_version(&nsb_manifest).expect("nsb siderust version");
    let cli_version =
        declared_siderust_version(&cli_manifest).expect("cli siderust version");
    let tools_version =
        declared_siderust_version(&tools_manifest).expect("tools siderust version");
    let (locked_version, locked_source, locked_checksum) =
        locked_siderust_package(&lockfile).expect("locked siderust package");

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
        locked_source, CRATES_IO_REGISTRY_SOURCE,
        "Cargo.lock must resolve Siderust from crates.io"
    );
    assert_eq!(
        locked_checksum, SIDERUST_0_12_0_CHECKSUM,
        "Cargo.lock must contain the published Siderust 0.12.0 checksum"
    );
    assert_eq!(
        SIDERUST_SOURCE,
        format!("crates.io:siderust:{nsb_version}"),
        "nsb::SIDERUST_SOURCE must identify the crates.io package"
    );
}
