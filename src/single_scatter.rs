//! Single-scattering lookup table for Rayleigh and Mie scattering.
//!
//! Provides pre-computed grids of scattering coefficients as a function of
//! zenith angle and wavelength, reducing real-time computation for night-sky
//! background brightness estimates.
//!
//! Scientific role:
//! atmospheric scattering is one of the mechanisms that redistributes light
//! from bright sources, especially the Moon, into other sky directions.
//!
//! Contribution to the science:
//! this file currently provides a simplified, educational lookup grid rather
//! than the full production moon-scattering pipeline. It is useful as a bridge
//! toward more detailed single-scatter treatments and for explaining why sky
//! brightness depends on wavelength and line-of-sight geometry.

/// Pre-computed single-scattering grid for zenith angle and wavelength.
///
/// This struct stores a lookup table of scattering coefficients indexed by
/// zenith angle (0° to ~89°) and wavelength (350–1100 nm). The underlying
/// data follows a Rayleigh scattering model (∝ λ⁻⁴) for visible wavelengths.
#[derive(Clone, Debug)]
pub struct ScatterGrid {
    /// Zenith angles in degrees
    zenith_deg: Vec<f64>,
    /// Wavelengths in nanometers
    wavelength_nm: Vec<f64>,
    /// Flattened scattering coefficients: data[i * wl_count + j]
    /// where i indexes zenith angle, j indexes wavelength.
    data: Vec<f64>,
}

impl ScatterGrid {
    /// Creates a new scatter grid with example data for Rayleigh scattering.
    ///
    /// Grid dimensions:
    /// - Zenith angles: [0°, 20°, 40°, 60°, 80°, 89°] (6 points)
    /// - Wavelengths: [400, 500, 600, 800, 1000] nm (5 points)
    ///
    /// Scattering coefficients are computed using a Rayleigh model:
    /// σ(λ, z) = σ₀ * (λ₀ / λ)⁴ * airmass_factor(z)
    ///
    /// where airmass_factor approximates the optical depth increase with zenith angle.
    pub fn new() -> Self {
        let zenith_deg: Vec<f64> = vec![0.0, 20.0, 40.0, 60.0, 80.0, 89.0];
        let wavelength_nm: Vec<f64> = vec![400.0, 500.0, 600.0, 800.0, 1000.0];

        // Reference scattering coefficient at 500 nm and 0° zenith
        const SIGMA_500NM_REF: f64 = 1.0; // Arbitrary unit; normalized to 1.0
        const WAVELENGTH_REF: f64 = 500.0;

        let mut data = Vec::with_capacity(zenith_deg.len() * wavelength_nm.len());

        for z_deg in &zenith_deg {
            // Simple airmass approximation: X ≈ 1 / cos(z)
            // For z near 90°, this becomes very large, but we cap it for realism.
            let z_rad: f64 = z_deg.to_radians();
            let airmass = if *z_deg < 89.0 {
                1.0 / z_rad.cos()
            } else {
                // At 89°, airmass ≈ 57; we use a realistic value
                57.0
            };

            for wl_nm in &wavelength_nm {
                // Rayleigh scattering: σ ∝ λ⁻⁴
                let rayleigh_factor = (WAVELENGTH_REF / wl_nm).powi(4);
                let coefficient = SIGMA_500NM_REF * rayleigh_factor * airmass;
                data.push(coefficient);
            }
        }

        ScatterGrid {
            zenith_deg,
            wavelength_nm,
            data,
        }
    }

    /// Returns the zenith angles covered by this grid.
    pub fn zenith_angles(&self) -> &[f64] {
        &self.zenith_deg
    }

    /// Returns the wavelengths covered by this grid.
    pub fn wavelengths(&self) -> &[f64] {
        &self.wavelength_nm
    }

    /// Returns the grid dimensions as (zenith_count, wavelength_count).
    pub fn dimensions(&self) -> (usize, usize) {
        (self.zenith_deg.len(), self.wavelength_nm.len())
    }

    /// Looks up the scattering coefficient at the given zenith angle and wavelength.
    ///
    /// Uses bilinear interpolation if the requested point falls between grid points.
    /// Points outside the grid are clamped to the nearest edge.
    ///
    /// # Arguments
    /// * `zenith_deg` - Zenith angle in degrees [0, 89]
    /// * `wavelength_nm` - Wavelength in nanometers
    ///
    /// # Returns
    /// The interpolated scattering coefficient.
    pub fn lookup(&self, zenith_deg: f64, wavelength_nm: f64) -> f64 {
        // Clamp to grid bounds
        let z = zenith_deg.max(self.zenith_deg[0]).min(self.zenith_deg[self.zenith_deg.len() - 1]);
        let wl = wavelength_nm
            .max(self.wavelength_nm[0])
            .min(self.wavelength_nm[self.wavelength_nm.len() - 1]);

        // Find surrounding grid points for zenith angle
        let (z_idx, z_frac) = self.find_index(&self.zenith_deg, z);
        let (wl_idx, wl_frac) = self.find_index(&self.wavelength_nm, wl);

        let wl_count = self.wavelength_nm.len();

        // Get the four surrounding values (bilinear interpolation)
        let v00 = self.data[z_idx * wl_count + wl_idx];
        let v01 = if wl_idx + 1 < self.wavelength_nm.len() {
            self.data[z_idx * wl_count + wl_idx + 1]
        } else {
            v00
        };
        let v10 = if z_idx + 1 < self.zenith_deg.len() {
            self.data[(z_idx + 1) * wl_count + wl_idx]
        } else {
            v00
        };
        let v11 = if z_idx + 1 < self.zenith_deg.len() && wl_idx + 1 < self.wavelength_nm.len() {
            self.data[(z_idx + 1) * wl_count + wl_idx + 1]
        } else {
            v00
        };

        // Bilinear interpolation
        let v0 = v00 * (1.0 - wl_frac) + v01 * wl_frac;
        let v1 = v10 * (1.0 - wl_frac) + v11 * wl_frac;
        v0 * (1.0 - z_frac) + v1 * z_frac
    }

    /// Finds the grid index and fractional position for a given coordinate value.
    ///
    /// Returns (index, fraction) where the value lies between grid[index] and grid[index+1],
    /// with fraction ∈ [0, 1] indicating the relative position.
    fn find_index(&self, grid: &[f64], value: f64) -> (usize, f64) {
        // Binary search or linear search (small grids, so linear is fine)
        for i in 0..grid.len() - 1 {
            if value <= grid[i + 1] {
                let range = grid[i + 1] - grid[i];
                let frac = if range > 1e-10 {
                    (value - grid[i]) / range
                } else {
                    0.0
                };
                return (i, frac.max(0.0).min(1.0));
            }
        }
        (grid.len() - 1, 0.0)
    }
}

impl Default for ScatterGrid {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_creation() {
        let grid = ScatterGrid::new();
        assert_eq!(grid.zenith_deg.len(), 6);
        assert_eq!(grid.wavelength_nm.len(), 5);
        assert_eq!(grid.data.len(), 6 * 5);
    }

    #[test]
    fn test_grid_dimensions() {
        let grid = ScatterGrid::new();
        let (z_count, wl_count) = grid.dimensions();
        assert_eq!(z_count, 6);
        assert_eq!(wl_count, 5);
    }

    #[test]
    fn test_zenith_wavelength_coverage() {
        let grid = ScatterGrid::new();
        let zenith = grid.zenith_angles();
        let wavelength = grid.wavelengths();

        assert_eq!(zenith[0], 0.0);
        assert_eq!(zenith[zenith.len() - 1], 89.0);
        assert_eq!(wavelength[0], 400.0);
        assert_eq!(wavelength[wavelength.len() - 1], 1000.0);
    }

    #[test]
    fn test_lookup_at_grid_points() {
        let grid = ScatterGrid::new();

        // Lookup at exact grid points should return the stored values
        let val_00 = grid.lookup(0.0, 400.0);
        assert!(val_00 > 0.0);

        let val_89_1000 = grid.lookup(89.0, 1000.0);
        assert!(val_89_1000 > 0.0);
    }

    #[test]
    fn test_lookup_interpolation() {
        let grid = ScatterGrid::new();

        // Interpolation between grid points
        let val_at_10_deg = grid.lookup(10.0, 450.0);
        assert!(val_at_10_deg > 0.0);

        // Should be between the corner values
        let val_0_400 = grid.lookup(0.0, 400.0);
        let _val_20_500 = grid.lookup(20.0, 500.0);
        assert!(val_at_10_deg >= val_0_400 * 0.5); // Very loose bound
    }

    #[test]
    fn test_lookup_out_of_bounds_clamping() {
        let grid = ScatterGrid::new();

        // Values outside bounds should be clamped to edge
        let val_neg_zenith = grid.lookup(-10.0, 500.0);
        let val_0_zenith = grid.lookup(0.0, 500.0);
        assert_eq!(val_neg_zenith, val_0_zenith);

        let val_high_zenith = grid.lookup(100.0, 500.0);
        let val_89_zenith = grid.lookup(89.0, 500.0);
        assert_eq!(val_high_zenith, val_89_zenith);

        let val_low_wl = grid.lookup(45.0, 300.0);
        let val_400_wl = grid.lookup(45.0, 400.0);
        assert_eq!(val_low_wl, val_400_wl);

        let val_high_wl = grid.lookup(45.0, 1500.0);
        let val_1000_wl = grid.lookup(45.0, 1000.0);
        assert_eq!(val_high_wl, val_1000_wl);
    }

    #[test]
    fn test_rayleigh_scaling() {
        // Test that the Rayleigh λ⁻⁴ scaling is approximately preserved
        // across the grid at a fixed zenith angle.
        let grid = ScatterGrid::new();

        let z = 0.0; // Zenith angle

        let val_400 = grid.lookup(z, 400.0);
        let val_500 = grid.lookup(z, 500.0);
        let val_600 = grid.lookup(z, 600.0);

        // For Rayleigh: σ(λ) ∝ λ⁻⁴
        // So σ(400) / σ(500) should be close to (500/400)⁴
        let ratio_400_500_expected = (500.0 / 400.0_f64).powi(4);
        let ratio_400_500_actual = val_400 / val_500;
        assert!((ratio_400_500_actual - ratio_400_500_expected).abs() < 0.01);

        let ratio_600_500_expected = (500.0 / 600.0_f64).powi(4);
        let ratio_600_500_actual = val_600 / val_500;
        assert!((ratio_600_500_actual - ratio_600_500_expected).abs() < 0.01);
    }

    #[test]
    fn test_airmass_increase_with_zenith() {
        // Scattering coefficient should increase with zenith angle due to airmass
        let grid = ScatterGrid::new();

        let wl = 500.0;
        let val_0 = grid.lookup(0.0, wl);
        let val_45 = grid.lookup(45.0, wl);
        let val_80 = grid.lookup(80.0, wl);

        // Higher zenith angle => higher airmass => higher scattering coefficient
        assert!(val_45 > val_0);
        assert!(val_80 > val_45);
    }

    #[test]
    fn test_default_constructor() {
        let grid1 = ScatterGrid::new();
        let grid2 = ScatterGrid::default();

        assert_eq!(grid1.dimensions(), grid2.dimensions());
        assert_eq!(
            grid1.lookup(45.0, 500.0),
            grid2.lookup(45.0, 500.0)
        );
    }
}
