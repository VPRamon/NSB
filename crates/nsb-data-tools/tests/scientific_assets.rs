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
fn repository_scientific_asset_registry_verify() -> Result<()> {
    let manifest_path = repository_manifest_path();
    verify(&manifest_path)
}

#[test]
fn first_release_registry_excludes_unapproved_starlight_products() -> Result<()> {
    let raw = fs::read_to_string(repository_manifest_path())?;
    let manifest: Manifest = toml::from_str(&raw)?;
    let starlight = manifest
        .assets
        .iter()
        .filter(|asset| asset.path.contains("starlight") || asset.schema.contains("starlight"))
        .map(|asset| asset.path.as_str())
        .collect::<Vec<_>>();
    if !starlight.is_empty() {
        bail!(
            "first public release must not register unapproved Starlight products: {starlight:?}"
        );
    }
    Ok(())
}

fn repository_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/manifest.toml")
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
        verify_asset(base, asset)?;
    }

    // These four frozen files are deliberately retained in Git for scientific
    // reproducibility but are *not* production assets in the runtime registry.
    // Do not silently whitelist them: verify their exact hashes against the
    // independently pinned review bundle/runtime identity before excluding
    // them from the registered production asset set.
    let frozen = verify_frozen_non_runtime_starlight(base)?;
    if !registered.is_disjoint(&frozen) {
        bail!("frozen non-production Starlight is incorrectly registered as runtime data");
    }
    let discovered = discover_assets(base)?;
    let unregistered: Vec<_> = discovered
        .difference(&registered)
        .filter(|path| !frozen.contains(*path))
        .cloned()
        .collect();
    let stale: Vec<_> = registered.difference(&discovered).cloned().collect();
    if !unregistered.is_empty() || !stale.is_empty() {
        bail!("asset registry mismatch; unregistered={unregistered:?}, missing_files={stale:?}");
    }

    Ok(())
}

fn verify_asset(base: &Path, asset: &Asset) -> Result<()> {
    let path = base.join(&asset.path);
    verify_payload(asset, &path)
}

fn verify_payload(asset: &Asset, path: &Path) -> Result<()> {
    let actual = nsb_data_tools::platform::checksum_io::sha256_file(path)
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
        let text = fs::read_to_string(path)
            .with_context(|| format!("{} is not valid UTF-8", asset.path))?;
        verify_header(asset, &text)?;
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

#[test]
fn repository_does_not_embed_restricted_gaia_or_calspec_inputs() -> Result<()> {
    let data = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data");
    let data = data.canonicalize().context("resolve crates/nsb/data")?;
    for entry in fs::read_dir(&data)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        for forbidden in [
            "GaiaSource_",
            "XpContinuousMeanSpectrum_",
            "allsky_M10",
            ".fits",
            ".hdf5",
        ] {
            if name.contains(forbidden) {
                bail!("restricted input {name} must not be embedded under crates/nsb/data");
            }
        }
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct FrozenReviewBundle {
    artifacts: Vec<FrozenReviewArtifact>,
}

#[derive(Debug, Deserialize)]
struct FrozenReviewArtifact {
    id: String,
    path: String,
    sha256: String,
}

#[derive(Debug, Deserialize)]
struct FrozenRuntimeAssets {
    candidate_path: String,
    candidate_sha256: String,
    runtime_map_path: String,
    runtime_map_sha256: String,
    runtime_sidecar_path: String,
    runtime_sidecar_sha256: String,
}

/// Verify the repository-only candidate payloads without making them
/// production-registered data. Fail closed on paths, missing files, or hashes.
fn verify_frozen_non_runtime_starlight(base: &Path) -> Result<BTreeSet<String>> {
    let repository = base.join("../../..");
    let review: FrozenReviewBundle = toml::from_str(&fs::read_to_string(
        repository.join("docs/nsb_components/starlight/release-candidate/review-bundle-v1.toml"),
    )?)?;
    let runtime: FrozenRuntimeAssets = toml::from_str(&fs::read_to_string(
        repository.join("docs/nsb_components/starlight/release-candidate/runtime-assets-v1.toml"),
    )?)?;

    const CANDIDATE: &str = "crates/nsb/data/starlight_nside128.csv";
    const REPORT: &str = "crates/nsb/data/merge_report.json";
    const MAP: &str = "crates/nsb/data/starlight_nside128.release.csv";
    const SIDECAR: &str = "crates/nsb/data/starlight_nside128.manifest.toml";

    if runtime.candidate_path != CANDIDATE
        || runtime.runtime_map_path != MAP
        || runtime.runtime_sidecar_path != SIDECAR
    {
        bail!("frozen runtime identities use unexpected Starlight paths");
    }

    let mut hashes = BTreeMap::new();
    for (id, expected_path) in [("candidate_map", CANDIDATE), ("merge_report", REPORT)] {
        let matches = review
            .artifacts
            .iter()
            .filter(|artifact| artifact.id == id)
            .collect::<Vec<_>>();
        if matches.len() != 1 || matches[0].path != expected_path {
            bail!("frozen review bundle must pin exactly one {id} at {expected_path}");
        }
        hashes.insert(expected_path, matches[0].sha256.as_str());
    }
    if hashes.get(CANDIDATE) != Some(&runtime.candidate_sha256.as_str()) {
        bail!("candidate identity disagrees between frozen review and runtime metadata");
    }
    hashes.insert(MAP, &runtime.runtime_map_sha256);
    hashes.insert(SIDECAR, &runtime.runtime_sidecar_sha256);

    let mut protected = BTreeSet::new();
    for (repository_path, expected_sha256) in hashes {
        if expected_sha256.len() != 64
            || !expected_sha256
                .bytes()
                .all(|value| value.is_ascii_hexdigit())
        {
            bail!("invalid frozen SHA-256 for {repository_path}");
        }
        let relative = repository_path
            .strip_prefix("crates/nsb/data/")
            .context("review bundle references a path outside crates/nsb/data")?;
        let actual = nsb_data_tools::platform::checksum_io::sha256_file(&base.join(relative))?;
        if actual != expected_sha256 {
            bail!(
                "frozen non-runtime Starlight checksum mismatch for {relative}: expected {expected_sha256}, actual {actual}"
            );
        }
        protected.insert(relative.to_string());
    }
    if protected.len() != 4 {
        bail!("expected exactly four frozen Starlight non-runtime files");
    }
    Ok(protected)
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
