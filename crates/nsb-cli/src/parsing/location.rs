use crate::cli::ObserverArgs;
use crate::error::CliError;
use nsb::SiteProfileId;
use serde::Serialize;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SitePreset {
    pub canonical_alias: &'static str,
    pub display_name: &'static str,
    pub lon_deg: f64,
    pub lat_deg: f64,
    pub height_m: f64,
    pub aliases: &'static [&'static str],
}

impl SitePreset {
    pub fn geodetic(self) -> Geodetic<ECEF> {
        Geodetic::<ECEF>::new_raw(
            Degrees::new(self.lon_deg),
            Degrees::new(self.lat_deg),
            Meters::new(self.height_m),
        )
    }
}

pub const SITE_PRESETS: &[SitePreset] = &[
    SitePreset {
        canonical_alias: "CTAO-S",
        display_name: "CTAO South",
        lon_deg: -70.406944,
        lat_deg: -24.627222,
        height_m: 2100.0,
        aliases: &["CTAO-S", "CTA-S", "CTAO-SOUTH", "CTA-SOUTH"],
    },
    SitePreset {
        canonical_alias: "CTAO-N",
        display_name: "CTAO North / Roque de los Muchachos",
        lon_deg: -17.8914,
        lat_deg: 28.7619,
        height_m: 2200.0,
        aliases: &["CTAO-N", "CTA-N", "CTAO-NORTH", "CTA-NORTH"],
    },
    SitePreset {
        canonical_alias: "PARANAL",
        display_name: "Cerro Paranal",
        lon_deg: -70.4044,
        lat_deg: -24.6275,
        height_m: 2635.0,
        aliases: &["PARANAL", "CERRO-PARANAL", "VLT"],
    },
    SitePreset {
        canonical_alias: "ROQUE-DE-LOS-MUCHACHOS",
        display_name: "Roque de los Muchachos",
        lon_deg: -17.8914,
        lat_deg: 28.7619,
        height_m: 2200.0,
        aliases: &["ROQUE-DE-LOS-MUCHACHOS", "ORM", "LA-PALMA", "LAPALMA"],
    },
    SitePreset {
        canonical_alias: "MAUNA-KEA",
        display_name: "Mauna Kea",
        lon_deg: -155.4681,
        lat_deg: 19.8206,
        height_m: 4205.0,
        aliases: &["MAUNA-KEA", "MAUNAKEA"],
    },
    SitePreset {
        canonical_alias: "LA-SILLA",
        display_name: "La Silla Observatory",
        lon_deg: -70.7346,
        lat_deg: -29.2567,
        height_m: 2400.0,
        aliases: &["LA-SILLA", "LASILLA"],
    },
];

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

pub fn resolve_site(alias: &str) -> Option<SitePreset> {
    let normalized = normalize_alias(alias);
    SITE_PRESETS.iter().copied().find(|site| {
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
    fn resolves_ctao_s_alias() {
        let site = resolve_site("ctao-s").expect("site");
        assert_eq!(site.canonical_alias, "CTAO-S");
    }

    #[test]
    fn rejects_unknown_site() {
        assert!(resolve_site("not-a-site").is_none());
    }
}
