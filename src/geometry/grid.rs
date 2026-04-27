//! Alt/Az grid sampler used by the all-sky map functions.
//!
//! Mirrors the Python `Grid` class: a regular grid in altitude and azimuth
//! covering the visible hemisphere, used to integrate component spectra
//! over the sky.

#[derive(Debug, Clone)]
pub struct Grid {
    pub alt_step_deg: f64,
    pub az_step_deg: f64,
    pub alt_min_deg: f64,
    pub alt_max_deg: f64,
}

impl Default for Grid {
    fn default() -> Self {
        // Python defaults: alt 0..90 step 5°, az 0..360 step 5°.
        Self { alt_step_deg: 5.0, az_step_deg: 5.0, alt_min_deg: 0.0, alt_max_deg: 90.0 }
    }
}

impl Grid {
    /// Yield (alt_deg, az_deg) cells covering the hemisphere.
    pub fn iter(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let n_alt = ((self.alt_max_deg - self.alt_min_deg) / self.alt_step_deg).round() as usize;
        let n_az = (360.0 / self.az_step_deg).round() as usize;
        (0..n_alt).flat_map(move |i| (0..n_az).map(move |j| {
            (self.alt_min_deg + (i as f64 + 0.5) * self.alt_step_deg,
             (j as f64 + 0.5) * self.az_step_deg)
        }))
    }
}
