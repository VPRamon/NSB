/// Solar radio flux in solar flux units.
///
/// `1 SFU = 1e-22 W m^-2 Hz^-1`.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct SolarFluxUnits(f64);

impl SolarFluxUnits {
    /// Construct an F10.7 value in solar flux units.
    pub const fn new(value: f64) -> Self {
        Self(value)
    }

    /// Return the numeric value in solar flux units.
    pub const fn value(self) -> f64 {
        self.0
    }

    /// Return whether the value is finite and positive.
    pub fn is_valid(self) -> bool {
        self.0.is_finite() && self.0 > 0.0
    }
}

/// Solar radio flux for which the bundled continuum correction is neutral.
pub const DEFAULT_SOLAR_RADIO_FLUX: SolarFluxUnits =
    SolarFluxUnits::new((1.0 - 2.068e-1) / 6.139e-3);
