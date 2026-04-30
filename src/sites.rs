//! Small typed catalogue of well-known dark-sky / observatory sites.
//!
//! Each entry provides the geodetic location (reusing siderust's
//! [`Geodetic<ECEF>`]) plus the nominal V-band atmospheric extinction
//! coefficient `k_v` (mag/airmass) characteristic of the site.  These are
//! intended as ready-made test inputs and example sites for the NSB model;
//! they are **not** authoritative time-resolved measurements.
//!
//! Sources for each site are documented in the per-constant doc comments.
//!
//! Scientific role:
//! this file is a small catalogue of representative dark-sky sites. It is not
//! part of the core NSB physics, but it provides realistic observing contexts
//! for examples, tests, and comparative studies.
//!
//! Contribution to the science:
//! the site metadata here makes it easier to evaluate how sky background
//! changes from one observatory environment to another. The geodetic position
//! affects local sky geometry, and the nominal extinction coefficient is useful
//! context for atmosphere-sensitive work such as moonlight interpretation.

use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::ECEF;
use siderust::qtty::{Degrees, Meters};

/// A catalogued site with the parameters relevant to the NSB model.
///
/// This is a thin descriptor — geometry is delegated to siderust's
/// [`Geodetic<ECEF>`]; only NSB-specific scalar metadata is added.
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
/// - Longitude: −70.4043°, Latitude: −24.6272°, Elevation: 2635 m
/// - Nominal V-band extinction `k_v ≈ 0.11` mag/airmass
///
/// Source: ESO Paranal site characterisation; Patat (2003, A&A 400, 1183)
/// and Patat et al. (2011, A&A 527, A91) — median photometric-night
/// extinction at Paranal.
pub const CERRO_PARANAL: CatalogSite = CatalogSite {
    name: "Cerro Paranal (VLT)",
    geodetic: Geodetic::new_raw(
        Degrees::new(-70.4043),
        Degrees::new(-24.6272),
        Meters::new(2635.0),
    ),
    k_v: 0.11,
};

/// Mauna Kea — Gemini-N / Subaru / IRTF area, Hawaiʻi, USA.
///
/// - Longitude: −155.4681°, Latitude: +19.8207°, Elevation: 4207 m
/// - Nominal V-band extinction `k_v ≈ 0.12` mag/airmass
///
/// Source: CFHT/Gemini site monitoring; Krisciunas et al. (1987, PASP 99, 887)
/// and Boulade et al. atmospheric-extinction summaries for the summit area.
pub const MAUNA_KEA: CatalogSite = CatalogSite {
    name: "Mauna Kea",
    geodetic: Geodetic::new_raw(
        Degrees::new(-155.4681),
        Degrees::new(19.8207),
        Meters::new(4207.0),
    ),
    k_v: 0.12,
};

/// Roque de los Muchachos — La Palma, Canary Islands, Spain.
///
/// - Longitude: −17.8925°, Latitude: +28.7543°, Elevation: 2396 m
/// - Nominal V-band extinction `k_v ≈ 0.13` mag/airmass
///
/// Source: ING/IAC site characterisation; King (1985, Carlsberg Meridian
/// Telescope) and the ORM atmospheric-extinction monitoring summaries
/// (cf. García-Gil et al. 2010).
pub const ROQUE_DE_LOS_MUCHACHOS: CatalogSite = CatalogSite {
    name: "Roque de los Muchachos (La Palma)",
    geodetic: Geodetic::new_raw(
        Degrees::new(-17.8925),
        Degrees::new(28.7543),
        Meters::new(2396.0),
    ),
    k_v: 0.13,
};

/// Apache Point Observatory (APO) — Sunspot, New Mexico, USA.
///
/// - Longitude: −105.8200°, Latitude: +32.7803°, Elevation: 2788 m
/// - Nominal V-band extinction `k_v ≈ 0.15` mag/airmass
///
/// Source: APO 3.5 m / SDSS site documentation; Hogg et al. (2001, AJ 122,
/// 2129) extinction analysis at APO.
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
/// - Longitude: −111.5967°, Latitude: +31.9583°, Elevation: 2096 m
/// - Nominal V-band extinction `k_v ≈ 0.17` mag/airmass
///
/// Source: NOAO/KPNO observer's manual; Landolt (1992, AJ 104, 340)
/// and KPNO photometric-night extinction summaries.
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
/// light-polluted comparison point.  Extinction reflects the typical
/// turbid low-altitude continental boundary layer rather than a measured
/// site value.
///
/// - Longitude: +2.1734°, Latitude: +41.3851°, Elevation: 50 m
/// - Nominal V-band extinction `k_v ≈ 0.30` mag/airmass
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

    #[test]
    fn catalogue_invariants() {
        assert!(
            ALL_SITES.len() >= 6,
            "expected at least 6 catalogued sites, got {}",
            ALL_SITES.len()
        );

        for site in ALL_SITES {
            let geo = site.geodetic();
            let lon = geo.lon.value();
            let lat = geo.lat.value();
            let elev = geo.height.value();

            assert!(
                (-90.0..=90.0).contains(&lat),
                "{}: latitude {lat} out of [-90, 90]",
                site.name
            );
            assert!(
                (-180.0..=360.0).contains(&lon),
                "{}: longitude {lon} out of [-180, 360]",
                site.name
            );
            assert!(
                (0.0..=5000.0).contains(&elev),
                "{}: elevation {elev} out of [0, 5000]",
                site.name
            );
            assert!(
                site.k_v > 0.0 && site.k_v < 1.0,
                "{}: k_v {} outside plausible range",
                site.name,
                site.k_v
            );
        }
    }
}
