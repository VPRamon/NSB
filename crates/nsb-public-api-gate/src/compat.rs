//! Guard against deliberately removed compatibility-only symbols.

use std::fs;
use std::path::{Path, PathBuf};

use thiserror::Error;

const FORBIDDEN_PATTERNS: &[&str] = &[
    "ALL_SUPPORTED",
    "python_parity",
    "periods_below_threshold_legacy",
    "#[deprecated]",
    "pub enum StarlightModel {",
    "pub fn with_starlight_model",
    "pub starlight_model:",
];

/// Debt patterns forbidden specifically in production Airglow implementation.
const AIRGLOW_FORBIDDEN_PATTERNS: &[&str] = &[
    "#[allow(dead_code)]",
    "AirglowScientificProfile",
    "with_f10_7",
    "LegacyDefault",
];

const MOONLIGHT_PUBLIC_IMPL_PATTERNS: &[&str] = &[
    "pub struct Jones2013Spectral",
    "pub struct KrisciunasSchaefer1991",
    "pub struct MoonOutputs",
    "pub const DEFAULT_K_EXT",
    "pub const DEFAULT_PERIOD_SEARCH_STEP",
    "pub fn with_extinction_scale",
    "pub fn periods_in_range",
];
const STARLIGHT_PUBLIC_IMPL_PATTERNS: &[&str] = &[
    "pub struct Starlight {",
    "pub struct StarlightOutputs {",
    "pub use model::Starlight;",
    "pub use output::StarlightOutputs;",
];

const ZODIACAL_PUBLIC_IMPL_PATTERNS: &[&str] = &[
    "pub struct ZodiacalLight {",
    "pub struct ZodiacalOutputs {",
    "pub struct ZodiacalSpectrum {",
    "pub enum ZodiacalBrightnessModel {",
    "pub struct ZodiacalBrightnessGrid {",
    "pub fn with_solar_spectrum(",
    "pub fn with_brightness_model(",
    "pub use model::ZodiacalLight;",
    "pub use output::ZodiacalOutputs;",
    "pub use output::{ZodiacalOutputs",
];

const ZODIACAL_ROOT_PUBLIC_IMPL_PATTERNS: &[&str] = &[
    "ZodiacalBrightnessGrid,",
    "ZodiacalBrightnessModel,",
    "ZodiacalLight,",
    "ZodiacalOutputs,",
    "ZodiacalSpectrum,",
    "pub use components::zodiacal::ZodiacalLight;",
    "pub use components::zodiacal::ZodiacalOutputs;",
    "pub use components::zodiacal::ZodiacalSpectrum;",
];

#[derive(Debug, Error)]
pub enum CompatError {
    #[error("removed or compatibility-only API found in production source:\n{0}")]
    Found(String),
    #[error("failed to scan production sources: {0}")]
    Io(String),
}

/// Fail if forbidden compatibility symbols reappear under production crate sources.
pub fn reject_removed_compat_apis(repo: &Path) -> Result<(), CompatError> {
    let crates_dir = repo.join("crates");
    let mut hits = Vec::new();
    let entries = fs::read_dir(&crates_dir)
        .map_err(|error| CompatError::Io(format!("read {}: {error}", crates_dir.display())))?;
    for entry in entries {
        let entry = entry.map_err(|error| CompatError::Io(error.to_string()))?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Gate/tooling crates may mention forbidden tokens in policy source.
        if matches!(name.as_ref(), "nsb-public-api-gate" | "nsb-coverage-gate") {
            continue;
        }
        let src = entry.path().join("src");
        if src.is_dir() {
            visit(&src, &mut hits)?;
        }
    }
    if hits.is_empty() {
        Ok(())
    } else {
        Err(CompatError::Found(hits.join("\n")))
    }
}

fn visit(path: &Path, hits: &mut Vec<String>) -> Result<(), CompatError> {
    if path.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| CompatError::Io(error.to_string()))? {
            let entry = entry.map_err(|error| CompatError::Io(error.to_string()))?;
            visit(&entry.path(), hits)?;
        }
        return Ok(());
    }
    if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
        return Ok(());
    }
    let text = fs::read_to_string(path).map_err(|error| CompatError::Io(error.to_string()))?;
    for (index, line) in text.lines().enumerate() {
        let domain_patterns = is_airglow_source(path)
            .then_some(AIRGLOW_FORBIDDEN_PATTERNS)
            .into_iter()
            .flatten()
            .copied()
            .chain(
                is_moonlight_source(path)
                    .then_some(MOONLIGHT_PUBLIC_IMPL_PATTERNS)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .chain(
                is_starlight_source(path)
                    .then_some(STARLIGHT_PUBLIC_IMPL_PATTERNS)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .chain(
                is_zodiacal_source(path)
                    .then_some(ZODIACAL_PUBLIC_IMPL_PATTERNS)
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .chain(
                is_nsb_root_source(path)
                    .then_some(ZODIACAL_ROOT_PUBLIC_IMPL_PATTERNS)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        for pattern in FORBIDDEN_PATTERNS.iter().copied().chain(domain_patterns) {
            if line.contains(pattern) {
                hits.push(format!("{}:{}:{line}", display_repo_path(path), index + 1));
            }
        }
    }
    Ok(())
}

fn is_airglow_source(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "airglow")
}

fn is_moonlight_source(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "moonlight")
}

fn is_starlight_source(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "starlight")
}

fn is_zodiacal_source(path: &Path) -> bool {
    path.components()
        .any(|component| component.as_os_str() == "zodiacal")
}

fn is_nsb_root_source(path: &Path) -> bool {
    path.ends_with(Path::new("crates/nsb/src/lib.rs"))
}

fn display_repo_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect::<PathBuf>()
        .display()
        .to_string()
}
