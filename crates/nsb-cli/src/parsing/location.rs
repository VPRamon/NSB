use crate::cli::ObserverArgs;
use crate::error::CliError;
use serde::{Deserialize, Serialize};
use siderust::catalogs::{Observatory, ObservatoryCatalog};
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::OnceLock;

const NSB_BUNDLED_OBSERVATORIES: &str = include_str!("../../data/observatories.toml");

#[derive(Debug, Deserialize)]
struct AliasFile {
    aliases: BTreeMap<String, String>,
}

/// Serialization/formatting view derived from a Siderust observatory.
///
/// This is not an observatory domain model: it owns no maintained scientific
/// data and is constructed only at the output boundary.
#[derive(Debug, Serialize)]
pub struct ObservatoryOutput {
    pub name: String,
    pub longitude_deg: f64,
    pub latitude_deg: f64,
    pub height_m: f64,
    pub aliases: Vec<String>,
}

impl ObservatoryOutput {
    pub fn from_observatory(observatory: &Observatory) -> Self {
        let geodetic = observatory.geodetic();
        Self {
            name: observatory.name.to_string(),
            longitude_deg: geodetic.lon.value(),
            latitude_deg: geodetic.lat.value(),
            height_m: geodetic.height.value(),
            aliases: aliases_for(&observatory.name),
        }
    }
}

fn alias_map() -> &'static BTreeMap<String, String> {
    static ALIASES: OnceLock<BTreeMap<String, String>> = OnceLock::new();
    ALIASES.get_or_init(|| {
        toml::from_str::<AliasFile>(include_str!("../../data/observatory-aliases.toml"))
            .expect("bundled observatory alias metadata must be valid TOML")
            .aliases
    })
}

/// Loads the active observatory catalog for a command.
///
/// Precedence policy:
///
/// 1. With `--observatory-catalog PATH`, the user file **replaces** the entire
///    effective catalog for that command (neither Siderust builtins nor NSB
///    extensions are consulted).
/// 2. Without that flag, the effective catalog is
///    `ObservatoryCatalog::builtin()` extended with NSB's bundled
///    `observatories.toml`. Exact name collisions between those layers are
///    errors; there is no silent override and no CTAO→ORM/Paranal fallback.
pub fn load_catalog(path: Option<&Path>) -> Result<ObservatoryCatalog, CliError> {
    match path {
        Some(path) => ObservatoryCatalog::from_path(path)
            .map_err(|source| CliError::ObservatoryCatalog(source.to_string())),
        None => effective_bundled_catalog(),
    }
}

fn effective_bundled_catalog() -> Result<ObservatoryCatalog, CliError> {
    let mut catalog = ObservatoryCatalog::builtin();
    let extensions = ObservatoryCatalog::from_toml(NSB_BUNDLED_OBSERVATORIES)
        .map_err(|source| CliError::ObservatoryCatalog(source.to_string()))?;
    catalog
        .extend(extensions)
        .map_err(|source| CliError::ObservatoryCatalog(source.to_string()))?;
    Ok(catalog)
}

pub fn resolve_observer(args: &ObserverArgs) -> Result<Geodetic<ECEF>, CliError> {
    match (args.site.as_deref(), args.lon, args.lat, args.height) {
        (Some(site), None, None, None) => {
            let catalog = load_catalog(args.observatory_catalog.as_deref())?;
            resolve_site(&catalog, site)
                .map(Observatory::geodetic)
                .ok_or_else(|| CliError::UnknownSite(site.to_string()))
        }
        (None, Some(lon), Some(lat), Some(height)) => {
            validate_coordinates(lon, lat, height)?;
            Ok(Geodetic::<ECEF>::new_raw(
                Degrees::new(lon),
                Degrees::new(lat),
                Meters::new(height),
            ))
        }
        _ => Err(CliError::InvalidObserver),
    }
}

pub fn resolve_site<'a>(catalog: &'a ObservatoryCatalog, name: &str) -> Option<&'a Observatory> {
    if let Some(observatory) = catalog.get(name.trim()) {
        return Some(observatory);
    }
    let normalized = normalize_alias(name);
    let catalog_name = alias_map()
        .iter()
        .find(|(alias, _)| normalize_alias(alias) == normalized)
        .map(|(_, catalog_name)| catalog_name)?;
    catalog.get(catalog_name)
}

pub fn catalog_output(catalog: &ObservatoryCatalog) -> Vec<ObservatoryOutput> {
    catalog
        .iter()
        .map(ObservatoryOutput::from_observatory)
        .collect()
}

fn aliases_for(name: &str) -> Vec<String> {
    alias_map()
        .iter()
        .filter(|(_, catalog_name)| catalog_name.as_str() == name)
        .map(|(alias, _)| alias.clone())
        .collect()
}

fn validate_coordinates(lon: f64, lat: f64, height: f64) -> Result<(), CliError> {
    if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
        return Err(CliError::InvalidCoordinates(
            "--lon must be finite and in [-180, 180] degrees".into(),
        ));
    }
    if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
        return Err(CliError::InvalidCoordinates(
            "--lat must be finite and in [-90, 90] degrees".into(),
        ));
    }
    if !height.is_finite() || !(-500.0..=10_000.0).contains(&height) {
        return Err(CliError::InvalidCoordinates(
            "--height must be finite and in [-500, 10000] metres".into(),
        ));
    }
    Ok(())
}

fn normalize_alias(alias: &str) -> String {
    alias.trim().to_ascii_uppercase().replace(['_', ' '], "-")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_catalog_includes_siderust_and_nsb_extensions() {
        let catalog = effective_bundled_catalog().unwrap();
        assert!(catalog.get("El Paranal Observatory").is_some());
        assert!(catalog.get("Roque de los Muchachos Observatory").is_some());
        assert!(catalog.get("CTAO North").is_some());
        assert!(catalog.get("CTAO South").is_some());
        assert!(catalog.get("H.E.S.S.").is_some());
        assert!(catalog.get("MAGIC Telescopes").is_some());
        assert!(catalog.get("First G-APD Cherenkov Telescope").is_some());
        assert!(catalog.get("VERITAS").is_some());
        assert!(catalog
            .get("Five-hundred-meter Aperture Spherical Telescope")
            .is_some());
        assert!(catalog.get("Gran Telescopio Canarias").is_some());
    }

    #[test]
    fn bundled_alias_resolves_to_catalog_record() {
        let catalog = effective_bundled_catalog().unwrap();
        assert_eq!(
            resolve_site(&catalog, "paranal").map(|site| site.name.as_ref()),
            Some("El Paranal Observatory")
        );
    }

    #[test]
    fn ctao_aliases_resolve_to_distinct_records() {
        let catalog = effective_bundled_catalog().unwrap();
        let ctao_n = resolve_site(&catalog, "CTAO-N").unwrap();
        let ctao_s = resolve_site(&catalog, "CTAO-S").unwrap();
        let orm = catalog.get("Roque de los Muchachos Observatory").unwrap();
        let paranal = catalog.get("El Paranal Observatory").unwrap();

        assert_eq!(ctao_n.name.as_ref(), "CTAO North");
        assert_eq!(ctao_s.name.as_ref(), "CTAO South");
        assert_ne!(ctao_n.geodetic(), orm.geodetic());
        assert_ne!(ctao_s.geodetic(), paranal.geodetic());
        assert!((ctao_n.geodetic().lon.value() - (-17.892005)).abs() < 1.0e-12);
        assert!((ctao_n.geodetic().lat.value() - 28.762164).abs() < 1.0e-12);
        assert!((ctao_n.geodetic().height.value() - 2240.2).abs() < 1.0e-9);
        assert!((ctao_s.geodetic().lon.value() - (-70.31634444444444)).abs() < 1.0e-12);
        assert!((ctao_s.geodetic().lat.value() - (-24.683427777777776)).abs() < 1.0e-12);
        assert!((ctao_s.geodetic().height.value() - 2184.6).abs() < 1.0e-9);
    }

    #[test]
    fn nsb_extension_aliases_resolve() {
        let catalog = effective_bundled_catalog().unwrap();
        for (alias, name) in [
            ("HESS", "H.E.S.S."),
            ("MAGIC", "MAGIC Telescopes"),
            ("FACT", "First G-APD Cherenkov Telescope"),
            ("VERITAS", "VERITAS"),
            ("FAST", "Five-hundred-meter Aperture Spherical Telescope"),
            ("GTC", "Gran Telescopio Canarias"),
        ] {
            assert_eq!(
                resolve_site(&catalog, alias).map(|site| site.name.as_ref()),
                Some(name),
                "alias {alias}"
            );
        }
    }
}
