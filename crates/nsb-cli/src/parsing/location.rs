use crate::cli::ObserverArgs;
use crate::error::CliError;
use nsb::SiteProfileId;
use serde::{Deserialize, Serialize};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};
use std::collections::HashSet;
use std::sync::OnceLock;

const BUNDLED_OBSERVATORIES_TOML: &str = include_str!("../../data/observatories.toml");

#[derive(Debug, Clone, Serialize)]
pub struct SitePreset {
    pub canonical_alias: String,
    pub display_name: String,
    pub lon_deg: f64,
    pub lat_deg: f64,
    pub height_m: f64,
    pub aliases: Vec<String>,
}

impl SitePreset {
    pub fn geodetic(&self) -> Geodetic<ECEF> {
        Geodetic::<ECEF>::new_raw(
            Degrees::new(self.lon_deg),
            Degrees::new(self.lat_deg),
            Meters::new(self.height_m),
        )
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SiteCatalogFile {
    site: Vec<SiteRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SiteRecord {
    canonical_alias: String,
    display_name: String,
    lon_deg: f64,
    lat_deg: f64,
    height_m: f64,
    aliases: Vec<String>,
    source_urls: Vec<String>,
}

pub fn site_presets() -> &'static [SitePreset] {
    static SITES: OnceLock<Vec<SitePreset>> = OnceLock::new();
    SITES
        .get_or_init(|| {
            parse_site_catalog(BUNDLED_OBSERVATORIES_TOML)
                .expect("bundled observatory catalog must be valid")
        })
        .as_slice()
}

fn parse_site_catalog(input: &str) -> Result<Vec<SitePreset>, String> {
    let parsed: SiteCatalogFile =
        toml::from_str(input).map_err(|error| format!("invalid observatory catalog TOML: {error}"))?;
    if parsed.site.is_empty() {
        return Err("observatory catalog must contain at least one site".to_string());
    }

    let mut seen_aliases = HashSet::new();
    let mut sites = Vec::with_capacity(parsed.site.len());
    for (index, record) in parsed.site.into_iter().enumerate() {
        let record_number = index + 1;
        if record.canonical_alias.trim().is_empty() {
            return Err(format!("site {record_number} has an empty canonical_alias"));
        }
        if record.display_name.trim().is_empty() {
            return Err(format!("site {record_number} has an empty display_name"));
        }
        if !record.lon_deg.is_finite() || !(-180.0..=180.0).contains(&record.lon_deg) {
            return Err(format!("site {record_number} has invalid lon_deg"));
        }
        if !record.lat_deg.is_finite() || !(-90.0..=90.0).contains(&record.lat_deg) {
            return Err(format!("site {record_number} has invalid lat_deg"));
        }
        if !record.height_m.is_finite() || !(-500.0..=10_000.0).contains(&record.height_m) {
            return Err(format!("site {record_number} has invalid height_m"));
        }
        if record.aliases.is_empty() {
            return Err(format!("site {record_number} must define at least one alias"));
        }
        let canonical = normalize_alias(&record.canonical_alias);
        if !record
            .aliases
            .iter()
            .any(|alias| normalize_alias(alias) == canonical)
        {
            return Err(format!(
                "site {record_number} aliases must include canonical_alias {:?}",
                record.canonical_alias
            ));
        }
        for alias in &record.aliases {
            let normalized = normalize_alias(alias);
            if normalized.is_empty() {
                return Err(format!("site {record_number} contains an empty alias"));
            }
            if !seen_aliases.insert(normalized.clone()) {
                return Err(format!("duplicate site alias {normalized:?}"));
            }
        }
        if record.source_urls.is_empty()
            || record
                .source_urls
                .iter()
                .any(|url| !url.starts_with("https://"))
        {
            return Err(format!(
                "site {record_number} must provide at least one HTTPS source URL"
            ));
        }

        sites.push(SitePreset {
            canonical_alias: record.canonical_alias,
            display_name: record.display_name,
            lon_deg: record.lon_deg,
            lat_deg: record.lat_deg,
            height_m: record.height_m,
            aliases: record.aliases,
        });
    }
    Ok(sites)
}

pub fn resolve_observer(args: &ObserverArgs) -> Result<Geodetic<ECEF>, CliError> {
    match (args.site.as_deref(), args.lon, args.lat, args.height) {
        (Some(site), None, None, None) => resolve_site(site)
            .map(SitePreset::geodetic)
            .ok_or_else(|| CliError::UnknownSite(site.to_string())),
        (None, Some(lon), Some(lat), Some(height)) => Ok(Geodetic::<ECEF>::new_raw(
            Degrees::new(lon),
            Degrees::new(lat),
            Meters::new(height),
        )),
        _ => Err(CliError::InvalidObserver),
    }
}

pub fn resolve_site(alias: &str) -> Option<&'static SitePreset> {
    let normalized = normalize_alias(alias);
    site_presets().iter().find(|site| {
        site.aliases
            .iter()
            .any(|candidate| normalize_alias(candidate) == normalized)
    })
}

pub fn site_profile(args: &ObserverArgs) -> SiteProfileId {
    match args.site.as_deref().and_then(resolve_site) {
        Some(site) if site.canonical_alias == "CTAO-N" => SiteProfileId::CtaNorth,
        Some(site) if site.canonical_alias == "CTAO-S" => SiteProfileId::CtaSouth,
        _ => SiteProfileId::GenericClearSky,
    }
}

fn normalize_alias(alias: &str) -> String {
    alias.trim().to_ascii_uppercase().replace(['_', ' '], "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_valid_and_covers_relevant_observatories() {
        let sites = site_presets();
        assert!(sites.len() >= 12);
        for alias in [
            "CTAO-S", "CTAO-N", "HESS", "MAGIC", "FACT", "VERITAS", "FAST", "GTC",
            "PARANAL", "ROQUE-DE-LOS-MUCHACHOS", "MAUNA-KEA", "LA-SILLA",
        ] {
            assert!(resolve_site(alias).is_some(), "missing bundled site {alias}");
        }
    }

    #[test]
    fn resolves_ctao_s_alias() {
        let site = resolve_site("ctao-s").expect("site");
        assert_eq!(site.canonical_alias, "CTAO-S");
    }

    #[test]
    fn ctao_sites_remain_distinct_from_nearby_observatories() {
        let north = resolve_site("CTAO-N").unwrap();
        let orm = resolve_site("ORM").unwrap();
        let south = resolve_site("CTAO-S").unwrap();
        let paranal = resolve_site("PARANAL").unwrap();

        assert_ne!(north.geodetic(), orm.geodetic());
        assert_ne!(south.geodetic(), paranal.geodetic());
    }

    #[test]
    fn bundled_catalog_rejects_duplicate_normalized_aliases() {
        let duplicate = r#"
            [[site]]
            canonical_alias = "A"
            display_name = "A"
            lon_deg = 0.0
            lat_deg = 0.0
            height_m = 0.0
            aliases = ["A"]
            source_urls = ["https://example.com/a"]

            [[site]]
            canonical_alias = "B"
            display_name = "B"
            lon_deg = 1.0
            lat_deg = 1.0
            height_m = 1.0
            aliases = ["a"]
            source_urls = ["https://example.com/b"]
        "#;
        assert!(parse_site_catalog(duplicate)
            .unwrap_err()
            .contains("duplicate site alias"));
    }

    #[test]
    fn rejects_unknown_site() {
        assert!(resolve_site("not-a-site").is_none());
    }
}
