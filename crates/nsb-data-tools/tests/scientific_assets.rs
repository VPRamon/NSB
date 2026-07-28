use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

const LFS_POINTER_VERSION: &str = "version https://git-lfs.github.com/spec/v1";

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: u32,
    assets: Vec<Asset>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum Storage {
    #[default]
    Git,
    GitLfs,
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
    storage: Storage,
    bytes: Option<u64>,
    #[serde(default)]
    header: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VerificationMode {
    Registry,
    Payload,
}

#[derive(Debug, PartialEq, Eq)]
struct LfsPointer {
    oid_sha256: String,
    size: u64,
}

#[test]
fn repository_scientific_asset_registry_verify() -> Result<()> {
    let manifest_path = repository_manifest_path();
    verify(&manifest_path, VerificationMode::Registry)
}

#[test]
#[ignore = "requires complete Git LFS payloads"]
fn repository_scientific_asset_payloads_verify() -> Result<()> {
    let manifest_path = repository_manifest_path();
    verify(&manifest_path, VerificationMode::Payload)
}

fn repository_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nsb/data/manifest.toml")
}

fn verify(manifest_path: &Path, mode: VerificationMode) -> Result<()> {
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
        verify_asset(base, asset, mode)?;
    }

    let discovered = discover_assets(base)?;
    if registered != discovered {
        let missing: Vec<_> = discovered.difference(&registered).cloned().collect();
        let stale: Vec<_> = registered.difference(&discovered).cloned().collect();
        bail!("asset registry mismatch; unregistered={missing:?}, missing_files={stale:?}");
    }

    Ok(())
}

fn verify_asset(base: &Path, asset: &Asset, mode: VerificationMode) -> Result<()> {
    let path = base.join(&asset.path);
    let pointer = read_lfs_pointer(&path)
        .with_context(|| format!("failed to inspect registered asset {}", path.display()))?;

    match (asset.storage, pointer) {
        (Storage::Git, Some(_)) => {
            bail!(
                "{} is a Git LFS pointer but manifest storage is not git-lfs",
                asset.path
            );
        }
        (Storage::Git, None) => verify_payload(asset, &path),
        (Storage::GitLfs, Some(pointer)) if mode == VerificationMode::Registry => {
            verify_lfs_pointer(asset, &pointer)
        }
        (Storage::GitLfs, Some(_)) => {
            bail!(
                "{} is still a Git LFS pointer; fetch the payload before payload verification",
                asset.path
            );
        }
        (Storage::GitLfs, None) => verify_payload(asset, &path),
    }
}

fn verify_payload(asset: &Asset, path: &Path) -> Result<()> {
    if let Some(expected_bytes) = asset.bytes {
        let actual_bytes = path
            .metadata()
            .with_context(|| format!("failed to stat registered asset {}", path.display()))?
            .len();
        if actual_bytes != expected_bytes {
            bail!(
                "size mismatch for {}: manifest {}, actual {}",
                asset.path,
                expected_bytes,
                actual_bytes
            );
        }
    }

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

fn verify_lfs_pointer(asset: &Asset, pointer: &LfsPointer) -> Result<()> {
    if pointer.oid_sha256 != asset.sha256 {
        bail!(
            "Git LFS OID mismatch for {}: manifest {}, pointer {}",
            asset.path,
            asset.sha256,
            pointer.oid_sha256
        );
    }
    let expected_bytes = asset
        .bytes
        .context("git-lfs assets must declare their payload byte size")?;
    if pointer.size != expected_bytes {
        bail!(
            "Git LFS size mismatch for {}: manifest {}, pointer {}",
            asset.path,
            expected_bytes,
            pointer.size
        );
    }
    Ok(())
}

fn read_lfs_pointer(path: &Path) -> Result<Option<LfsPointer>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut lines = BufReader::new(file).lines();
    let Some(first) = lines.next() else {
        return Ok(None);
    };
    if first? != LFS_POINTER_VERSION {
        return Ok(None);
    }

    let oid_line = lines
        .next()
        .transpose()?
        .context("Git LFS pointer is missing its oid line")?;
    let size_line = lines
        .next()
        .transpose()?
        .context("Git LFS pointer is missing its size line")?;
    if lines.next().transpose()?.is_some() {
        bail!("Git LFS pointer contains unexpected trailing content");
    }

    let oid_sha256 = oid_line
        .strip_prefix("oid sha256:")
        .context("Git LFS pointer has an invalid oid line")?;
    if oid_sha256.len() != 64
        || !oid_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("Git LFS pointer has an invalid SHA-256 OID");
    }
    let size = size_line
        .strip_prefix("size ")
        .context("Git LFS pointer has an invalid size line")?
        .parse::<u64>()
        .context("Git LFS pointer size is not an unsigned integer")?;

    Ok(Some(LfsPointer {
        oid_sha256: oid_sha256.to_string(),
        size,
    }))
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
    if asset.storage == Storage::GitLfs && asset.bytes.is_none() {
        bail!(
            "git-lfs asset {:?} must declare its payload byte size",
            asset.path
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn registry_accepts_a_valid_lfs_pointer_without_downloading_payload() {
        let fixture = Fixture::new(DIGEST, 42);
        verify(&fixture.manifest_path, VerificationMode::Registry).unwrap();
    }

    #[test]
    fn registry_rejects_an_lfs_pointer_with_the_wrong_size() {
        let fixture = Fixture::new(DIGEST, 41);
        let error = verify(&fixture.manifest_path, VerificationMode::Registry).unwrap_err();
        assert!(error.to_string().contains("Git LFS size mismatch"));
    }

    #[test]
    fn payload_verification_rejects_an_unfetched_lfs_pointer() {
        let fixture = Fixture::new(DIGEST, 42);
        let error = verify(&fixture.manifest_path, VerificationMode::Payload).unwrap_err();
        assert!(error.to_string().contains("fetch the payload"));
    }

    #[test]
    fn malformed_lfs_pointer_fails_with_an_actionable_diagnostic() {
        let fixture = Fixture::new_with_pointer(
            DIGEST,
            42,
            &format!("{LFS_POINTER_VERSION}\noid sha256:not-a-digest\nsize 42\n"),
        );
        let error = verify(&fixture.manifest_path, VerificationMode::Registry).unwrap_err();
        assert!(format!("{error:#}").contains("invalid SHA-256 OID"));
    }

    struct Fixture {
        _root: TempDir,
        manifest_path: PathBuf,
    }

    impl Fixture {
        fn new(digest: &str, manifest_bytes: u64) -> Self {
            let pointer = format!("{LFS_POINTER_VERSION}\noid sha256:{digest}\nsize 42\n");
            Self::new_with_pointer(digest, manifest_bytes, &pointer)
        }

        fn new_with_pointer(digest: &str, manifest_bytes: u64, pointer: &str) -> Self {
            let root = tempfile::tempdir().unwrap();
            let manifest_path = root.path().join("manifest.toml");
            let asset_path = root.path().join("fixture.csv");
            File::create(&asset_path)
                .unwrap()
                .write_all(pointer.as_bytes())
                .unwrap();
            fs::write(
                &manifest_path,
                format!(
                    r#"schema_version = 1

[[assets]]
path = "fixture.csv"
schema = "fixture-v1"
sha256 = "{digest}"
source = "deterministic test fixture"
license = "test-only"
generator = "scientific_assets.rs"
generation_command = "generated in test"
validation_report = "test"
calibration_status = "test"
runtime_embedded = true
storage = "git-lfs"
bytes = {manifest_bytes}
"#
                ),
            )
            .unwrap();
            Self {
                _root: root,
                manifest_path,
            }
        }
    }
}
