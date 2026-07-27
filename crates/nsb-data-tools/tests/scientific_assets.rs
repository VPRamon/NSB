use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    assets: Vec<Asset>,
}

#[derive(Debug, Deserialize)]
struct Asset {
    path: String,
    schema: String,
    sha256: String,
    source: String,
    license: String,
    generator: String,
    generation_command: String,
    validation_report: String,
    calibration_status: String,
    runtime_embedded: bool,
    #[serde(default)]
    header: BTreeMap<String, String>,
}

#[test]
fn repository_scientific_assets_verify() -> Result<()> {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../nsb/data/manifest.toml");
    verify(&manifest_path)
}

fn verify(manifest_path: &Path) -> Result<()> {
    let raw = fs::read_to_string(manifest_path)
        .with_context(|| format!("failed to read {}", manifest_path.display()))?;
    let manifest: Manifest = toml::from_str(&raw)
        .with_context(|| format!("failed to parse {}", manifest_path.display()))?;
    if manifest.schema_version != 1 {
        bail!(
            "unsupported asset manifest schema {}",
            manifest.schema_version
        );
    }
    if manifest.assets.is_empty() {
        bail!("asset manifest contains no assets");
    }
    if !manifest.assets.iter().any(|asset| asset.runtime_embedded) {
        bail!("asset manifest contains no runtime-embedded assets");
    }

    let base = manifest_path
        .parent()
        .context("manifest path has no parent directory")?;
    let mut registered = BTreeSet::new();
    for asset in &manifest.assets {
        validate_required_fields(asset)?;
        if !registered.insert(asset.path.clone()) {
            bail!("duplicate asset path {:?}", asset.path);
        }
        let path = base.join(&asset.path);
        let actual = nsb_data_tools::platform::checksum_io::sha256_file(&path)
            .with_context(|| format!("failed to checksum registered asset {}", path.display()))?;
        if actual != asset.sha256 {
            bail!(
                "checksum mismatch for {}: manifest {}, actual {}",
                asset.path,
                asset.sha256,
                actual
            );
        }
        if !asset.header.is_empty() {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("{} is not valid UTF-8", asset.path))?;
            verify_header(asset, &text)?;
        }
    }

    let discovered = discover_assets(base)?;
    if registered != discovered {
        let missing: Vec<_> = discovered.difference(&registered).cloned().collect();
        let stale: Vec<_> = registered.difference(&discovered).cloned().collect();
        bail!("asset registry mismatch; unregistered={missing:?}, missing_files={stale:?}");
    }

    Ok(())
}

fn validate_required_fields(asset: &Asset) -> Result<()> {
    let fields = [
        ("path", asset.path.as_str()),
        ("schema", asset.schema.as_str()),
        ("sha256", asset.sha256.as_str()),
        ("source", asset.source.as_str()),
        ("license", asset.license.as_str()),
        ("generator", asset.generator.as_str()),
        ("generation_command", asset.generation_command.as_str()),
        ("validation_report", asset.validation_report.as_str()),
        ("calibration_status", asset.calibration_status.as_str()),
    ];
    for (name, value) in fields {
        if value.trim().is_empty() {
            bail!("asset {:?} has empty required field {name}", asset.path);
        }
    }
    if asset.sha256.len() != 64
        || !asset
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!(
            "asset {:?} has invalid SHA-256 {:?}",
            asset.path,
            asset.sha256
        );
    }
    Ok(())
}

fn verify_header(asset: &Asset, text: &str) -> Result<()> {
    let actual: BTreeMap<_, _> = text
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with('#'))
        .filter_map(|line| line.trim_start_matches('#').trim().split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect();
    for (key, expected) in &asset.header {
        match actual.get(key.as_str()) {
            Some(value) if *value == expected => {}
            Some(value) => bail!(
                "header mismatch for {} key {}: manifest {:?}, asset {:?}",
                asset.path,
                key,
                expected,
                value
            ),
            None => bail!("{} is missing required header key {}", asset.path, key),
        }
    }
    Ok(())
}

fn discover_assets(base: &Path) -> Result<BTreeSet<String>> {
    let mut files = BTreeSet::new();
    discover_recursive(base, base, &mut files)?;
    files.remove("manifest.toml");
    Ok(files)
}

fn discover_recursive(base: &Path, directory: &Path, files: &mut BTreeSet<String>) -> Result<()> {
    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to list {}", directory.display()))?
    {
        let path = entry?.path();
        if path.is_dir() {
            discover_recursive(base, &path, files)?;
        } else if path.is_file() {
            files.insert(
                path.strip_prefix(base)?
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/"),
            );
        }
    }
    Ok(())
}
