//! Shared Gaia-compatible HEALPix helpers backed by Siderust.
//!
//! Starlight accumulation uses nested HEALPix at or below Gaia DR3 level 12
//! (`nside <= 4096`). Geometry invariants (`npix`, index domain, power-of-two
//! nested `nside`) come from [`siderust::healpix`], not handwritten formulas.
//!
//! # Coordinate contract
//!
//! Gaia `source_id` embeds a level-12 nested HEALPix index in the ICRS /
//! equatorial frame used by the Gaia archive. The published Starlight candidate
//! map is accumulated in **Galactic** nested HEALPix at the configured output
//! `nside`. Selection-function lookup must use the artifact's declared frame
//! (production artifacts: equatorial nested at `healpix_nside`).

use anyhow::{bail, Context, Result};
use siderust::coordinates::cartesian::Direction;
use siderust::coordinates::frames::{Galactic, ReferenceFrame, ICRS};
use siderust::coordinates::spherical::direction;
use siderust::coordinates::transform::TransformFrame;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use siderust::qtty::Degrees;

/// Validated Gaia DR3 ICRS equatorial sky position in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IcrsSkyPosition {
    pub ra_deg: f64,
    pub dec_deg: f64,
}

impl IcrsSkyPosition {
    /// Parse and validate Gaia `ra` / `dec` fields in degrees.
    pub fn new(ra_deg: f64, dec_deg: f64) -> Result<Self> {
        let position = Self { ra_deg, dec_deg };
        position.validate()?;
        Ok(position)
    }

    /// Fail closed on non-finite or out-of-range Gaia astrometry.
    pub fn validate(self) -> Result<Self> {
        if !self.ra_deg.is_finite() || !(0.0..360.0).contains(&self.ra_deg) {
            bail!("ICRS right ascension must be finite and in [0, 360) degrees");
        }
        if !self.dec_deg.is_finite() || !(-90.0..=90.0).contains(&self.dec_deg) {
            bail!("ICRS declination must be finite and in [-90, 90] degrees");
        }
        Ok(self)
    }

    pub fn to_icrs_direction(self) -> Result<Direction<ICRS>> {
        self.validate()?;
        Ok(
            direction::ICRS::new(Degrees::new(self.ra_deg), Degrees::new(self.dec_deg))
                .to_cartesian(),
        )
    }
}

/// Gaia DR3 HEALPix order embedded in `source_id` (nside = 4096).
pub const GAIA_HEALPIX_ORDER: u32 = 12;
/// Largest nested `nside` Starlight accepts for Gaia-derived products.
pub const GAIA_MAX_NSIDE: u32 = 1 << GAIA_HEALPIX_ORDER;
/// Bit shift between Gaia `source_id` and its embedded level-12 HEALPix index.
pub const GAIA_SOURCE_ID_HEALPIX_SHIFT: u32 = 35;

/// Declared HEALPix coordinate frame for spatial artifacts and lookups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealpixCoordinateFrame {
    /// ICRS / Gaia equatorial HEALPix (embedded in `source_id`).
    Equatorial,
    /// Galactic HEALPix used by the Starlight candidate and runtime map.
    Galactic,
}

/// Declared HEALPix pixel ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealpixOrderingScheme {
    Nested,
    Ring,
}

impl HealpixOrderingScheme {
    pub fn to_siderust(self) -> HealpixOrdering {
        match self {
            Self::Nested => HealpixOrdering::Nested,
            Self::Ring => HealpixOrdering::Ring,
        }
    }
}

/// Validate a nested HEALPix `nside` usable for Gaia Starlight products.
pub fn gaia_nested_nside(nside: u32) -> Result<Nside> {
    let nside = Nside::new(nside).context("invalid HEALPix nside")?;
    if nside.get() > GAIA_MAX_NSIDE {
        bail!(
            "HEALPix nside {} exceeds Gaia level-12 maximum {GAIA_MAX_NSIDE}",
            nside.get()
        );
    }
    // Nested grids reject non-power-of-two nside.
    let _ = gaia_nested_grid_from_nside(nside)?;
    Ok(nside)
}

/// Build a nested HEALPix grid for Gaia Starlight products.
pub fn gaia_nested_grid(nside: u32) -> Result<HealpixGrid> {
    gaia_nested_grid_from_nside(gaia_nested_nside(nside)?)
}

fn gaia_nested_grid_from_nside(nside: Nside) -> Result<HealpixGrid> {
    HealpixGrid::new(nside, HealpixOrdering::Nested).with_context(|| {
        format!(
            "invalid nested HEALPix grid for nside {} (must be a power of two)",
            nside.get()
        )
    })
}

fn galactic_ring_grid(nside: u32) -> Result<HealpixGrid> {
    let nside = Nside::new(nside).context("invalid HEALPix nside")?;
    HealpixGrid::new(nside, HealpixOrdering::Ring).context("invalid Galactic RING grid")
}

/// Pixel count `12 * nside^2` for a Gaia nested grid.
pub fn gaia_nested_npix(nside: u32) -> Result<u64> {
    Ok(gaia_nested_grid(nside)?.npix())
}

/// NSIDE=2 parent of a nested pixel at `child_nside` (power-of-two, nested).
pub fn nested_parent_at_coarser_nside(
    pixel: u32,
    child_nside: u32,
    parent_nside: u32,
) -> Result<u32> {
    if !parent_nside.is_power_of_two()
        || !child_nside.is_power_of_two()
        || parent_nside >= child_nside
    {
        bail!("parent nside must be a power-of-two coarser than child nside");
    }
    let shift = 2 * (child_nside.trailing_zeros() - parent_nside.trailing_zeros());
    Ok(pixel >> shift)
}

/// Gaia `source_id` embedded nested HEALPix index in the equatorial frame.
pub fn gaia_source_id_equatorial_nested_pixel(source_id: u64, target_nside: u32) -> Result<u32> {
    gaia_nested_nside(target_nside)?;
    let target_order = target_nside.trailing_zeros();
    let level_12_pixel = source_id >> GAIA_SOURCE_ID_HEALPIX_SHIFT;
    let shift = 2 * (GAIA_HEALPIX_ORDER - target_order);
    u32::try_from(level_12_pixel >> shift).context("equatorial HEALPix pixel exceeds u32")
}

/// ICRS equatorial nested HEALPix pixel from GaiaSource `ra` / `dec` (degrees).
///
/// Production selection-function lookup must use this rather than the `source_id`
/// bit-shift when GaiaSource astrometry is available.
pub fn icrs_equatorial_nested_pixel(ra_deg: f64, dec_deg: f64, target_nside: u32) -> Result<u32> {
    let direction = IcrsSkyPosition::new(ra_deg, dec_deg)?.to_icrs_direction()?;
    let ring_grid = gaia_ring_grid(target_nside)?;
    let ring = ring_grid
        .direction_to_pixel(direction)
        .map_err(|error| anyhow::anyhow!("ICRS RING assignment failed: {error}"))?
        .get();
    u32::try_from(equatorial_ring_to_nested(target_nside, ring)?)
        .context("nested pixel exceeds u32")
}

fn gaia_ring_grid(nside: u32) -> Result<HealpixGrid> {
    let nside = Nside::new(nside).context("invalid HEALPix nside")?;
    HealpixGrid::new(nside, HealpixOrdering::Ring).context("invalid ICRS RING grid")
}

/// Convert nested index to RING at `nside` using the ICRS pixel-centre path.
pub fn equatorial_nested_to_ring(nside: u32, ipnest: u64) -> Result<u64> {
    let direction = nested_pixel_center::<ICRS>(nside, ipnest)?;
    let ring_grid = gaia_ring_grid(nside)?;
    Ok(ring_grid
        .direction_to_pixel(direction)
        .map_err(|error| anyhow::anyhow!("ICRS RING assignment failed: {error}"))?
        .get())
}

/// Convert RING index to nested at `nside` using the verified forward map.
pub fn equatorial_ring_to_nested(nside: u32, ipring: u64) -> Result<u64> {
    let table = equatorial_ring_to_nested_table(nside)?;
    table
        .get(ipring as usize)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("ring index {ipring} is outside nside={nside}"))
}

fn equatorial_ring_to_nested_table(nside: u32) -> Result<&'static [u64]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static TABLES: OnceLock<Mutex<HashMap<u32, &'static [u64]>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = tables.lock().expect("equatorial ring-to-nested table lock");
    if let Some(table) = guard.get(&nside) {
        return Ok(table);
    }
    let npix = gaia_nested_npix(nside)?;
    let mut table = vec![0_u64; usize::try_from(npix).context("npix fits usize")?];
    for nest in 0..npix {
        let ring = equatorial_nested_to_ring(nside, nest)?;
        let slot = usize::try_from(ring).context("ring index fits usize")?;
        if table[slot] != 0 && table[slot] != nest {
            bail!(
                "equatorial ring-to-nested inversion collision at ring {ring} for nested {nest} and {}",
                table[slot]
            );
        }
        table[slot] = nest;
    }
    let leaked = table.leak();
    guard.insert(nside, leaked);
    Ok(leaked)
}

/// Galactic nested HEALPix pixel from Gaia DR3 ICRS `ra` / `dec` (production contract).
pub fn galactic_nested_pixel_from_icrs_position(
    ra_deg: f64,
    dec_deg: f64,
    target_nside: u32,
) -> Result<u32> {
    let icrs = IcrsSkyPosition::new(ra_deg, dec_deg)?.to_icrs_direction()?;
    let galactic: Direction<Galactic> = icrs.to_frame();
    galactic_nested_pixel_from_direction(target_nside, galactic)
}

/// Approximate Galactic pixel from the `source_id` embedded equatorial HEALPix centre.
///
/// Diagnostic only: production accumulation must use [`galactic_nested_pixel_from_icrs_position`]
/// with GaiaSource `ra` / `dec`.
pub fn gaia_source_id_galactic_nested_pixel(source_id: u64, target_nside: u32) -> Result<u32> {
    let equatorial_nested =
        gaia_source_id_equatorial_nested_pixel(source_id, GAIA_MAX_NSIDE)? as u64;
    let equatorial_direction = nested_pixel_center::<ICRS>(GAIA_MAX_NSIDE, equatorial_nested)?;
    let galactic_direction: Direction<Galactic> = equatorial_direction.to_frame();
    galactic_nested_pixel_from_direction(target_nside, galactic_direction)
}

/// Legacy generator bug: equatorial `source_id` bit-shift used as a Galactic pixel.
pub fn legacy_equatorial_bitshift_mislabelled_as_galactic_pixel(
    source_id: u64,
    target_nside: u32,
) -> Result<u32> {
    gaia_source_id_equatorial_nested_pixel(source_id, target_nside)
}

/// Assign a Galactic direction to a nested pixel at `nside`.
pub fn galactic_nested_pixel_from_direction(
    nside: u32,
    direction: Direction<Galactic>,
) -> Result<u32> {
    let ring_grid = galactic_ring_grid(nside)?;
    let ring_index = ring_grid
        .direction_to_pixel(direction)
        .map_err(|error| anyhow::anyhow!("Galactic RING assignment failed: {error}"))?
        .get();
    u32::try_from(ring_to_nested(nside, ring_index)?).context("nested pixel exceeds u32")
}

/// Nested pixel centre as a typed unit direction (frame-neutral HEALPix `pix2ang`).
pub fn nested_pixel_center<F>(nside: u32, ipnest: u64) -> Result<Direction<F>>
where
    F: ReferenceFrame,
{
    gaia_nested_grid(nside)?
        .validate_index(HealpixIndex::new(ipnest))
        .map_err(|error| {
            anyhow::anyhow!("nested pixel {ipnest} is outside nside={nside}: {error}")
        })?;
    let nside = i64::from(nside);
    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];
    let npface = nside * nside;
    let ipnest = i64::try_from(ipnest).context("nested index fits i64")?;
    let face = ipnest / npface;
    let ipf = ipnest % npface;
    let mut ix = 0i64;
    let mut iy = 0i64;
    for bit in 0..16 {
        ix |= ((ipf >> (2 * bit)) & 1) << bit;
        iy |= ((ipf >> (2 * bit + 1)) & 1) << bit;
    }
    let jr = JRLL[usize::try_from(face).expect("face fits")] * nside - ix - iy - 1;
    let nl4 = 4 * nside;
    let nside_f = nside as f64;
    let fact1 = 1.0 / (1.5 * nside_f);
    let fact2 = 1.0 / (3.0 * nside_f * nside_f);
    let (nr, z, kshift) = if jr < nside {
        let nr = jr;
        (nr, 1.0 - (nr as f64) * (nr as f64) * fact2, 0)
    } else if jr > 3 * nside {
        let nr = nl4 - jr;
        (nr, (nr as f64) * (nr as f64) * fact2 - 1.0, 0)
    } else {
        (nside, (2.0 * nside_f - jr as f64) * fact1, (jr - nside) & 1)
    };
    let mut jp = (JPLL[usize::try_from(face).expect("face fits")] * nr + ix - iy + 1 + kshift) / 2;
    if jp > nl4 {
        jp -= nl4;
    }
    if jp < 1 {
        jp += nl4;
    }
    let phi = (jp as f64 - (kshift as f64 + 1.0) * 0.5) * std::f64::consts::FRAC_PI_2 / nr as f64;
    let z = z.clamp(-1.0, 1.0);
    let sin_theta = (1.0 - z * z).max(0.0).sqrt();
    Ok(Direction::<F>::from_array([
        sin_theta * phi.cos(),
        sin_theta * phi.sin(),
        z,
    ]))
}

/// Angular separation in radians between two unit directions.
pub fn angular_separation_rad<F: ReferenceFrame>(a: Direction<F>, b: Direction<F>) -> f64 {
    let [ax, ay, az] = a.as_array();
    let [bx, by, bz] = b.as_array();
    (ax * bx + ay * by + az * bz).clamp(-1.0, 1.0).acos()
}

/// Convert nested index to RING at `nside` using the Galactic pixel-centre path.
pub fn galactic_nested_to_ring(nside: u32, ipnest: u64) -> Result<u64> {
    let direction = nested_pixel_center::<Galactic>(nside, ipnest)?;
    let ring_grid = galactic_ring_grid(nside)?;
    Ok(ring_grid
        .direction_to_pixel(direction)
        .map_err(|error| anyhow::anyhow!("Galactic RING assignment failed: {error}"))?
        .get())
}

/// Convert RING index to nested at `nside` using the verified forward map.
pub fn ring_to_nested(nside: u32, ipring: u64) -> Result<u64> {
    let table = ring_to_nested_table(nside)?;
    table
        .get(ipring as usize)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("ring index {ipring} is outside nside={nside}"))
}

fn ring_to_nested_table(nside: u32) -> Result<&'static [u64]> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static TABLES: OnceLock<Mutex<HashMap<u32, &'static [u64]>>> = OnceLock::new();
    let tables = TABLES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = tables.lock().expect("ring-to-nested table lock");
    if let Some(table) = guard.get(&nside) {
        return Ok(table);
    }
    let npix = gaia_nested_npix(nside)?;
    let mut table = vec![0_u64; usize::try_from(npix).context("npix fits usize")?];
    for nest in 0..npix {
        let ring = galactic_nested_to_ring(nside, nest)?;
        let slot = usize::try_from(ring).context("ring index fits usize")?;
        if table[slot] != 0 && table[slot] != nest {
            bail!(
                "ring-to-nested inversion collision at ring {ring} for nested {nest} and {}",
                table[slot]
            );
        }
        table[slot] = nest;
    }
    let leaked: &'static [u64] = Box::leak(table.into_boxed_slice());
    guard.insert(nside, leaked);
    Ok(leaked)
}

/// Independent integer NESTED -> RING reference (Gorski et al. 2005).
pub fn reference_nest2ring(nside: u32, ipnest: u64) -> u64 {
    assert!(nside.is_power_of_two() && nside > 0);
    let nside = i64::from(nside);
    let npface = nside * nside;
    let npix = 12 * npface;
    let ipnest = i64::try_from(ipnest).expect("nested index fits i64");
    assert!((0..npix).contains(&ipnest));

    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

    let face = usize::try_from(ipnest / npface).expect("face fits usize");
    let ipf = u64::try_from(ipnest % npface).expect("face-local index fits u64");
    let mut ix = 0_u64;
    let mut iy = 0_u64;
    for bit in 0..32_u32 {
        ix |= ((ipf >> (2 * bit)) & 1) << bit;
        iy |= ((ipf >> (2 * bit + 1)) & 1) << bit;
    }
    let ix = i64::try_from(ix).expect("x fits i64");
    let iy = i64::try_from(iy).expect("y fits i64");

    let jr = JRLL[face] * nside - ix - iy - 1;
    let nl4 = 4 * nside;
    let (nr, n_before, kshift) = if jr < nside {
        let nr = jr;
        (nr, 2 * nr * (nr - 1), 0)
    } else if jr > 3 * nside {
        let nr = nl4 - jr;
        (nr, npix - 2 * nr * (nr + 1), 0)
    } else {
        (
            nside,
            2 * nside * (nside - 1) + (jr - nside) * nl4,
            (jr - nside) & 1,
        )
    };

    let mut jp = (JPLL[face] * nr + ix - iy + 1 + kshift) / 2;
    if jp > nl4 {
        jp -= nl4;
    }
    if jp < 1 {
        jp += nl4;
    }

    u64::try_from(n_before + jp - 1).expect("RING index is non-negative")
}

#[cfg(test)]
pub(crate) fn fixture_icrs_from_source_id(source_id: u64) -> IcrsSkyPosition {
    let equatorial_nested =
        gaia_source_id_equatorial_nested_pixel(source_id, GAIA_MAX_NSIDE).unwrap() as u64;
    let direction = nested_pixel_center::<ICRS>(GAIA_MAX_NSIDE, equatorial_nested).unwrap();
    let spherical = direction.to_spherical();
    IcrsSkyPosition {
        ra_deg: spherical.azimuth.value(),
        dec_deg: spherical.polar.value(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::spherical::Direction as SphericalDirection;
    use siderust::qtty::Degrees;

    #[test]
    fn accepts_gaia_compatible_powers_of_two() {
        for nside in [1, 2, 128, 256, 4096] {
            let grid = gaia_nested_grid(nside).unwrap();
            assert_eq!(grid.nside().get(), nside);
            assert_eq!(grid.npix(), 12 * u64::from(nside) * u64::from(nside));
        }
    }

    #[test]
    fn rejects_zero_non_power_of_two_and_above_gaia_max() {
        for nside in [0, 3, 8192] {
            assert!(gaia_nested_nside(nside).is_err());
            assert!(gaia_nested_grid(nside).is_err());
        }
    }

    #[test]
    fn equatorial_and_galactic_pixels_differ_for_generic_source() -> Result<()> {
        let source_id = (123_456_u64 << GAIA_SOURCE_ID_HEALPIX_SHIFT) | 17;
        let equatorial = gaia_source_id_equatorial_nested_pixel(source_id, 128)?;
        let galactic = gaia_source_id_galactic_nested_pixel(source_id, 128)?;
        assert_ne!(
            equatorial, galactic,
            "equatorial and Galactic NSIDE=128 pixels must not be treated as interchangeable"
        );
        Ok(())
    }

    #[test]
    fn galactic_source_pixel_is_stable_for_known_direction() -> Result<()> {
        let source_id = 4_295_806_660_u64;
        let first = gaia_source_id_galactic_nested_pixel(source_id, 128)?;
        let second = gaia_source_id_galactic_nested_pixel(source_id, 128)?;
        assert_eq!(first, second);
        assert_ne!(
            first,
            gaia_source_id_equatorial_nested_pixel(source_id, 128)?
        );
        Ok(())
    }

    #[test]
    fn nested_ring_round_trip_for_nside_128() -> Result<()> {
        let npix = gaia_nested_npix(128)?;
        for nest in [0_u64, 1, 127, 128, 12_345, npix - 1] {
            let ring = galactic_nested_to_ring(128, nest)?;
            let back = ring_to_nested(128, ring)?;
            assert_eq!(back, nest, "round-trip failed for nested {nest}");
        }
        Ok(())
    }

    #[test]
    fn galactic_direction_matches_siderust_ring_grid() -> Result<()> {
        let lon = 266.4051;
        let lat = -28.936175;
        let direction = SphericalDirection::<Galactic>::new(Degrees::new(lon), Degrees::new(lat))
            .to_cartesian();
        let ring = galactic_ring_grid(128)?
            .direction_to_pixel(direction)?
            .get();
        let nest = ring_to_nested(128, ring)?;
        let nested_dir = nested_pixel_center::<Galactic>(128, nest)?;
        let ring_dir = galactic_ring_grid(128)?.pixel_center(HealpixIndex::new(ring))?;
        assert!(
            angular_separation_rad(nested_dir, ring_dir) < 1.0e-7,
            "nested centre must match siderust RING centre"
        );
        Ok(())
    }

    #[test]
    fn reference_icrs_positions_assign_stable_galactic_pixels() -> Result<()> {
        let cases = [
            ("galactic_centre", 266.4051, -28.936175),
            ("north_celestial_pole", 0.0, 90.0),
            ("generic_gaia_source", 123.456, -12.345),
        ];
        for (label, ra, dec) in cases {
            let first = galactic_nested_pixel_from_icrs_position(ra, dec, 128)?;
            let second = galactic_nested_pixel_from_icrs_position(ra, dec, 128)?;
            assert_eq!(first, second, "{label} must be deterministic");
            let legacy_source_id = ((first as u64) << GAIA_SOURCE_ID_HEALPIX_SHIFT) | 7;
            let legacy =
                legacy_equatorial_bitshift_mislabelled_as_galactic_pixel(legacy_source_id, 128)?;
            assert_ne!(
                first, legacy,
                "{label} must not equal legacy equatorial bit-shift pixel"
            );
        }
        Ok(())
    }

    #[test]
    fn galactic_centre_icrs_transforms_to_galactic_origin() -> Result<()> {
        let position = IcrsSkyPosition::new(266.4051, -28.936175)?;
        let galactic: Direction<Galactic> = position.to_icrs_direction()?.to_frame();
        let spherical = galactic.to_spherical();
        assert!(
            spherical.azimuth.value().abs() < 0.05,
            "Galactic longitude should be near 0°, got {}",
            spherical.azimuth.value()
        );
        assert!(
            spherical.polar.value().abs() < 0.05,
            "Galactic latitude should be near 0°, got {}",
            spherical.polar.value()
        );
        Ok(())
    }

    #[test]
    fn reference_nest2ring_matches_galactic_direction_path_on_samples() -> Result<()> {
        for nest in [0_u64, 1, 42, 1_234, 12_345] {
            assert_eq!(
                reference_nest2ring(128, nest),
                galactic_nested_to_ring(128, nest)?
            );
        }
        Ok(())
    }
}
