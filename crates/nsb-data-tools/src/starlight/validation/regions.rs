//! Frozen, reproducible sky-region definitions for independent validation.
//!
//! Regions are described declaratively as formulas over NESTED HEALPix pixel
//! geometry (and, for two data-driven regions, the candidate map's own
//! per-pixel admitted-source count) rather than as opaque pixel lists. This
//! keeps the region set auditable, diffable, and independent of any single
//! `nside` materialization.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::f64::consts::PI;

pub const REGIONS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionsDocument {
    pub schema_version: u32,
    pub nside: u32,
    pub ordering: String,
    pub coordinate_frame: String,
    pub regions: Vec<RegionDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegionDefinition {
    pub id: String,
    pub description: String,
    pub selector: RegionSelector,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limitation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub enum RegionSelector {
    /// Every pixel in the HEALPix domain.
    All,
    /// Pixels whose absolute Galactic latitude falls in `[min, max]` degrees.
    LatitudeBand {
        min_abs_b_deg: f64,
        max_abs_b_deg: f64,
    },
    /// Pixels within `radius_deg` great-circle distance of one sky point.
    Cone {
        center_l_deg: f64,
        center_b_deg: f64,
        radius_deg: f64,
    },
    /// Union of several cones (e.g. several literature fields).
    ConeUnion { cones: Vec<ConeSpec> },
    /// Pixels whose Galactic longitude falls in a band that may wrap through
    /// zero, i.e. `min_l_deg > max_l_deg` selects `l >= min_l_deg OR
    /// l <= max_l_deg`.
    LongitudeBand { min_l_deg: f64, max_l_deg: f64 },
    /// Pixels in a percentile range of one candidate-map-derived per-pixel
    /// statistic. This is a reproducible, formula-based proxy computed fresh
    /// from whichever candidate map is supplied to `run`, not a frozen list.
    PixelStatPercentile {
        field: PixelStatField,
        min_percentile: f64,
        max_percentile: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PixelStatField {
    AdmittedSources,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConeSpec {
    pub center_l_deg: f64,
    pub center_b_deg: f64,
    pub radius_deg: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl RegionsDocument {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != REGIONS_SCHEMA_VERSION {
            bail!(
                "unsupported Starlight validation regions schema_version {}",
                self.schema_version
            );
        }
        if self.nside == 0 || !self.nside.is_power_of_two() {
            bail!("regions document nside must be a power of two");
        }
        if self.ordering != "nested" {
            bail!("regions document ordering must be nested");
        }
        if self.coordinate_frame != "galactic" {
            bail!("regions document coordinate_frame must be galactic");
        }
        if self.regions.is_empty() {
            bail!("regions document must define at least one region");
        }
        let mut ids = BTreeSet::new();
        for region in &self.regions {
            if region.id.trim().is_empty() {
                bail!("region id must not be empty");
            }
            if !ids.insert(region.id.as_str()) {
                bail!("duplicate region id {}", region.id);
            }
            if region.description.trim().is_empty() {
                bail!("region {} has an empty description", region.id);
            }
            region.selector.validate(&region.id)?;
        }
        if !ids.contains("all-sky") {
            bail!("regions document must define the all-sky region");
        }
        Ok(())
    }
}

impl RegionSelector {
    fn validate(&self, region_id: &str) -> Result<()> {
        match self {
            Self::All => {}
            Self::LatitudeBand {
                min_abs_b_deg,
                max_abs_b_deg,
            } => {
                if !min_abs_b_deg.is_finite()
                    || !max_abs_b_deg.is_finite()
                    || !(0.0..=90.0).contains(min_abs_b_deg)
                    || !(0.0..=90.0).contains(max_abs_b_deg)
                    || min_abs_b_deg >= max_abs_b_deg
                {
                    bail!("region {region_id} has an invalid latitude band");
                }
            }
            Self::Cone {
                center_l_deg,
                center_b_deg,
                radius_deg,
            } => validate_cone(region_id, *center_l_deg, *center_b_deg, *radius_deg)?,
            Self::ConeUnion { cones } => {
                if cones.is_empty() {
                    bail!("region {region_id} cone union must not be empty");
                }
                for cone in cones {
                    validate_cone(
                        region_id,
                        cone.center_l_deg,
                        cone.center_b_deg,
                        cone.radius_deg,
                    )?;
                }
            }
            Self::LongitudeBand {
                min_l_deg,
                max_l_deg,
            } => {
                if !min_l_deg.is_finite()
                    || !max_l_deg.is_finite()
                    || !(0.0..360.0).contains(min_l_deg)
                    || !(0.0..360.0).contains(max_l_deg)
                {
                    bail!("region {region_id} has an invalid longitude band");
                }
            }
            Self::PixelStatPercentile {
                min_percentile,
                max_percentile,
                ..
            } => {
                if !min_percentile.is_finite()
                    || !max_percentile.is_finite()
                    || !(0.0..=100.0).contains(min_percentile)
                    || !(0.0..=100.0).contains(max_percentile)
                    || min_percentile >= max_percentile
                {
                    bail!("region {region_id} has an invalid percentile range");
                }
            }
        }
        Ok(())
    }
}

fn validate_cone(
    region_id: &str,
    center_l_deg: f64,
    center_b_deg: f64,
    radius_deg: f64,
) -> Result<()> {
    if !center_l_deg.is_finite()
        || !center_b_deg.is_finite()
        || !radius_deg.is_finite()
        || !(0.0..360.0).contains(&center_l_deg)
        || !(-90.0..=90.0).contains(&center_b_deg)
        || !(0.0..=180.0).contains(&radius_deg)
    {
        bail!("region {region_id} has an invalid cone specification");
    }
    Ok(())
}

/// Deterministic evaluator resolving a frozen [`RegionsDocument`] against one
/// HEALPix pixel domain and one candidate map's per-pixel statistics.
pub struct RegionEngine {
    nside: u32,
    /// `(longitude_deg, latitude_deg)` for every pixel in the domain, indexed
    /// by pixel id.
    angles: Vec<(f64, f64)>,
}

impl RegionEngine {
    pub fn build(nside: u32) -> Result<Self> {
        if nside == 0 || !nside.is_power_of_two() {
            bail!("RegionEngine requires a power-of-two nside");
        }
        let domain = pixel_domain_size(nside)?;
        let angles = (0..domain)
            .map(|pixel| pix2ang_nested(nside, pixel))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { nside, angles })
    }

    pub fn nside(&self) -> u32 {
        self.nside
    }

    pub fn domain_size(&self) -> u64 {
        self.angles.len() as u64
    }

    /// Resolve every declared region to its exact pixel membership set.
    pub fn resolve(
        &self,
        regions: &RegionsDocument,
        admitted_sources: &BTreeMap<u32, u64>,
    ) -> Result<BTreeMap<String, BTreeSet<u32>>> {
        if regions.nside != self.nside {
            bail!(
                "regions document nside {} does not match engine nside {}",
                regions.nside,
                self.nside
            );
        }
        let percentile_cache = self.admitted_source_percentiles(admitted_sources)?;
        let mut resolved = BTreeMap::new();
        for region in &regions.regions {
            let pixels = self.select(&region.selector, &percentile_cache)?;
            resolved.insert(region.id.clone(), pixels);
        }
        Ok(resolved)
    }

    fn select(&self, selector: &RegionSelector, percentiles: &[f64]) -> Result<BTreeSet<u32>> {
        let domain = self.angles.len() as u32;
        let mut pixels = BTreeSet::new();
        match selector {
            RegionSelector::All => {
                pixels.extend(0..domain);
            }
            RegionSelector::LatitudeBand {
                min_abs_b_deg,
                max_abs_b_deg,
            } => {
                for (pixel, (_, b)) in self.angles.iter().enumerate() {
                    let abs_b = b.abs();
                    if abs_b >= *min_abs_b_deg && abs_b <= *max_abs_b_deg {
                        pixels.insert(pixel as u32);
                    }
                }
            }
            RegionSelector::Cone {
                center_l_deg,
                center_b_deg,
                radius_deg,
            } => {
                for (pixel, (l, b)) in self.angles.iter().enumerate() {
                    if angular_separation_deg(*l, *b, *center_l_deg, *center_b_deg) <= *radius_deg {
                        pixels.insert(pixel as u32);
                    }
                }
            }
            RegionSelector::ConeUnion { cones } => {
                for (pixel, (l, b)) in self.angles.iter().enumerate() {
                    let inside = cones.iter().any(|cone| {
                        angular_separation_deg(*l, *b, cone.center_l_deg, cone.center_b_deg)
                            <= cone.radius_deg
                    });
                    if inside {
                        pixels.insert(pixel as u32);
                    }
                }
            }
            RegionSelector::LongitudeBand {
                min_l_deg,
                max_l_deg,
            } => {
                for (pixel, (l, _)) in self.angles.iter().enumerate() {
                    let inside = if min_l_deg <= max_l_deg {
                        *l >= *min_l_deg && *l <= *max_l_deg
                    } else {
                        *l >= *min_l_deg || *l <= *max_l_deg
                    };
                    if inside {
                        pixels.insert(pixel as u32);
                    }
                }
            }
            RegionSelector::PixelStatPercentile {
                min_percentile,
                max_percentile,
                ..
            } => {
                for (pixel, percentile) in percentiles.iter().enumerate() {
                    if *percentile >= *min_percentile && *percentile <= *max_percentile {
                        pixels.insert(pixel as u32);
                    }
                }
            }
        }
        Ok(pixels)
    }

    /// Percentile rank (0-100, ascending) of each pixel's admitted-source
    /// count within the full domain, treating unlisted pixels as zero per the
    /// candidate map's declared sparse semantics.
    fn admitted_source_percentiles(
        &self,
        admitted_sources: &BTreeMap<u32, u64>,
    ) -> Result<Vec<f64>> {
        let domain = self.angles.len();
        let mut ranked = (0..domain)
            .map(|pixel| {
                let value = admitted_sources.get(&(pixel as u32)).copied().unwrap_or(0);
                (value, pixel)
            })
            .collect::<Vec<_>>();
        ranked.sort_by_key(|(value, _)| *value);
        let mut percentiles = vec![0.0_f64; domain];
        for (rank, (_, pixel)) in ranked.into_iter().enumerate() {
            percentiles[pixel] = (rank + 1) as f64 / domain as f64 * 100.0;
        }
        Ok(percentiles)
    }
}

fn pixel_domain_size(nside: u32) -> Result<u32> {
    u32::try_from(12_u64 * u64::from(nside) * u64::from(nside))
        .context("HEALPix pixel-domain size exceeds u32")
}

fn angular_separation_deg(l1: f64, b1: f64, l2: f64, b2: f64) -> f64 {
    let (l1, b1, l2, b2) = (
        l1.to_radians(),
        b1.to_radians(),
        l2.to_radians(),
        b2.to_radians(),
    );
    // Vincenty formula for great-circle distance; numerically stable for both
    // very small and near-antipodal separations.
    let delta_l = l2 - l1;
    let numerator = ((b2.cos() * delta_l.sin()).powi(2)
        + (b1.cos() * b2.sin() - b1.sin() * b2.cos() * delta_l.cos()).powi(2))
    .sqrt();
    let denominator = b1.sin() * b2.sin() + b1.cos() * b2.cos() * delta_l.cos();
    numerator.atan2(denominator).to_degrees()
}

/// Galactic `(longitude_deg, latitude_deg)` of the center of a NESTED HEALPix
/// pixel, using the standard base-resolution face layout (Gorski et al.
/// 2005). This is an independent re-derivation, not a call into the
/// production map writer, so that validation tooling does not share a bug
/// with the code it is meant to check.
pub(crate) fn pix2ang_nested(nside: u32, pixel: u32) -> Result<(f64, f64)> {
    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

    if nside == 0 || !nside.is_power_of_two() {
        bail!("pix2ang requires a power-of-two nside");
    }
    let domain = pixel_domain_size(nside)?;
    if pixel >= domain {
        bail!("pixel {pixel} is outside the nside={nside} domain");
    }

    let pixels_per_face = nside * nside;
    let face = (pixel / pixels_per_face) as usize;
    let face_pixel = pixel % pixels_per_face;
    let mut x = 0_u32;
    let mut y = 0_u32;
    let mut source_bit = 0_u32;
    let mut coordinate_bit = 1_u32;
    while coordinate_bit < nside {
        x |= ((face_pixel >> source_bit) & 1) * coordinate_bit;
        y |= ((face_pixel >> (source_bit + 1)) & 1) * coordinate_bit;
        source_bit += 2;
        coordinate_bit <<= 1;
    }

    let nside_i64 = i64::from(nside);
    let nside_f64 = f64::from(nside);
    let jrt = i64::from(x) + i64::from(y);
    let jr = JRLL[face] * nside_i64 - jrt - 1;

    let (nr, z, kshift) = if jr < nside_i64 {
        let nr = jr;
        let z = 1.0 - (nr * nr) as f64 / (3.0 * nside_f64 * nside_f64);
        (nr, z, 0_i64)
    } else if jr > 3 * nside_i64 {
        let nr = 4 * nside_i64 - jr;
        let z = -1.0 + (nr * nr) as f64 / (3.0 * nside_f64 * nside_f64);
        (nr, z, 0_i64)
    } else {
        let nr = nside_i64;
        let z = (2 * nside_i64 - jr) as f64 * (2.0 / (3.0 * nside_f64));
        let kshift = (jr - nside_i64) & 1;
        (nr, z, kshift)
    };

    let mut jp = (JPLL[face] * nr + i64::from(x) - i64::from(y) + 1 + kshift) / 2;
    if jp > 4 * nside_i64 {
        jp -= 4 * nside_i64;
    }
    if jp < 1 {
        jp += 4 * nside_i64;
    }
    let phi = (jp as f64 - (kshift as f64 + 1.0) * 0.5) * (PI / 2.0) / nr as f64;

    let z = z.clamp(-1.0, 1.0);
    let latitude_deg = z.asin().to_degrees();
    let mut longitude_deg = phi.to_degrees();
    if longitude_deg < 0.0 {
        longitude_deg += 360.0;
    }
    if longitude_deg >= 360.0 {
        longitude_deg -= 360.0;
    }
    Ok((longitude_deg, latitude_deg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_resolution_pixel_centers_match_known_healpix_geometry() {
        // Known nside=1 NESTED pixel centers (Gorski et al. 2005, Fig. 4).
        let expected = [
            (45.0, 41.810_314_895_778_59),
            (135.0, 41.810_314_895_778_59),
            (225.0, 41.810_314_895_778_59),
            (315.0, 41.810_314_895_778_59),
            (0.0, 0.0),
            (90.0, 0.0),
            (180.0, 0.0),
            (270.0, 0.0),
            (45.0, -41.810_314_895_778_59),
            (135.0, -41.810_314_895_778_59),
            (225.0, -41.810_314_895_778_59),
            (315.0, -41.810_314_895_778_59),
        ];
        for (pixel, (expected_l, expected_b)) in expected.into_iter().enumerate() {
            let (l, b) = pix2ang_nested(1, pixel as u32).unwrap();
            assert!((l - expected_l).abs() < 1.0e-9, "pixel {pixel} longitude");
            assert!((b - expected_b).abs() < 1.0e-9, "pixel {pixel} latitude");
        }
    }

    #[test]
    fn pixel_out_of_domain_is_rejected() {
        assert!(pix2ang_nested(1, 12).is_err());
        assert!(pix2ang_nested(3, 0).is_err());
    }

    #[test]
    fn all_sky_selects_the_full_domain() {
        let engine = RegionEngine::build(4).unwrap();
        let regions = RegionsDocument {
            schema_version: REGIONS_SCHEMA_VERSION,
            nside: 4,
            ordering: "nested".to_string(),
            coordinate_frame: "galactic".to_string(),
            regions: vec![RegionDefinition {
                id: "all-sky".to_string(),
                description: "test".to_string(),
                selector: RegionSelector::All,
                limitation: None,
            }],
        };
        let resolved = engine.resolve(&regions, &BTreeMap::new()).unwrap();
        assert_eq!(
            resolved.get("all-sky").unwrap().len() as u64,
            engine.domain_size()
        );
    }

    #[test]
    fn latitude_band_selects_poles_symmetrically() {
        let engine = RegionEngine::build(8).unwrap();
        let poles = RegionSelector::LatitudeBand {
            min_abs_b_deg: 60.0,
            max_abs_b_deg: 90.0,
        };
        let pixels = engine.select(&poles, &[]).unwrap();
        assert!(!pixels.is_empty());
        for pixel in pixels {
            let (_, b) = engine.angles[pixel as usize];
            assert!(b.abs() >= 60.0);
        }
    }

    #[test]
    fn longitude_band_wraps_through_zero() {
        let engine = RegionEngine::build(8).unwrap();
        let seam = RegionSelector::LongitudeBand {
            min_l_deg: 355.0,
            max_l_deg: 5.0,
        };
        let pixels = engine.select(&seam, &[]).unwrap();
        assert!(!pixels.is_empty());
        for pixel in pixels {
            let (l, _) = engine.angles[pixel as usize];
            assert!(l >= 355.0 || l <= 5.0);
        }
    }

    #[test]
    fn galactic_center_and_anticenter_cones_are_disjoint() {
        let engine = RegionEngine::build(16).unwrap();
        let center = RegionSelector::Cone {
            center_l_deg: 0.0,
            center_b_deg: 0.0,
            radius_deg: 15.0,
        };
        let anticenter = RegionSelector::Cone {
            center_l_deg: 180.0,
            center_b_deg: 0.0,
            radius_deg: 15.0,
        };
        let center_pixels = engine.select(&center, &[]).unwrap();
        let anticenter_pixels = engine.select(&anticenter, &[]).unwrap();
        assert!(!center_pixels.is_empty());
        assert!(!anticenter_pixels.is_empty());
        assert!(center_pixels.is_disjoint(&anticenter_pixels));
    }

    #[test]
    fn percentile_selector_reflects_admitted_source_ranking() {
        let engine = RegionEngine::build(2).unwrap();
        let domain = engine.domain_size();
        let admitted = (0..domain)
            .map(|pixel| (pixel as u32, pixel + 1))
            .collect::<BTreeMap<_, _>>();
        let top_decile = RegionSelector::PixelStatPercentile {
            field: PixelStatField::AdmittedSources,
            min_percentile: 90.0,
            max_percentile: 100.0,
        };
        let percentiles = engine.admitted_source_percentiles(&admitted).unwrap();
        let pixels = engine.select(&top_decile, &percentiles).unwrap();
        // With a strictly increasing admitted-source count, the top decile
        // must contain the single highest-indexed pixel.
        assert!(pixels.contains(&(domain as u32 - 1)));
    }

    #[test]
    fn regions_document_requires_all_sky_and_rejects_duplicates() {
        let mut document = RegionsDocument {
            schema_version: REGIONS_SCHEMA_VERSION,
            nside: 8,
            ordering: "nested".to_string(),
            coordinate_frame: "galactic".to_string(),
            regions: vec![RegionDefinition {
                id: "poles".to_string(),
                description: "test".to_string(),
                selector: RegionSelector::LatitudeBand {
                    min_abs_b_deg: 60.0,
                    max_abs_b_deg: 90.0,
                },
                limitation: None,
            }],
        };
        assert!(document.validate().is_err());
        document.regions.push(RegionDefinition {
            id: "all-sky".to_string(),
            description: "test".to_string(),
            selector: RegionSelector::All,
            limitation: None,
        });
        document.validate().unwrap();
        document.regions.push(document.regions[0].clone());
        assert!(document.validate().is_err());
    }
}
