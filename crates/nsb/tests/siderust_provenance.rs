// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Siderust dependency provenance must stay truthful with the locked graph.
//!
//! Hard-coded `SIDERUST_VERSION` / `SIDERUST_SOURCE` are intentional public
//! provenance exports (see `docs/developer-guide/public-api.md`). This contract
//! fails if a maintainer bumps the Siderust dependency without updating those
//! constants.

use nsb::{SIDERUST_SOURCE, SIDERUST_VERSION};
use std::fs;
use std::path::PathBuf;

const REVISION: &str = "83d1ebb29ca1ae0b327d5784d13eda231d52c120";

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn declared_siderust_version(manifest: &str) -> Option<String> {
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.split_once('=').map(|(name, _)| name.trim()) != Some("siderust") {
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

fn locked_siderust_package(lockfile: &str) -> Option<(String, String)> {
    let package = lockfile.split("[[package]]").find(|package| {
        package
            .lines()
            .any(|line| line.trim() == "name = \"siderust\"")
    })?;

    Some((
        package_field(package, "version")?,
        package_field(package, "source")?,
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

    let nsb_version = declared_siderust_version(&nsb_manifest).expect("nsb siderust version");
    let cli_version = declared_siderust_version(&cli_manifest).expect("cli siderust version");
    let tools_version = declared_siderust_version(&tools_manifest).expect("tools siderust version");
    let (locked_version, locked_source) =
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
        SIDERUST_VERSION, locked_version,
        "nsb::SIDERUST_VERSION must match the declared Siderust version"
    );
    assert_eq!(
        locked_source,
        format!("git+https://github.com/Siderust/siderust?rev={REVISION}#{REVISION}")
    );
    assert_eq!(
        SIDERUST_SOURCE,
        format!("git:https://github.com/Siderust/siderust@{REVISION}")
    );
    let workspace: toml::Value =
        toml::from_str(&fs::read_to_string(root.join("Cargo.toml")).unwrap()).unwrap();
    assert_eq!(
        workspace["patch"]["crates-io"]["siderust"]["rev"].as_str(),
        Some(REVISION)
    );
    assert_eq!(
        workspace["patch"]["crates-io"]["siderust"]["git"].as_str(),
        Some("https://github.com/Siderust/siderust")
    );
    let packages: toml::Value = toml::from_str(&lockfile).unwrap();
    assert_eq!(
        packages["package"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["name"].as_str() == Some("siderust"))
            .count(),
        1
    );
}
