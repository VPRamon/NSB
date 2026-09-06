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

pub fn load_catalog(path: Option<&Path>) -> Result<ObservatoryCatalog, CliError> {
    match path {
        Some(path) => ObservatoryCatalog::from_path(path)
            .map_err(|source| CliError::ObservatoryCatalog(source.to_string())),
        None => Ok(ObservatoryCatalog::builtin()),
    }
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
    fn bundled_alias_resolves_to_catalog_record() {
        let catalog = ObservatoryCatalog::builtin();
        assert_eq!(
            resolve_site(&catalog, "paranal").map(|site| site.name.as_ref()),
            Some("El Paranal Observatory")
        );
    }

    #[test]
    fn ctao_aliases_do_not_substitute_nearby_observatories() {
        let catalog = ObservatoryCatalog::builtin();
        assert!(resolve_site(&catalog, "CTAO-N").is_none());
        assert!(resolve_site(&catalog, "CTAO-S").is_none());
    }
}
