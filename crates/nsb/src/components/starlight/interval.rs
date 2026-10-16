// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Explicit external interval-map inspection, separate from default assets.
use crate::site::AtmosphericConditions;
use crate::transport::{DirectPathGeometry, DirectTransmission};
use crate::units::length::Nanometers;
use crate::units::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian as Radiance;
use crate::{NsbError, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};
use std::collections::BTreeMap;
use std::path::Path;

fn invalid(message: impl Into<String>) -> NsbError {
    NsbError::InvalidMap {
        message: message.into(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Pixel {
    pixel: u64,
    sources: u64,
    radiance: [f64; 7],
    statistical_sigma: [f64; 7],
    integrated_statistical_sigma: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    schema: String,
    product_id: String,
    band_nm: [u16; 2],
    bin_edges_nm: [u16; 8],
    nside: u32,
    ordering: String,
    coordinate_frame: String,
    domain: String,
    units: String,
    wavelength_convention: String,
    maturity: String,
    population_policy: String,
    non_xp_inference: Option<serde_json::Value>,
    selection_correction: Option<serde_json::Value>,
    bright_star_supplement: Option<serde_json::Value>,
    systematic_uncertainty: Option<serde_json::Value>,
    statistical_covariance: String,
    design_sha256: String,
    input_partitions: BTreeMap<String, serde_json::Value>,
    catalogue_rows: u64,
    xp_rows: u64,
    unmatched_xp_rows: u64,
    admitted_sources: u64,
    excluded_sources: BTreeMap<String, u64>,
    pixels: Vec<Pixel>,
}

/// Checksum-verified experimental Gaia XP 360–570 nm map.
///
/// This is an advanced explicit product, never the default 300–650 nm asset.
/// Samples are Galactic pixel averages above the atmosphere. They do not imply
/// source masking, fiber-aperture radiance, unresolved completion or in-scattering.
#[derive(Debug)]
pub struct StarlightIntervalMap {
    document: Document,
    sha256: String,
    grid: HealpixGrid,
}

/// Band-limited direct pixel radiance and a spectral-shape envelope.
///
/// Bounds assume a nonnegative physical stellar spectrum within each 30 nm bin.
/// Statistical sigma is separate from the envelope; catalogue/calibration
/// systematic uncertainty and scattering remain unquantified.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DirectStarlightBounds {
    /// Midpoint of the spectral-shape envelope (not a reconstructed spectrum).
    pub midpoint: Radiance,
    /// Smallest direct radiance compatible with the sub-band integrals.
    pub lower: Radiance,
    /// Largest direct radiance compatible with the sub-band integrals.
    pub upper: Radiance,
    /// Conservative sigma bound allowing arbitrary cross-bin covariance.
    pub statistical_sigma_bound: Radiance,
}

impl StarlightIntervalMap {
    /// Verify exact external bytes and fail closed on incompatible provenance.
    pub fn from_json_file(path: &Path, expected_sha256: &str) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_json_bytes(&bytes, expected_sha256)
    }

    /// Load an externally pinned candidate; this grants no production admission.
    pub fn from_json_bytes(bytes: &[u8], expected_sha256: &str) -> Result<Self> {
        if !is_sha256(expected_sha256) {
            return Err(invalid("expected SHA-256 must be lowercase hexadecimal"));
        }
        let sha256 = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if sha256 != expected_sha256 {
            return Err(invalid("interval map checksum mismatch"));
        }
        let d: Document = serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        if d.schema != "nsb-starlight-interval-map-v1" || d.product_id != "starlight-gaia-dr3-xp-360-570-v1"
            || d.band_nm != [360,570] || d.bin_edges_nm != [360,390,420,450,480,510,540,570]
            || d.ordering != "ring" || d.coordinate_frame != "galactic" || d.domain != "top-of-atmosphere"
            || d.units != "photon/(cm2 ns sr)" || d.wavelength_convention != "vacuum" || d.maturity != "experimental"
            || d.population_policy != "gaia-dr3-quality-xp-only-positive-subbands-v1"
            || d.non_xp_inference.is_some() || d.selection_correction.is_some() || d.bright_star_supplement.is_some()
            || d.systematic_uncertainty.is_some() || !is_sha256(&d.design_sha256)
            || d.statistical_covariance != "full BP coefficient covariance; source-independent; bin cross covariance not packed"
            || d.input_partitions.is_empty() || d.unmatched_xp_rows > d.xp_rows
        { return Err(invalid("unsupported interval product provenance")); }
        for value in d.input_partitions.values() {
            if !value["shard_sha256"].as_str().is_some_and(is_sha256) {
                return Err(invalid("missing interval shard identity"));
            }
            for key in ["gaia-source", "xp-continuous"] {
                if !value["inputs"][key].as_str().is_some_and(is_sha256) {
                    return Err(invalid("missing interval input identity"));
                }
            }
        }
        if !d.nside.is_power_of_two() || d.nside > 4096 {
            return Err(invalid("unsupported interval HEALPix resolution"));
        }
        let nside = Nside::new(d.nside).map_err(|e| invalid(e.to_string()))?;
        let grid =
            HealpixGrid::new(nside, HealpixOrdering::Ring).map_err(|e| invalid(e.to_string()))?;
        let npix = 12_u64 * u64::from(d.nside).pow(2);
        if d.pixels.len() as u64 != npix {
            return Err(invalid("incomplete interval pixel coverage"));
        }
        let mut admitted = 0_u64;
        for (i, p) in d.pixels.iter().enumerate() {
            if p.pixel != i as u64
                || p.radiance
                    .iter()
                    .chain(&p.statistical_sigma)
                    .chain(std::iter::once(&p.integrated_statistical_sigma))
                    .any(|x| !x.is_finite() || *x < 0.0)
                || (p.sources == 0 && p.radiance.iter().any(|x| *x != 0.0))
            {
                return Err(invalid("invalid or misordered interval pixel"));
            }
            admitted = admitted
                .checked_add(p.sources)
                .ok_or_else(|| invalid("source count overflow"))?;
        }
        let excluded = d
            .excluded_sources
            .values()
            .try_fold(0_u64, |sum, n| sum.checked_add(*n))
            .ok_or_else(|| invalid("exclusion count overflow"))?;
        if admitted != d.admitted_sources
            || admitted.checked_add(excluded) != Some(d.catalogue_rows)
        {
            return Err(invalid("interval population accounting mismatch"));
        }
        Ok(Self {
            document: d,
            sha256,
            grid,
        })
    }

    /// Exact checksum of the loaded product.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    /// Vacuum wavelength interval in nm; unrelated intervals remain unsupported.
    pub fn band_nm(&self) -> [u16; 2] {
        self.document.band_nm
    }
    /// Scientific maturity remains experimental after successful loading.
    pub fn maturity(&self) -> &str {
        &self.document.maturity
    }
    /// Product HEALPix resolution.
    pub fn nside(&self) -> u32 {
        self.document.nside
    }
    /// Static Gaia population count actually admitted by this candidate.
    pub fn admitted_sources(&self) -> u64 {
        self.document.admitted_sources
    }

    fn pixel(&self, direction: Direction<Galactic>) -> Result<&Pixel> {
        let index = self
            .grid
            .direction_to_pixel(direction)
            .map_err(|e| invalid(e.to_string()))?
            .get();
        self.document
            .pixels
            .get(index as usize)
            .ok_or_else(|| invalid("interval pixel index"))
    }

    /// Inspect the TOA pixel mean and its covariance-aware integrated sigma.
    pub fn top_of_atmosphere_pixel(
        &self,
        direction: Direction<Galactic>,
    ) -> Result<(Radiance, Radiance)> {
        let p = self.pixel(direction)?;
        Ok((
            Radiance::new(p.radiance.iter().sum()),
            Radiance::new(p.integrated_statistical_sigma),
        ))
    }

    /// Bound direct ground-level pixel radiance using NSB Rayleigh/Mie transport.
    ///
    /// Endpoint bounds require nonpositive aerosol power-law exponent and no
    /// molecular-absorption term; direct transmission then increases across each
    /// bin. This does not calculate scattered starlight or a fiber response.
    pub fn direct_pixel_bounds(
        &self,
        direction: Direction<Galactic>,
        geometry: DirectPathGeometry,
        atmosphere: AtmosphericConditions,
    ) -> Result<DirectStarlightBounds> {
        atmosphere.validate()?;
        if atmosphere.mie_params.alpha > 0.0 {
            return Err(NsbError::Unsupported(
                "interval transmission envelope requires nonpositive aerosol power-law exponent"
                    .into(),
            ));
        }
        let p = self.pixel(direction)?;
        let model = DirectTransmission::rayleigh_mie_young1994();
        let mut lower = 0.0;
        let mut upper = 0.0;
        let mut sigma = 0.0;
        for i in 0..7 {
            let lo = model
                .transmission(
                    Nanometers::new(f64::from(self.document.bin_edges_nm[i])),
                    geometry,
                    atmosphere,
                )?
                .value();
            let hi = model
                .transmission(
                    Nanometers::new(f64::from(self.document.bin_edges_nm[i + 1])),
                    geometry,
                    atmosphere,
                )?
                .value();
            if !lo.is_finite() || !hi.is_finite() || lo > hi || lo < 0.0 || hi > 1.0 {
                return Err(invalid("invalid direct transmission envelope"));
            }
            lower += p.radiance[i] * lo;
            upper += p.radiance[i] * hi;
            sigma += p.statistical_sigma[i] * hi;
        }
        Ok(DirectStarlightBounds {
            midpoint: Radiance::new((lower + upper) * 0.5),
            lower: Radiance::new(lower),
            upper: Radiance::new(upper),
            statistical_sigma_bound: Radiance::new(sigma),
        })
    }
}

fn is_sha256(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::angular::Degrees;
    use siderust::coordinates::spherical;

    fn fixture() -> serde_json::Value {
        serde_json::json!({
            "schema":"nsb-starlight-interval-map-v1", "product_id":"starlight-gaia-dr3-xp-360-570-v1",
            "band_nm":[360,570],"bin_edges_nm":[360,390,420,450,480,510,540,570],
            "nside":1,"ordering":"ring","coordinate_frame":"galactic","domain":"top-of-atmosphere",
            "units":"photon/(cm2 ns sr)","wavelength_convention":"vacuum","maturity":"experimental",
            "population_policy":"gaia-dr3-quality-xp-only-positive-subbands-v1",
            "non_xp_inference":null,"selection_correction":null,"bright_star_supplement":null,"systematic_uncertainty":null,
            "statistical_covariance":"full BP coefficient covariance; source-independent; bin cross covariance not packed",
            "design_sha256":"a".repeat(64),"input_partitions":{"fixture":{"shard_sha256":"b".repeat(64),"inputs":{"gaia-source":"c".repeat(64),"xp-continuous":"d".repeat(64)}}},
            "catalogue_rows":14,"xp_rows":12,"unmatched_xp_rows":0,"admitted_sources":12,"excluded_sources":{"no_band_calibrated_non_xp_inference":2},
            "pixels":(0..12).map(|i|serde_json::json!({"pixel":i,"sources":1,"radiance":vec![1.0+i as f64;7],"statistical_sigma":vec![0.1;7],"integrated_statistical_sigma":0.3})).collect::<Vec<_>>()
        })
    }
    fn load(v: serde_json::Value) -> Result<StarlightIntervalMap> {
        let b = serde_json::to_vec(&v).unwrap();
        let sha = Sha256::digest(&b)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        StarlightIntervalMap::from_json_bytes(&b, &sha)
    }
    fn direction() -> Direction<Galactic> {
        spherical::Direction::<Galactic>::new(Degrees::new(0.0), Degrees::new(0.0)).to_cartesian()
    }
    #[test]
    fn typed_pixel_lookup_and_direct_envelope_preserve_units_and_identity() {
        let map = load(fixture()).unwrap();
        assert_eq!(map.band_nm(), [360, 570]);
        assert_eq!(map.nside(), 1);
        assert_eq!(map.admitted_sources(), 12);
        assert_eq!(map.maturity(), "experimental");
        let (toa, sigma) = map.top_of_atmosphere_pixel(direction()).unwrap();
        assert_eq!(toa.value(), 35.0);
        assert_eq!(sigma.value(), 0.3);
        let a = AtmosphericConditions::clear_sky_with_pressure(siderust::qtty::Hectopascals::new(
            790.0,
        ))
        .unwrap();
        let zenith = DirectPathGeometry::new(Degrees::new(0.0)).unwrap();
        let b = map.direct_pixel_bounds(direction(), zenith, a).unwrap();
        assert!(
            0.0 < b.lower.value() && b.lower < b.midpoint && b.midpoint < b.upper && b.upper < toa
        );
        assert!(b.statistical_sigma_bound.value() > 0.0);
        let horizon = map
            .direct_pixel_bounds(
                direction(),
                DirectPathGeometry::new(Degrees::new(60.0)).unwrap(),
                a,
            )
            .unwrap();
        assert!(horizon.midpoint < b.midpoint);
        let mut bad = a;
        bad.mie_params.alpha = 1.0;
        assert!(map.direct_pixel_bounds(direction(), zenith, bad).is_err());
    }
    #[test]
    fn checksum_schema_band_domain_population_and_pixel_corruption_fail_closed() {
        let v = fixture();
        let bytes = serde_json::to_vec(&v).unwrap();
        assert!(StarlightIntervalMap::from_json_bytes(&bytes, &"0".repeat(64)).is_err());
        assert!(StarlightIntervalMap::from_json_bytes(&bytes, "bad").is_err());
        for (key, value) in [
            ("band_nm", serde_json::json!([300, 650])),
            ("units", "wrong".into()),
            ("domain", "ground".into()),
            ("ordering", "nested".into()),
            ("coordinate_frame", "icrs".into()),
            ("maturity", "production".into()),
            ("design_sha256", "bad".into()),
            ("catalogue_rows", 13.into()),
            ("admitted_sources", 11.into()),
            ("nside", 3.into()),
            ("systematic_uncertainty", 0.into()),
            ("unknown", true.into()),
        ] {
            let mut changed = v.clone();
            changed[key] = value;
            assert!(load(changed).is_err(), "{key}");
        }
        let mut changed = v.clone();
        changed["pixels"][0]["radiance"][0] = (-1.0).into();
        assert!(load(changed).is_err());
        let mut changed = v.clone();
        changed["pixels"][0]["pixel"] = 1.into();
        assert!(load(changed).is_err());
        let mut changed = v.clone();
        changed["pixels"].as_array_mut().unwrap().pop();
        assert!(load(changed).is_err());
        let mut changed = v;
        changed["input_partitions"]["fixture"]["inputs"]["gaia-source"] = "bad".into();
        assert!(load(changed).is_err());
    }
}
