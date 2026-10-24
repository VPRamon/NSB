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
struct FixedBinPixel {
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
struct FixedBinDocument {
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
    pixels: Vec<FixedBinPixel>,
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
    // Contiguous pixel-major channels avoid two Vec heap allocations per
    // HEALPix pixel, while retaining the exact source and sigma contracts.
    pub(super) radiance: Vec<f64>,
    pub(super) statistical_sigma: Option<Vec<f64>>,
    pub(super) integrated_statistical_sigma: Option<Vec<f64>>,
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
                let d: FixedBinDocument =
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
                    || d.admitted_sources > d.xp_rows.saturating_sub(d.unmatched_xp_rows)
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
                        || d.admitted_sources > gaia.xp_rows.saturating_sub(gaia.unmatched_xp_rows)
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
                let pixels = d
                    .pixels
                    .into_iter()
                    .map(|p| {
                        (
                            p.pixel,
                            SpectralPixel {
                                sources: p.sources,
                                radiance: p.radiance,
                                statistical_sigma: p.statistical_sigma,
                                integrated_statistical_sigma: p.integrated_statistical_sigma,
                            },
                        )
                    })
                    .collect();
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
            // The locked Siderust API represents NESTED grids but its
            // direction lookup currently implements RING only. Reject at load
            // time instead of accepting a product that cannot be queried.
            "nested" => {
                return Err(invalid(
                    "NESTED spectral lookup requires upstream Siderust support",
                ))
            }
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
        let pixel_count = indexed_pixels.len();
        let mut radiance = Vec::with_capacity(pixel_count * bins);
        let mut statistical_sigma: Option<Vec<f64>> = None;
        let mut integrated_statistical_sigma: Option<Vec<f64>> = None;
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
            if let (Some(per_bin), Some(total)) = (
                pixel.statistical_sigma.as_deref(),
                pixel.integrated_statistical_sigma,
            ) {
                validate_statistical_consistency(per_bin, total)?;
            }
            if sigma_channels.is_none() {
                if channels.0 {
                    statistical_sigma = Some(Vec::with_capacity(pixel_count * bins));
                }
                if channels.1 {
                    integrated_statistical_sigma = Some(Vec::with_capacity(pixel_count));
                }
            }
            sigma_channels = Some(channels);
            admitted = admitted
                .checked_add(pixel.sources)
                .ok_or_else(|| invalid("source count overflow"))?;
            radiance.extend(pixel.radiance);
            if let Some(out) = &mut statistical_sigma {
                out.extend(
                    pixel
                        .statistical_sigma
                        .expect("uniform validated channel schema"),
                );
            }
            if let Some(out) = &mut integrated_statistical_sigma {
                out.push(
                    pixel
                        .integrated_statistical_sigma
                        .expect("uniform validated channel schema"),
                );
            }
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
            radiance,
            statistical_sigma,
            integrated_statistical_sigma,
        })
    }

    fn bins(&self) -> usize {
        self.edges_nm.len() - 1
    }

    fn pixel_index(&self, direction: Direction<Galactic>) -> Result<usize> {
        let index = self
            .grid
            .direction_to_pixel(direction)
            .map_err(|e| invalid(e.to_string()))?
            .get();
        let slot = usize::try_from(index)
            .map_err(|_| invalid("spectral pixel index exceeds address space"))?;
        if slot >= self.radiance.len() / self.bins() {
            return Err(invalid("starlight spectral pixel index"));
        }
        Ok(slot)
    }

    // With one bin the marginal and native-band quantities are identical;
    // multiple bins require an independently supplied covariance-aware sigma.
    fn native_statistical_sigma(&self, pixel: usize) -> Option<f64> {
        self.integrated_statistical_sigma
            .as_ref()
            .map(|s| s[pixel])
            .or_else(|| {
                if self.bins() == 1 {
                    self.statistical_sigma.as_ref().map(|s| s[pixel])
                } else {
                    None
                }
            })
    }

    /// Full-band samples for constructing the immutable legacy API view.
    pub(super) fn integrated_pixels(&self) -> impl Iterator<Item = (f64, Option<f64>)> + '_ {
        self.radiance
            .chunks_exact(self.bins())
            .enumerate()
            .map(|(i, band)| (band.iter().sum(), self.native_statistical_sigma(i)))
    }

    pub(super) fn top_of_atmosphere_pixel(
        &self,
        direction: Direction<Galactic>,
    ) -> Result<(Radiance, Option<Radiance>)> {
        let index = self.pixel_index(direction)?;
        let bins = self.bins();
        let radiance = &self.radiance[index * bins..(index + 1) * bins];
        Ok((
            Radiance::new(radiance.iter().sum()),
            self.native_statistical_sigma(index).map(Radiance::new),
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
        let pixel = self.pixel_index(direction)?;
        let base = pixel * self.bins();
        let sigma = if start == 0 && end == self.edges_nm.len() - 1 {
            self.native_statistical_sigma(pixel)
        } else if end == start + 1 {
            self.statistical_sigma.as_ref().map(|s| s[base + start])
        } else {
            // Full covariance for this union is not packed.
            None
        };
        Ok((
            Radiance::new(self.radiance[base + start..base + end].iter().sum()),
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
        let pixel = self.pixel_index(direction)?;
        let base = pixel * self.bins();
        let radiance = &self.radiance[base..base + self.bins()];
        let sigma = self.statistical_sigma.as_ref().ok_or_else(|| {
            NsbError::Unsupported(
                "direct Starlight statistical envelope requires measured per-bin sigma".into(),
            )
        })?;
        let sigma = &sigma[base..base + self.bins()];
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
            lower += radiance[i] * lo;
            upper += radiance[i] * hi;
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

/// Necessary triangle and reverse-triangle bounds for the sigma of a sum.
/// Inputs have already passed shape and finite/nonnegative validation.
fn validate_statistical_consistency(per_bin: &[f64], total: f64) -> Result<()> {
    let largest = per_bin.iter().copied().fold(0.0, f64::max);
    let scale = largest.max(total);
    if scale == 0.0 {
        return Ok(());
    }
    // Normalize before summing to avoid overflow, including at f64::MAX.
    let sum: f64 = per_bin.iter().map(|s| s / scale).sum();
    let total = total / scale;
    let lower = (2.0 * (largest / scale) - sum).max(0.0);
    // Conservative O(n*epsilon) forward-error allowance for division, summation
    // and bound subtraction. No absolute floor: zero versus tiny sigma fails.
    let tolerance = 8.0 * f64::EPSILON * per_bin.len() as f64 * sum.max(total);
    if total > sum + tolerance || total + tolerance < lower {
        return Err(invalid(
            "statistical sigma violates covariance consistency bounds (single-bin sigmas must agree)",
        ));
    }
    Ok(())
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

    fn generic_fixture(edges: Vec<u16>, nside: u32, ordering: &str) -> serde_json::Value {
        let bins = edges.len() - 1;
        serde_json::json!({
            "schema":"nsb-starlight-spectral-map-v2",
            "product_id":"synthetic-arbitrary-bin-test",
            "band_nm":[edges[0], *edges.last().unwrap()],
            "bin_edges_nm":edges,
            "nside":nside, "ordering":ordering,
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
            "catalogue_rows":12*nside*nside, "admitted_sources":12*nside*nside, "excluded_sources":{},
            "pixels":(0..12*nside*nside).map(|i| serde_json::json!({
                "pixel":i,"sources":1,"radiance":vec![1.0;bins],
                "statistical_sigma":vec![0.1;bins],
                "integrated_statistical_sigma":0.1
            })).collect::<Vec<_>>()
        })
    }

    #[test]
    fn generic_schema_accepts_different_bin_counts_and_reports_covariance_limits() {
        for edges in [
            vec![350, 570],
            vec![350, 410, 470, 570],
            (0..=10).map(|i| 350 + i * 20).collect::<Vec<_>>(),
        ] {
            let bins = edges.len() - 1;
            let payload = generic_fixture(edges, 1, "ring");
            let map = load(payload).unwrap();
            assert_eq!(map.edges_nm.len(), bins + 1);
            // Channels are dense and pixel-major, independent of bin count.
            assert_eq!(map.radiance.len(), 12 * bins);
            assert_eq!(map.statistical_sigma.as_ref().unwrap().len(), 12 * bins);
            assert_eq!(map.integrated_statistical_sigma.as_ref().unwrap().len(), 12);
            let d = direction();
            let full = map
                .integrate_band(
                    d,
                    Nanometers::new(f64::from(map.edges_nm[0])),
                    Nanometers::new(f64::from(*map.edges_nm.last().unwrap())),
                )
                .unwrap();
            assert_eq!(full.0.value(), bins as f64);
            assert_eq!(full.1.unwrap().value(), 0.1);
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

    #[test]
    fn generic_uncertainty_channels_geometry_and_flux_conservation() {
        for bins in [1, 3, 7, 12] {
            let edges = (0..=bins).map(|i| 350 + i * 15).collect::<Vec<u16>>();
            for nside in [1, 2, 4] {
                for ordering in ["ring", "nested"] {
                    for (per_bin, full_band) in
                        [(false, false), (true, false), (false, true), (true, true)]
                    {
                        let mut value = generic_fixture(edges.clone(), nside, ordering);
                        for pixel in value["pixels"].as_array_mut().unwrap() {
                            if !per_bin {
                                pixel.as_object_mut().unwrap().remove("statistical_sigma");
                            }
                            if !full_band {
                                pixel
                                    .as_object_mut()
                                    .unwrap()
                                    .remove("integrated_statistical_sigma");
                            }
                        }
                        if ordering == "nested" {
                            assert!(load(value).unwrap_err().to_string().contains("NESTED"));
                            continue;
                        }
                        let map = load(value).unwrap();
                        let (flux, sigma) = map.top_of_atmosphere_pixel(direction()).unwrap();
                        assert_eq!(flux.value(), f64::from(bins));
                        assert_eq!(
                            sigma.map(|s| s.value()),
                            (full_band || (bins == 1 && per_bin)).then_some(0.1)
                        );
                        let mut sum = 0.0;
                        for edge in edges.windows(2) {
                            let (flux, sigma) = map
                                .integrate_band(
                                    direction(),
                                    Nanometers::new(f64::from(edge[0])),
                                    Nanometers::new(f64::from(edge[1])),
                                )
                                .unwrap();
                            sum += flux.value();
                            // A single-bin map's full-band covariance channel takes precedence.
                            let expected = if bins == 1 {
                                (full_band || (bins == 1 && per_bin)).then_some(0.1)
                            } else {
                                per_bin.then_some(0.1)
                            };
                            assert_eq!(sigma.map(|s| s.value()), expected);
                        }
                        assert_eq!(sum, f64::from(bins));
                        if !per_bin {
                            assert!(map
                                .direct_pixel_bounds(
                                    direction(),
                                    DirectPathGeometry::new(Degrees::new(0.0)).unwrap(),
                                    AtmosphericConditions::clear_sky_with_pressure(
                                        siderust::qtty::Hectopascals::new(790.0)
                                    )
                                    .unwrap()
                                )
                                .is_err());
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn generic_invalid_semantics_fail_closed() {
        let valid = generic_fixture(vec![350, 410, 470, 570], 1, "ring");
        for (field, bad) in [
            (
                "schema",
                serde_json::json!("nsb-starlight-spectral-map-v99"),
            ),
            ("bin_edges_nm", serde_json::json!([350, 410, 410, 570])),
            ("band_nm", serde_json::json!([300, 650])),
            ("nside", serde_json::json!(3)),
            ("ordering", serde_json::json!("unknown")),
            ("coordinate_frame", serde_json::json!("icrs")),
            ("domain", serde_json::json!("ground-level")),
            ("units", serde_json::json!("watt/(m2 sr)")),
            ("maturity", serde_json::json!("production")),
        ] {
            let mut bad_value = valid.clone();
            bad_value[field] = bad;
            assert!(load(bad_value).is_err(), "accepted invalid {field}");
        }
        for field in ["statistical_sigma", "integrated_statistical_sigma"] {
            let mut mixed = valid.clone();
            mixed["pixels"][0].as_object_mut().unwrap().remove(field);
            assert!(load(mixed).is_err(), "accepted mixed {field}");
        }
        let mut missing = valid.clone();
        missing["pixels"].as_array_mut().unwrap().pop();
        assert!(load(missing).is_err());
        let mut duplicate = valid.clone();
        duplicate["pixels"][1]["pixel"] = 0.into();
        assert!(load(duplicate).is_err());
        let mut negative = valid.clone();
        negative["pixels"][0]["radiance"][0] = (-1.0).into();
        assert!(load(negative).is_err());
        let mut account = valid;
        account["admitted_sources"] = 11.into();
        assert!(load(account).is_err());
    }

    #[test]
    fn generic_provenance_and_domain_errors_preserve_scientific_boundaries() {
        let mut valid = generic_fixture(vec![360, 390, 450, 570], 1, "ring");
        valid["provenance"]["gaia"] = serde_json::json!({
            "design_sha256":"a".repeat(64), "xp_rows":12, "unmatched_xp_rows":0,
            "input_partitions":{"p":{"shard_sha256":"b".repeat(64),
                "inputs":{"gaia-source":"c".repeat(64), "xp-continuous":"d".repeat(64)}}}
        });
        let map = load(valid.clone()).unwrap();
        for (low, high) in [
            (f64::NAN, 570.0),
            (360.0, f64::INFINITY),
            (570.0, 360.0),
            (300.0, 570.0),
            (360.0, 650.0),
            (360.0, 569.0),
        ] {
            assert!(map
                .integrate_band(direction(), Nanometers::new(low), Nanometers::new(high))
                .is_err());
        }
        let mut bad = valid.clone();
        bad["provenance"]["gaia"]["design_sha256"] = "invalid".into();
        assert!(load(bad).is_err());
        let mut bad = valid.clone();
        bad["provenance"]["gaia"]["input_partitions"] = serde_json::json!({});
        assert!(load(bad).is_err());
        let mut bad = valid.clone();
        bad["provenance"]["gaia"]["unmatched_xp_rows"] = 13.into();
        assert!(load(bad).is_err());
        let mut bad = valid.clone();
        bad["provenance"]["gaia"]["unmatched_xp_rows"] = 1.into();
        assert!(
            load(bad).is_err(),
            "admitted sources cannot exceed matched XP rows"
        );
        let mut bad = valid.clone();
        bad["provenance"]["gaia"]["input_partitions"]["p"]["inputs"]["xp-continuous"] =
            "invalid".into();
        assert!(load(bad).is_err());
        for field in [
            "source_catalogue",
            "source_license",
            "source_selection",
            "generation_method",
        ] {
            let mut bad = valid.clone();
            bad["provenance"][field] = " ".into();
            assert!(load(bad).is_err());
        }
        let mut bad = valid.clone();
        bad["provenance"]["limitations"] = serde_json::json!([]);
        assert!(load(bad).is_err());
        let mut bad = valid.clone();
        bad["excluded_sources"] = serde_json::json!({"overflow-a":u64::MAX,"overflow-b":1});
        assert!(load(bad).is_err());
        let mut bad = valid;
        bad["pixels"][0]["sources"] = u64::MAX.into();
        assert!(load(bad).is_err());
    }

    fn sigma_fixture(
        edges: Vec<u16>,
        per_bin: Option<Vec<f64>>,
        total: Option<f64>,
    ) -> serde_json::Value {
        let mut value = generic_fixture(edges, 1, "ring");
        for pixel in value["pixels"].as_array_mut().unwrap() {
            pixel["statistical_sigma"] = serde_json::json!(per_bin);
            pixel["integrated_statistical_sigma"] = serde_json::json!(total);
        }
        value
    }

    #[test]
    fn native_sigma_channels_preserve_known_information_through_public_adapters() {
        use crate::components::starlight::StarlightMap;
        for edges in [vec![300, 650], vec![360, 570], vec![450, 451]] {
            for (per_bin, total, expected, valid) in [
                (None, None, None, true),
                (Some(vec![0.1]), None, Some(0.1), true),
                (None, Some(0.1), Some(0.1), true),
                (Some(vec![0.1]), Some(0.1), Some(0.1), true),
                (Some(vec![0.1]), Some(0.5), None, false),
            ] {
                let value = sigma_fixture(edges.clone(), per_bin, total);
                if !valid {
                    assert!(matches!(load(value), Err(NsbError::InvalidMap { .. })));
                    continue;
                }
                let bytes = serde_json::to_vec(&value).unwrap();
                let sha = Sha256::digest(&bytes)
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>();
                let map = StarlightMap::from_json_bytes(&bytes, &sha).unwrap();
                let native = map.top_of_atmosphere_pixel(direction()).unwrap();
                assert_eq!(native.1.map(|s| s.value()), expected);
                assert_eq!(
                    map.integrate_band(
                        direction(),
                        Nanometers::new(f64::from(edges[0])),
                        Nanometers::new(f64::from(edges[1]))
                    )
                    .unwrap(),
                    native
                );
                let pixel = map.try_pixel_at(direction()).unwrap();
                assert_eq!(pixel.integrated, native.0);
                assert_eq!(pixel.statistical_uncertainty, native.1);
                assert!(
                    pixel.systematic_uncertainty.is_none() && pixel.total_uncertainty.is_none()
                );
            }
        }
        for (per_bin, total, expected) in [
            (Some(vec![0.1, 0.2, 0.3]), None, None),
            (None, Some(0.4), Some(0.4)),
            (Some(vec![0.1, 0.2, 0.3]), Some(0.4), Some(0.4)),
        ] {
            let has_per_bin = per_bin.is_some();
            let value = sigma_fixture(vec![360, 380, 470, 570], per_bin, total);
            let map = load(value).unwrap();
            let native = map.top_of_atmosphere_pixel(direction()).unwrap();
            assert_eq!(native.1.map(|s| s.value()), expected);
            assert_eq!(
                map.integrate_band(direction(), Nanometers::new(360.0), Nanometers::new(570.0))
                    .unwrap(),
                native
            );
            assert_eq!(
                map.integrate_band(direction(), Nanometers::new(360.0), Nanometers::new(380.0))
                    .unwrap()
                    .1
                    .map(|s| s.value()),
                has_per_bin.then_some(0.1)
            );
            assert!(map
                .integrate_band(direction(), Nanometers::new(360.0), Nanometers::new(470.0))
                .unwrap()
                .1
                .is_none());
            assert_eq!(map.integrated_pixels().next().unwrap().1, expected);
        }
    }

    #[test]
    fn covariance_bounds_reject_impossible_maps_without_assuming_independence() {
        for (per_bin, total, valid) in [
            (vec![0.1, 0.2, 0.3], 0.8, false),
            (vec![0.5, 0.1], 0.1, false),
            (vec![0.5, 0.1], 0.4, true),
            (vec![0.5, 0.1], 0.6, true),
            (vec![1.0, 1.0], 0.0, true),
            (vec![0.0, 0.0], 0.0, true),
            (vec![0.0, 0.0], 1e-300, false),
            (vec![1e300, 1e300], 1e300, true),
            (vec![1e-300, 1e-300], 1e-300, true),
        ] {
            let edges = (0..=per_bin.len()).map(|i| 350 + 10 * i as u16).collect();
            let result = load(sigma_fixture(edges, Some(per_bin), Some(total)));
            assert_eq!(result.is_ok(), valid);
            if let Err(error) = result {
                assert!(matches!(error, NsbError::InvalidMap { .. }));
                assert!(error.to_string().contains("covariance consistency"));
            }
        }
    }

    #[test]
    fn covariance_comparisons_are_relative_and_avoid_overflow() {
        let tiny = f64::from_bits(1);
        for sigma in [0.0, tiny, 1e-300, 0.1, 1e300, f64::MAX] {
            assert!(validate_statistical_consistency(&[sigma], sigma).is_ok());
        }
        for sigma in [1e-300, 0.1, 1e300] {
            assert!(
                validate_statistical_consistency(&[sigma], sigma * (1.0 + 4.0 * f64::EPSILON))
                    .is_ok()
            );
            assert!(
                validate_statistical_consistency(&[sigma], sigma * (1.0 - 4.0 * f64::EPSILON))
                    .is_ok()
            );
            assert!(validate_statistical_consistency(
                &[sigma],
                sigma * (1.0 + 64.0 * f64::EPSILON)
            )
            .is_err());
            assert!(validate_statistical_consistency(
                &[sigma],
                sigma * (1.0 - 64.0 * f64::EPSILON)
            )
            .is_err());
        }
        assert!(validate_statistical_consistency(&[0.0], tiny).is_err());
        assert!(validate_statistical_consistency(&[tiny], 0.0).is_err());
        assert!(validate_statistical_consistency(&[tiny], 2.0 * tiny).is_err());
        assert!(validate_statistical_consistency(&[f64::MAX, f64::MAX], f64::MAX).is_ok());
        assert!(validate_statistical_consistency(&[f64::MAX, 0.0], 0.0).is_err());
        let sigmas = vec![1.0; 1024];
        assert!(validate_statistical_consistency(&sigmas, 1024.0).is_ok());
        assert!(
            validate_statistical_consistency(&sigmas, 1024.0 * (1.0 + 4.0 * f64::EPSILON)).is_ok()
        );
        assert!(validate_statistical_consistency(&sigmas, 1024.01).is_err());
    }

    #[test]
    fn nonfinite_and_negative_sigma_json_never_loads() {
        let value = sigma_fixture(vec![360, 570], Some(vec![0.125]), Some(0.125));
        let raw = serde_json::to_string(&value).unwrap();
        for token in ["NaN", "Infinity", "1e400", "-0.125"] {
            let bytes = raw.replace("0.125", token).into_bytes();
            let sha = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            assert!(SpectralMap::from_json_bytes(&bytes, &sha).is_err());
        }
    }
}
