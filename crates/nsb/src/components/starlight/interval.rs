// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Internal, versioned spectral Starlight parser; public entry point is StarlightMap.
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
struct LegacyPixel {
    pixel: u64,
    sources: u64,
    radiance: [f64; 7],
    statistical_sigma: [f64; 7],
    integrated_statistical_sigma: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SpectralPixel {
    pub(super) sources: u64,
    pub(super) radiance: Vec<f64>,
    pub(super) statistical_sigma: Option<Vec<f64>>,
    pub(super) integrated_statistical_sigma: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenericPixel {
    pixel: u64,
    sources: u64,
    radiance: Vec<f64>,
    #[serde(default)]
    statistical_sigma: Option<Vec<f64>>,
    #[serde(default)]
    integrated_statistical_sigma: Option<f64>,
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
    // Required explicit declarations: absent != declared null.
    non_xp_inference: serde_json::Value,
    selection_correction: serde_json::Value,
    bright_star_supplement: serde_json::Value,
    systematic_uncertainty: serde_json::Value,
    statistical_covariance: String,
    design_sha256: String,
    input_partitions: BTreeMap<String, serde_json::Value>,
    catalogue_rows: u64,
    xp_rows: u64,
    unmatched_xp_rows: u64,
    admitted_sources: u64,
    excluded_sources: BTreeMap<String, u64>,
    pixels: Vec<LegacyPixel>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenericProvenance {
    source_catalogue: String,
    source_license: String,
    source_selection: String,
    generation_method: String,
    limitations: Vec<String>,
    #[serde(default)]
    gaia: Option<GaiaAuditProvenance>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GaiaAuditProvenance {
    design_sha256: String,
    input_partitions: BTreeMap<String, GaiaInputIdentity>,
    xp_rows: u64,
    unmatched_xp_rows: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GaiaInputIdentity {
    shard_sha256: String,
    inputs: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GenericDocument {
    schema: String,
    product_id: String,
    band_nm: [u16; 2],
    bin_edges_nm: Vec<u16>,
    nside: u32,
    ordering: String,
    coordinate_frame: String,
    domain: String,
    units: String,
    wavelength_convention: String,
    maturity: String,
    provenance: GenericProvenance,
    catalogue_rows: u64,
    admitted_sources: u64,
    excluded_sources: BTreeMap<String, u64>,
    pixels: Vec<GenericPixel>,
}

/// Direct atmospheric transmission bounds; no spectral reconstruction.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DirectStarlightBounds {
    /// Midpoint of the envelope, not a measured spectrum.
    pub midpoint: Radiance,
    /// Lower physical bound on directly transmitted radiance.
    pub lower: Radiance,
    /// Upper physical bound on directly transmitted radiance.
    pub upper: Radiance,
    /// Conservative statistical sigma bound; available only with per-bin sigma.
    pub statistical_sigma_bound: Radiance,
}

/// Shared internal representation of checksum-pinned integrated spectral bins.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SpectralMap {
    pub(super) grid: HealpixGrid,
    pub(super) sha256: String,
    pub(super) product_id: String,
    pub(super) source_catalogue: String,
    pub(super) source_license: String,
    pub(super) source_selection: String,
    pub(super) maturity: String,
    pub(super) band_nm: [u16; 2],
    pub(super) edges_nm: Vec<u16>,
    pub(super) admitted_sources: u64,
    pub(super) pixels: Vec<SpectralPixel>,
}

impl SpectralMap {
    pub(super) fn from_json_file(path: &Path, expected_sha256: &str) -> Result<Self> {
        let bytes = std::fs::read(path)?;
        Self::from_json_bytes(&bytes, expected_sha256)
    }

    pub(super) fn from_json_bytes(bytes: &[u8], expected_sha256: &str) -> Result<Self> {
        if !is_sha256(expected_sha256) {
            return Err(invalid("expected SHA-256 must be lowercase hexadecimal"));
        }
        let sha256 = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        if sha256 != expected_sha256 {
            return Err(invalid("starlight map checksum mismatch"));
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| invalid(e.to_string()))?;
        match value.get("schema").and_then(serde_json::Value::as_str) {
            Some("nsb-starlight-interval-map-v1") => {
                let d: Document =
                    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
                if d.schema != "nsb-starlight-interval-map-v1"
                    || d.product_id != "starlight-gaia-dr3-xp-360-570-v1"
                    || d.band_nm != [360, 570]
                    || d.bin_edges_nm != [360, 390, 420, 450, 480, 510, 540, 570]
                    || d.ordering != "ring"
                    || d.coordinate_frame != "galactic"
                    || d.domain != "top-of-atmosphere"
                    || d.units != "photon/(cm2 ns sr)"
                    || d.wavelength_convention != "vacuum"
                    || d.maturity != "experimental"
                    || d.population_policy != "gaia-dr3-quality-xp-only-positive-subbands-v1"
                    || !d.non_xp_inference.is_null()
                    || !d.selection_correction.is_null()
                    || !d.bright_star_supplement.is_null()
                    || !d.systematic_uncertainty.is_null()
                    || !is_sha256(&d.design_sha256)
                    || d.statistical_covariance != "full BP coefficient covariance; source-independent; bin cross covariance not packed"
                    || d.input_partitions.is_empty()
                    || d.unmatched_xp_rows > d.xp_rows
                {
                    return Err(invalid("unsupported interval product provenance"));
                }
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
                let pixels = d
                    .pixels
                    .into_iter()
                    .map(|p| {
                        (
                            p.pixel,
                            SpectralPixel {
                                sources: p.sources,
                                radiance: p.radiance.to_vec(),
                                statistical_sigma: Some(p.statistical_sigma.to_vec()),
                                integrated_statistical_sigma: Some(p.integrated_statistical_sigma),
                            },
                        )
                    })
                    .collect();
                Self::validated(
                    sha256,
                    d.product_id,
                    "Gaia DR3 XP".into(),
                    "CC BY-NC 3.0 IGO".into(),
                    d.population_policy,
                    d.maturity,
                    d.band_nm,
                    d.bin_edges_nm.to_vec(),
                    d.nside,
                    &d.ordering,
                    d.catalogue_rows,
                    d.admitted_sources,
                    d.excluded_sources,
                    pixels,
                )
            }
            Some("nsb-starlight-spectral-map-v2") => {
                let d: GenericDocument =
                    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
                if d.schema != "nsb-starlight-spectral-map-v2"
                    || d.product_id.trim().is_empty()
                    || d.coordinate_frame != "galactic"
                    || d.domain != "top-of-atmosphere"
                    || d.units != "photon/(cm2 ns sr)"
                    || d.wavelength_convention != "vacuum"
                    || d.maturity != "experimental"
                    || d.provenance.source_catalogue.trim().is_empty()
                    || d.provenance.source_license.trim().is_empty()
                    || d.provenance.source_selection.trim().is_empty()
                    || d.provenance.generation_method.trim().is_empty()
                    || d.provenance.limitations.is_empty()
                {
                    return Err(invalid("invalid generic starlight provenance or units"));
                }
                if let Some(gaia) = &d.provenance.gaia {
                    if !is_sha256(&gaia.design_sha256)
                        || gaia.input_partitions.is_empty()
                        || gaia.unmatched_xp_rows > gaia.xp_rows
                        || gaia.input_partitions.values().any(|input| {
                            !is_sha256(&input.shard_sha256)
                                || ["gaia-source", "xp-continuous"].iter().any(|key| {
                                    !input.inputs.get(*key).is_some_and(|sha| is_sha256(sha))
                                })
                        })
                    {
                        return Err(invalid("invalid Gaia provenance in generic Starlight map"));
                    }
                }
                let pixels = d.pixels.into_iter().map(|p| (
                    p.pixel,
                    SpectralPixel {
                        sources: p.sources,
                        radiance: p.radiance,
                        statistical_sigma: p.statistical_sigma,
                        integrated_statistical_sigma: p.integrated_statistical_sigma,
                    },
                )).collect();
                Self::validated(
                    sha256,
                    d.product_id,
                    d.provenance.source_catalogue,
                    d.provenance.source_license,
                    d.provenance.source_selection,
                    d.maturity,
                    d.band_nm,
                    d.bin_edges_nm,
                    d.nside,
                    &d.ordering,
                    d.catalogue_rows,
                    d.admitted_sources,
                    d.excluded_sources,
                    pixels,
                )
            }
            _ => Err(invalid("unsupported starlight spectral map schema")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn validated(
        sha256: String,
        product_id: String,
        source_catalogue: String,
        source_license: String,
        source_selection: String,
        maturity: String,
        band_nm: [u16; 2],
        edges_nm: Vec<u16>,
        nside: u32,
        ordering: &str,
        catalogue_rows: u64,
        admitted_sources: u64,
        excluded_sources: BTreeMap<String, u64>,
        indexed_pixels: Vec<(u64, SpectralPixel)>,
    ) -> Result<Self> {
        if edges_nm.len() < 2
            || edges_nm.len() > 1025
            || edges_nm[0] == 0
            || edges_nm.windows(2).any(|edges| edges[0] >= edges[1])
            || band_nm != [edges_nm[0], *edges_nm.last().unwrap()]
            || !nside.is_power_of_two()
            || nside > 4096
        {
            return Err(invalid(
                "invalid starlight spectral bins or HEALPix resolution",
            ));
        }
        let ordering = match ordering {
            "ring" => HealpixOrdering::Ring,
            "nested" => HealpixOrdering::Nested,
            _ => return Err(invalid("unsupported starlight pixel ordering")),
        };
        let grid = HealpixGrid::new(
            Nside::new(nside).map_err(|e| invalid(e.to_string()))?,
            ordering,
        )
        .map_err(|e| invalid(e.to_string()))?;
        let npix = 12_u64 * u64::from(nside).pow(2);
        if indexed_pixels.len() as u64 != npix {
            return Err(invalid("incomplete starlight pixel coverage"));
        }
        let bins = edges_nm.len() - 1;
        let mut admitted = 0_u64;
        let mut pixels = Vec::with_capacity(indexed_pixels.len());
        let mut sigma_channels = None;
        for (i, (index, pixel)) in indexed_pixels.into_iter().enumerate() {
            let has_sigma = pixel.statistical_sigma.is_some();
            let channels = (has_sigma, pixel.integrated_statistical_sigma.is_some());
            if index != i as u64
                || pixel.radiance.len() != bins
                || pixel
                    .statistical_sigma
                    .as_ref()
                    .is_some_and(|s| s.len() != bins)
                || sigma_channels.is_some_and(|expected| expected != channels)
                || pixel
                    .radiance
                    .iter()
                    .chain(pixel.statistical_sigma.as_deref().unwrap_or(&[]))
                    .chain(pixel.integrated_statistical_sigma.iter())
                    .any(|x| !x.is_finite() || *x < 0.0 || (pixel.sources == 0 && *x != 0.0))
            {
                return Err(invalid("invalid or misordered starlight spectral pixel"));
            }
            sigma_channels = Some(channels);
            admitted = admitted
                .checked_add(pixel.sources)
                .ok_or_else(|| invalid("source count overflow"))?;
            pixels.push(pixel);
        }
        let excluded = excluded_sources
            .values()
            .try_fold(0_u64, |sum, n| sum.checked_add(*n))
            .ok_or_else(|| invalid("exclusion count overflow"))?;
        if admitted != admitted_sources || admitted.checked_add(excluded) != Some(catalogue_rows) {
            return Err(invalid("starlight population accounting mismatch"));
        }
        Ok(Self {
            grid,
            sha256,
            product_id,
            source_catalogue,
            source_license,
            source_selection,
            maturity,
            band_nm,
            edges_nm,
            admitted_sources,
            pixels,
        })
    }

    pub(super) fn pixel(&self, direction: Direction<Galactic>) -> Result<&SpectralPixel> {
        let index = self
            .grid
            .direction_to_pixel(direction)
            .map_err(|e| invalid(e.to_string()))?
            .get();
        self.pixels
            .get(index as usize)
            .ok_or_else(|| invalid("starlight spectral pixel index"))
    }

    pub(super) fn top_of_atmosphere_pixel(
        &self,
        direction: Direction<Galactic>,
    ) -> Result<(Radiance, Option<Radiance>)> {
        let p = self.pixel(direction)?;
        Ok((
            Radiance::new(p.radiance.iter().sum()),
            p.integrated_statistical_sigma.map(Radiance::new),
        ))
    }

    pub(super) fn integrate_band(
        &self,
        direction: Direction<Galactic>,
        low: Nanometers,
        high: Nanometers,
    ) -> Result<(Radiance, Option<Radiance>)> {
        let (a, b) = (low.value(), high.value());
        if !a.is_finite()
            || !b.is_finite()
            || a >= b
            || a < f64::from(self.edges_nm[0])
            || b > f64::from(*self.edges_nm.last().unwrap())
        {
            return Err(NsbError::Unsupported(
                "requested band outside starlight spectral coverage".into(),
            ));
        }
        let start = self
            .edges_nm
            .iter()
            .position(|e| f64::from(*e) == a)
            .ok_or_else(|| {
                NsbError::Unsupported(
                    "exact Starlight integration requires stored bin edges".into(),
                )
            })?;
        let end = self
            .edges_nm
            .iter()
            .position(|e| f64::from(*e) == b)
            .ok_or_else(|| {
                NsbError::Unsupported(
                    "exact Starlight integration requires stored bin edges".into(),
                )
            })?;
        let p = self.pixel(direction)?;
        let sigma = if start == 0 && end == self.edges_nm.len() - 1 {
            p.integrated_statistical_sigma
        } else if end == start + 1 {
            p.statistical_sigma.as_ref().map(|s| s[start])
        } else {
            // Full covariance for this union is not packed.
            None
        };
        Ok((
            Radiance::new(p.radiance[start..end].iter().sum()),
            sigma.map(Radiance::new),
        ))
    }

    pub(super) fn direct_pixel_bounds(
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
        let sigma = p.statistical_sigma.as_ref().ok_or_else(|| {
            NsbError::Unsupported(
                "direct Starlight statistical envelope requires measured per-bin sigma".into(),
            )
        })?;
        let model = DirectTransmission::rayleigh_mie_young1994();
        let mut lower = 0.0;
        let mut upper = 0.0;
        let mut uncertainty = 0.0;
        for (i, window) in self.edges_nm.windows(2).enumerate() {
            let lo = model
                .transmission(Nanometers::new(f64::from(window[0])), geometry, atmosphere)?
                .value();
            let hi = model
                .transmission(Nanometers::new(f64::from(window[1])), geometry, atmosphere)?
                .value();
            if !lo.is_finite() || !hi.is_finite() || lo > hi || lo < 0.0 || hi > 1.0 {
                return Err(invalid("invalid direct transmission envelope"));
            }
            lower += p.radiance[i] * lo;
            upper += p.radiance[i] * hi;
            uncertainty += sigma[i] * hi;
        }
        Ok(DirectStarlightBounds {
            midpoint: Radiance::new((lower + upper) * 0.5),
            lower: Radiance::new(lower),
            upper: Radiance::new(upper),
            statistical_sigma_bound: Radiance::new(uncertainty),
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
    fn load(v: serde_json::Value) -> Result<SpectralMap> {
        let b = serde_json::to_vec(&v).unwrap();
        let sha = Sha256::digest(&b)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        SpectralMap::from_json_bytes(&b, &sha)
    }
    fn direction() -> Direction<Galactic> {
        spherical::Direction::<Galactic>::new(Degrees::new(0.0), Degrees::new(0.0)).to_cartesian()
    }
    #[test]
    fn typed_pixel_lookup_and_direct_envelope_preserve_units_and_identity() {
        let map = load(fixture()).unwrap();
        assert_eq!(map.band_nm, [360, 570]);
        assert_eq!(map.grid.nside().get(), 1);
        assert_eq!(map.admitted_sources, 12);
        assert_eq!(map.maturity.as_str(), "experimental");
        let (toa, sigma) = map.top_of_atmosphere_pixel(direction()).unwrap();
        assert_eq!(toa.value(), 35.0);
        assert_eq!(sigma.unwrap().value(), 0.3);
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
        assert!(SpectralMap::from_json_bytes(&bytes, &"0".repeat(64)).is_err());
        assert!(SpectralMap::from_json_bytes(&bytes, "bad").is_err());
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

    fn empty_pixel_fixture() -> serde_json::Value {
        let mut v = fixture();
        v["pixels"][0]["sources"] = 0.into();
        v["pixels"][0]["radiance"] = serde_json::json!(vec![0.0; 7]);
        v["pixels"][0]["statistical_sigma"] = serde_json::json!(vec![0.0; 7]);
        v["pixels"][0]["integrated_statistical_sigma"] = 0.into();
        v["admitted_sources"] = 11.into();
        v["excluded_sources"]["no_band_calibrated_non_xp_inference"] = 3.into();
        v
    }

    #[test]
    fn empty_pixel_channels_must_all_be_zero() {
        let empty = empty_pixel_fixture();
        assert!(load(empty.clone()).is_ok());
        assert!(load(fixture()).is_ok(), "nonempty pixels remain valid");

        let mut nonzero_radiance = empty.clone();
        nonzero_radiance["pixels"][0]["radiance"][0] = 0.5.into();
        assert!(load(nonzero_radiance).is_err(), "empty pixel radiance");

        let mut nonzero_sigma = empty.clone();
        nonzero_sigma["pixels"][0]["statistical_sigma"][0] = 0.5.into();
        assert!(load(nonzero_sigma).is_err(), "empty pixel per-bin sigma");

        let mut nonzero_integrated_sigma = empty;
        nonzero_integrated_sigma["pixels"][0]["integrated_statistical_sigma"] = 0.5.into();
        assert!(
            load(nonzero_integrated_sigma).is_err(),
            "empty pixel integrated sigma"
        );
    }

    #[test]
    fn experimental_provenance_requires_explicit_null_fields() {
        let valid = fixture();
        assert!(load(valid.clone()).is_ok());
        for key in [
            "non_xp_inference",
            "selection_correction",
            "bright_star_supplement",
            "systematic_uncertainty",
        ] {
            let mut missing = valid.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(load(missing).is_err(), "missing {key} must fail");

            let mut non_null = valid.clone();
            non_null[key] = serde_json::json!({"unexpected": true});
            assert!(load(non_null).is_err(), "non-null {key} must fail");
        }

        let mut extended = valid;
        extended["unexpected_provenance"] = true.into();
        assert!(load(extended).is_err(), "unknown fields must fail");
    }

    #[test]
    fn generic_schema_accepts_different_bin_counts_and_reports_covariance_limits() {
        for edges in [
            vec![350, 570],
            vec![350, 410, 470, 570],
            (0..=10).map(|i| 350 + i * 20).collect::<Vec<_>>(),
        ] {
            let bins = edges.len() - 1;
            let payload = serde_json::json!({
                "schema":"nsb-starlight-spectral-map-v2",
                "product_id":"synthetic-arbitrary-bin-test",
                "band_nm":[edges[0], *edges.last().unwrap()],
                "bin_edges_nm":edges,
                "nside":1, "ordering":"ring",
                "coordinate_frame":"galactic", "domain":"top-of-atmosphere",
                "units":"photon/(cm2 ns sr)", "wavelength_convention":"vacuum",
                "maturity":"experimental",
                "provenance":{
                    "source_catalogue":"synthetic",
                    "source_license":"test-only",
                    "source_selection":"test-all",
                    "generation_method":"synthetic-bin-fixture",
                    "limitations":["test-only"]
                },
                "catalogue_rows":12, "admitted_sources":12, "excluded_sources":{},
                "pixels":(0..12).map(|i| serde_json::json!({
                    "pixel":i,"sources":1,"radiance":vec![1.0;bins],
                    "statistical_sigma":vec![0.1;bins],
                    "integrated_statistical_sigma":0.5
                })).collect::<Vec<_>>()
            });
            let map = load(payload).unwrap();
            assert_eq!(map.edges_nm.len(), bins + 1);
            let d = direction();
            let full = map
                .integrate_band(
                    d,
                    Nanometers::new(f64::from(map.edges_nm[0])),
                    Nanometers::new(f64::from(*map.edges_nm.last().unwrap())),
                )
                .unwrap();
            assert_eq!(full.0.value(), bins as f64);
            assert_eq!(full.1.unwrap().value(), 0.5);
            if bins > 2 {
                let interior = map
                    .integrate_band(
                        d,
                        Nanometers::new(f64::from(map.edges_nm[0])),
                        Nanometers::new(f64::from(map.edges_nm[2])),
                    )
                    .unwrap();
                assert_eq!(interior.0.value(), 2.0);
                assert!(interior.1.is_none(), "cross-bin covariance not packed");
            }
            assert!(map
                .integrate_band(
                    d,
                    Nanometers::new(351.0),
                    Nanometers::new(f64::from(*map.edges_nm.last().unwrap()))
                )
                .is_err());
        }
    }
}
