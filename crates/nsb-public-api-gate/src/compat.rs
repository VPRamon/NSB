//! Guard against deliberately removed compatibility-only symbols.

use std::fs;
use std::path::{Path, PathBuf};

use syn::{ImplItem, Item, UseTree, Visibility};
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

const ZODIACAL_REMOVED_IMPL_SYMBOLS: &[&str] = &[
    "ZodiacalBrightnessGrid",
    "ZodiacalBrightnessModel",
    "ZodiacalLight",
    "ZodiacalOutputs",
    "ZodiacalSpectrum",
];

const ZODIACAL_REMOVED_IMPL_METHODS: &[&str] = &["with_solar_spectrum", "with_brightness_model"];

#[derive(Debug, Error)]
pub enum CompatError {
    #[error("removed or compatibility-only API found in production source:\n{0}")]
    Found(String),
    #[error("failed to scan production sources: {0}")]
    Io(String),
    #[error("failed to parse Rust source while checking public API: {0}")]
    Parse(String),
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
    reject_public_zodiacal_impl_surface(path, &text, hits)?;

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
            );
        for pattern in FORBIDDEN_PATTERNS.iter().copied().chain(domain_patterns) {
            if line.contains(pattern) {
                hits.push(format!("{}:{}:{line}", display_repo_path(path), index + 1));
            }
        }
    }
    Ok(())
}

fn reject_public_zodiacal_impl_surface(
    path: &Path,
    text: &str,
    hits: &mut Vec<String>,
) -> Result<(), CompatError> {
    if !is_zodiacal_source(path) && !is_nsb_root_source(path) {
        return Ok(());
    }

    let file = syn::parse_file(text)
        .map_err(|error| CompatError::Parse(format!("{}: {error}", display_repo_path(path))))?;
    let check_declarations = is_zodiacal_source(path);

    for item in &file.items {
        inspect_zodiacal_item(path, item, check_declarations, hits);
    }

    Ok(())
}

fn inspect_zodiacal_item(
    path: &Path,
    item: &Item,
    check_declarations: bool,
    hits: &mut Vec<String>,
) {
    match item {
        Item::Use(item_use) if is_public(&item_use.vis) => {
            if let Some(symbol) = removed_use_symbol(&item_use.tree) {
                hits.push(format!(
                    "{}: public re-export of removed Zodiacal implementation symbol {symbol}",
                    display_repo_path(path)
                ));
            }
        }
        Item::Struct(item_struct) if check_declarations && is_public(&item_struct.vis) => {
            reject_removed_type_name(path, &item_struct.ident.to_string(), hits);
        }
        Item::Enum(item_enum) if check_declarations && is_public(&item_enum.vis) => {
            reject_removed_type_name(path, &item_enum.ident.to_string(), hits);
        }
        Item::Type(item_type) if check_declarations && is_public(&item_type.vis) => {
            reject_removed_type_name(path, &item_type.ident.to_string(), hits);
        }
        Item::Union(item_union) if check_declarations && is_public(&item_union.vis) => {
            reject_removed_type_name(path, &item_union.ident.to_string(), hits);
        }
        Item::Trait(item_trait) if check_declarations && is_public(&item_trait.vis) => {
            reject_removed_type_name(path, &item_trait.ident.to_string(), hits);
        }
        Item::Fn(item_fn) if check_declarations && is_public(&item_fn.vis) => {
            let name = item_fn.sig.ident.to_string();
            if ZODIACAL_REMOVED_IMPL_METHODS
                .iter()
                .any(|removed| *removed == name.as_str())
            {
                hits.push(format!(
                    "{}: public function from removed Zodiacal implementation API {name}",
                    display_repo_path(path)
                ));
            }
        }
        Item::Impl(item_impl) if check_declarations => {
            for impl_item in &item_impl.items {
                if let ImplItem::Fn(method) = impl_item {
                    let name = method.sig.ident.to_string();
                    if is_public(&method.vis)
                        && ZODIACAL_REMOVED_IMPL_METHODS
                            .iter()
                            .any(|removed| *removed == name.as_str())
                    {
                        hits.push(format!(
                            "{}: public method from removed Zodiacal implementation API {name}",
                            display_repo_path(path)
                        ));
                    }
                }
            }
        }
        Item::Mod(item_mod) => {
            if let Some((_, items)) = &item_mod.content {
                for nested in items {
                    inspect_zodiacal_item(path, nested, check_declarations, hits);
                }
            }
        }
        _ => {}
    }
}

fn reject_removed_type_name(path: &Path, name: &str, hits: &mut Vec<String>) {
    if let Some(symbol) = removed_symbol(name) {
        hits.push(format!(
            "{}: public declaration of removed Zodiacal implementation symbol {symbol}",
            display_repo_path(path)
        ));
    }
}

fn removed_use_symbol(tree: &UseTree) -> Option<&'static str> {
    match tree {
        UseTree::Path(path) => removed_use_symbol(&path.tree),
        UseTree::Name(name) => removed_symbol(&name.ident.to_string()),
        UseTree::Rename(rename) => removed_symbol(&rename.ident.to_string())
            .or_else(|| removed_symbol(&rename.rename.to_string())),
        UseTree::Group(group) => group.items.iter().find_map(removed_use_symbol),
        UseTree::Glob(_) => None,
    }
}

fn removed_symbol(name: &str) -> Option<&'static str> {
    ZODIACAL_REMOVED_IMPL_SYMBOLS
        .iter()
        .copied()
        .find(|symbol| *symbol == name)
}

fn is_public(visibility: &Visibility) -> bool {
    matches!(visibility, Visibility::Public(_))
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
