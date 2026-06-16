//! Small typed catalogue of well-known dark-sky / observatory sites.
//!
//! Each entry provides the geodetic location plus the nominal V-band
//! atmospheric extinction coefficient `k_v` (mag/airmass) characteristic of
//! the site. Geodetic coordinates for sites present in `siderust` are
//! delegated to that crate's constants rather than being duplicated here.
//!
//! Scientific role:
//! this file provides ready-made test inputs and example sites for the NSB
//! model; the entries are **not** authoritative time-resolved measurements.

use siderust::catalogs::observatories;
use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};

/// A catalogued site with the parameters relevant to the NSB model.
///
/// Geometry is delegated to siderust's [`Geodetic<ECEF>`]; only NSB-specific
/// scalar metadata (`k_v`) is added.
#[derive(Debug, Clone, Copy)]
pub struct CatalogSite {
    /// Human-readable site name.
    pub name: &'static str,
    /// Geodetic position (WGS84 ellipsoidal lon/lat/height).
    pub geodetic: Geodetic<ECEF>,
    /// Nominal V-band atmospheric extinction coefficient (mag/airmass).
    pub k_v: f64,
}

impl CatalogSite {
    /// Returns the geodetic position of this site.
    #[inline]
    pub const fn geodetic(&self) -> Geodetic<ECEF> {
        self.geodetic
    }
}

/// Cerro Paranal — ESO VLT, Atacama Desert, Chile.
///
/// Geodetic coordinates from `siderust::catalogs::observatories::EL_PARANAL`.
/// Nominal V-band extinction `k_v ≈ 0.11` mag/airmass (Patat et al. 2011).
pub const CERRO_PARANAL: CatalogSite = CatalogSite {
    name: "Cerro Paranal (VLT)",
    geodetic: observatories::EL_PARANAL.geodetic(),
    k_v: 0.11,
};

/// Mauna Kea — Gemini-N / Subaru / IRTF area, Hawaiʻi, USA.
///
/// Geodetic coordinates from `siderust::catalogs::observatories::MAUNA_KEA`.
/// Nominal V-band extinction `k_v ≈ 0.12` mag/airmass.
pub const MAUNA_KEA: CatalogSite = CatalogSite {
    name: "Mauna Kea",
    geodetic: observatories::MAUNA_KEA.geodetic(),
    k_v: 0.12,
};

/// Roque de los Muchachos — La Palma, Canary Islands, Spain.
///
/// Geodetic coordinates from
/// `siderust::catalogs::observatories::ROQUE_DE_LOS_MUCHACHOS`.
/// Nominal V-band extinction `k_v ≈ 0.13` mag/airmass.
pub const ROQUE_DE_LOS_MUCHACHOS: CatalogSite = CatalogSite {
    name: "Roque de los Muchachos (La Palma)",
    geodetic: observatories::ROQUE_DE_LOS_MUCHACHOS.geodetic(),
    k_v: 0.13,
};

/// Apache Point Observatory (APO) — Sunspot, New Mexico, USA.
///
/// Not present in `siderust`; coordinates from APO site documentation.
/// Nominal V-band extinction `k_v ≈ 0.15` mag/airmass (Hogg et al. 2001).
pub const APACHE_POINT: CatalogSite = CatalogSite {
    name: "Apache Point Observatory",
    geodetic: Geodetic::new_raw(
        Degrees::new(-105.8200),
        Degrees::new(32.7803),
        Meters::new(2788.0),
    ),
    k_v: 0.15,
};

/// Kitt Peak National Observatory (KPNO) — Arizona, USA.
///
/// Not present in `siderust`; coordinates from NOAO observer's manual.
/// Nominal V-band extinction `k_v ≈ 0.17` mag/airmass (Landolt 1992).
pub const KITT_PEAK: CatalogSite = CatalogSite {
    name: "Kitt Peak National Observatory",
    geodetic: Geodetic::new_raw(
        Degrees::new(-111.5967),
        Degrees::new(31.9583),
        Meters::new(2096.0),
    ),
    k_v: 0.17,
};

/// Bright suburban reference site — illustrative Bortle ~5/6 location.
///
/// Coordinates are arbitrary (Barcelona metropolitan area), provided as a
/// non-observatory reference for examples and tests that need a clearly
/// light-polluted comparison point.
pub const SUBURBAN_REFERENCE: CatalogSite = CatalogSite {
    name: "Suburban reference (Bortle ~5/6)",
    geodetic: Geodetic::new_raw(
        Degrees::new(2.1734),
        Degrees::new(41.3851),
        Meters::new(50.0),
    ),
    k_v: 0.30,
};

/// All catalogued sites, in declaration order.
pub const ALL_SITES: &[CatalogSite] = &[
    CERRO_PARANAL,
    MAUNA_KEA,
    ROQUE_DE_LOS_MUCHACHOS,
    APACHE_POINT,
    KITT_PEAK,
    SUBURBAN_REFERENCE,
];

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::qtty::{Degrees, Meters};

    #[test]
    fn catalogue_invariants() {
        assert!(
            ALL_SITES.len() >= 6,
            "expected at least 6 catalogued sites, got {}",
            ALL_SITES.len()
        );

        for site in ALL_SITES {
            let geo = site.geodetic();

            assert!(
                geo.lat >= Degrees::new(-90.0) && geo.lat <= Degrees::new(90.0),
                "{}: latitude {}° out of [-90, 90]",
                site.name,
                geo.lat.value()
            );
            assert!(
                geo.lon >= Degrees::new(-180.0) && geo.lon <= Degrees::new(360.0),
                "{}: longitude {}° out of [-180, 360]",
                site.name,
                geo.lon.value()
            );
            assert!(
                geo.height >= Meters::new(0.0) && geo.height <= Meters::new(5000.0),
                "{}: elevation {}m out of [0, 5000]",
                site.name,
                geo.height.value()
            );
            assert!(
                site.k_v > 0.0 && site.k_v < 1.0,
                "{}: k_v {} outside plausible range",
                site.name,
                site.k_v
            );
        }
    }

    #[test]
    fn siderust_delegated_sites_match_upstream_geodetic() {
        let siderust_paranal = observatories::EL_PARANAL.geodetic();
        let nsb_paranal = CERRO_PARANAL.geodetic();
        assert_eq!(nsb_paranal.lat.value(), siderust_paranal.lat.value());
        assert_eq!(nsb_paranal.lon.value(), siderust_paranal.lon.value());
        assert_eq!(nsb_paranal.height.value(), siderust_paranal.height.value());

        let siderust_mk = observatories::MAUNA_KEA.geodetic();
        let nsb_mk = MAUNA_KEA.geodetic();
        assert_eq!(nsb_mk.lat.value(), siderust_mk.lat.value());
        assert_eq!(nsb_mk.lon.value(), siderust_mk.lon.value());

        let siderust_rlm = observatories::ROQUE_DE_LOS_MUCHACHOS.geodetic();
        let nsb_rlm = ROQUE_DE_LOS_MUCHACHOS.geodetic();
        assert_eq!(nsb_rlm.lat.value(), siderust_rlm.lat.value());
        assert_eq!(nsb_rlm.lon.value(), siderust_rlm.lon.value());
    }
}
