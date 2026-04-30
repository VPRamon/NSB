//! Catalogue of night-sky airglow spectral emission lines.
//!
//! This module provides a curated catalogue of major airglow emission lines
//! observable in the Earth's atmosphere, useful for NSB modeling and spectral
//! contamination assessment.
//!
//! The catalogue includes:
//! - **OH Meinel bands** (900–2500 nm, infrared; strongest source of nighttime airglow)
//! - **Oxygen emission lines** (O I 557.7 nm green line, O I 630/636 nm red lines, O₂ A-band)
//! - **Sodium D lines** (589.0, 589.6 nm; mesospheric resonance scattering)
//! - **Nitrogen emissions** (N₂ Lyman-Birge-Hopfield bands in UV)
//! - **Miscellaneous** (Ca+ H/K lines, Hα, etc.)
//!
//! References:
//! - Leinert et al. (1998) "The 1997 reference of diffuse night sky brightness"
//! - ESA "The Night Sky"
//! - Chamberlain & Hunten (1987) "Theory of Planetary Atmospheres" (O I, OH)
//! - Meinel catalogs and Meinel bands spectroscopy
//!
//! Scientific role:
//! this file is a descriptive catalogue of notable airglow emission features.
//! It is not the main airglow evaluator used by `NsbEvaluator`; instead it
//! documents the spectral lines and bands that make airglow scientifically
//! important.
//!
//! Contribution to the science:
//! the catalogue helps connect the crate's simplified airglow treatment to the
//! real spectral phenomena behind it, and it provides a useful reference for
//! users who want to understand which atmospheric emissions contribute to
//! optical and near-infrared sky brightness.

/// A single airglow emission line with wavelength, intensity, and description.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AirglowLine {
    /// Common name or identification of the line/band.
    pub name: &'static str,
    /// Wavelength in nanometers (vacuum wavelength).
    pub wavelength: f64,
    /// Relative photon flux or intensity (arbitrary units; use for ranking).
    /// For reference: stronger lines (e.g., OH Meinel, O I 557.7 nm) have higher values.
    pub intensity: f64,
    /// Brief description and source/context.
    pub description: &'static str,
}

/// Catalogue of major night-sky airglow emission lines.
/// 
/// This list spans from UV (N₂ LBH ~160 nm) to infrared (OH Meinel ~2500 nm).
/// Intensities are approximate relative values for comparison; absolute values depend
/// on altitude, latitude, and activity level.
pub const ALL_LINES: &[AirglowLine] = &[
    // ============= OH Meinel Bands (900–2500 nm) =============
    AirglowLine {
        name: "OH Meinel Δv = 2 (9-7 band)",
        wavelength: 1509.0,
        intensity: 850.0,
        description: "Infrared OH radical emission; one of the strongest Meinel bands.",
    },
    AirglowLine {
        name: "OH Meinel Δv = 3 (6-3 band)",
        wavelength: 1407.0,
        intensity: 920.0,
        description: "Infrared OH radical emission; prominent nighttime airglow source.",
    },
    AirglowLine {
        name: "OH Meinel Δv = 4 (4-0 band)",
        wavelength: 1195.0,
        intensity: 780.0,
        description: "Infrared OH radical emission; secondary strong feature.",
    },
    AirglowLine {
        name: "OH Meinel Δv = 5 (3-0 band)",
        wavelength: 1060.0,
        intensity: 650.0,
        description: "Infrared OH radical emission; important for near-IR observations.",
    },
    AirglowLine {
        name: "OH Meinel Δv = 6 (2-0 band)",
        wavelength: 960.0,
        intensity: 520.0,
        description: "Infrared OH radical emission; extends into near-infrared.",
    },
    AirglowLine {
        name: "OH Meinel Δv = 7 (2-0 band, longer wavelength)",
        wavelength: 2300.0,
        intensity: 400.0,
        description: "Far-infrared OH radical emission; primarily mid-infrared domain.",
    },
    AirglowLine {
        name: "OH Meinel Δv = 8 (1-0 band)",
        wavelength: 876.0,
        intensity: 450.0,
        description: "Infrared OH radical emission; extends slightly into visible near-IR.",
    },
    // ============= Green Line & Oxygen Emissions =============
    AirglowLine {
        name: "O I 557.7 nm (Green line)",
        wavelength: 557.7,
        intensity: 280.0,
        description: "Atomic oxygen forbidden emission (¹S → ³P); prominent green airglow, altitude ~97 km.",
    },
    AirglowLine {
        name: "O I 630.0 nm (Red line, 1st)",
        wavelength: 630.0,
        intensity: 120.0,
        description: "Atomic oxygen forbidden emission; red airglow, altitude ~250–350 km (thermosphere).",
    },
    AirglowLine {
        name: "O I 636.4 nm (Red line, 2nd)",
        wavelength: 636.4,
        intensity: 110.0,
        description: "Atomic oxygen forbidden emission; weak red line, thermosphere airglow.",
    },
    AirglowLine {
        name: "O₂ A-band (0-0, head) (762 nm)",
        wavelength: 762.0,
        intensity: 200.0,
        description: "Molecular oxygen band emission; significant in red/infrared airglow.",
    },
    // ============= Sodium D Lines =============
    AirglowLine {
        name: "Na D₁ (589.0 nm)",
        wavelength: 589.0,
        intensity: 85.0,
        description: "Sodium resonance line; mesospheric resonance scattering, altitude ~90 km.",
    },
    AirglowLine {
        name: "Na D₂ (589.6 nm)",
        wavelength: 589.6,
        intensity: 170.0,
        description: "Sodium resonance line (brighter doublet); mesospheric resonance scattering.",
    },
    // ============= Nitrogen Emissions (UV) =============
    AirglowLine {
        name: "N₂ Lyman-Birge-Hopfield (0-0 band, ~159 nm)",
        wavelength: 159.0,
        intensity: 95.0,
        description: "Molecular nitrogen UV emission; far-ultraviolet airglow, upper atmosphere.",
    },
    AirglowLine {
        name: "N₂ Lyman-Birge-Hopfield (1-1 band, ~170 nm)",
        wavelength: 170.0,
        intensity: 75.0,
        description: "Molecular nitrogen UV emission; secondary LBH band.",
    },
    // ============= Miscellaneous Emissions =============
    AirglowLine {
        name: "Ca+ H line (397.0 nm)",
        wavelength: 397.0,
        intensity: 40.0,
        description: "Singly-ionized calcium; weak but detectable in high-sensitivity observations.",
    },
    AirglowLine {
        name: "Ca+ K line (393.3 nm)",
        wavelength: 393.3,
        intensity: 38.0,
        description: "Singly-ionized calcium K doublet; resonance scattering.",
    },
    AirglowLine {
        name: "Hα (656.3 nm)",
        wavelength: 656.3,
        intensity: 65.0,
        description: "Hydrogen Balmer-α; weak airglow, recombination from ionospheric H.",
    },
    AirglowLine {
        name: "Hβ (486.1 nm)",
        wavelength: 486.1,
        intensity: 32.0,
        description: "Hydrogen Balmer-β; weak hydrogen line airglow.",
    },
    AirglowLine {
        name: "NO γ-band (~200 nm)",
        wavelength: 200.0,
        intensity: 50.0,
        description: "Nitric oxide UV emission; mesospheric airglow.",
    },
];

/// Total number of airglow lines in the catalogue.
pub const NUM_LINES: usize = ALL_LINES.len();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_airglow_catalogue_completeness() {
        // Ensure catalogue has sufficient lines for modeling
        assert!(
            NUM_LINES >= 15,
            "Catalogue must contain at least 15 airglow lines; found {}",
            NUM_LINES
        );
    }

    #[test]
    fn test_airglow_wavelength_validity() {
        // All wavelengths should be in the UV–infrared range [150, 3000] nm
        for line in ALL_LINES {
            assert!(
                line.wavelength >= 150.0 && line.wavelength <= 3000.0,
                "Line '{}' has invalid wavelength {} nm; must be in [150, 3000] nm",
                line.name,
                line.wavelength
            );
        }
    }

    #[test]
    fn test_airglow_intensity_positivity() {
        // All intensities must be positive
        for line in ALL_LINES {
            assert!(
                line.intensity > 0.0,
                "Line '{}' has non-positive intensity {}; must be > 0",
                line.name,
                line.intensity
            );
        }
    }

    #[test]
    fn test_airglow_names_nonempty() {
        // All names must be non-empty
        for line in ALL_LINES {
            assert!(
                !line.name.is_empty(),
                "Airglow line has empty name; all names must be non-empty"
            );
        }
    }

    #[test]
    fn test_airglow_descriptions_nonempty() {
        // All descriptions should be non-empty
        for line in ALL_LINES {
            assert!(
                !line.description.is_empty(),
                "Line '{}' has empty description; all descriptions must be non-empty",
                line.name
            );
        }
    }

    #[test]
    fn test_airglow_line_integrity() {
        // Verify exact count and spot-check a few critical lines
        assert_eq!(
            NUM_LINES, 20,
            "Expected 20 airglow lines; found {}",
            NUM_LINES
        );

        // Check that OH Meinel lines are present (the strongest)
        let has_oh_meinel = ALL_LINES.iter().any(|l| l.name.contains("OH Meinel"));
        assert!(has_oh_meinel, "Missing OH Meinel bands in catalogue");

        // Check that O I 557.7 nm is present
        let has_green_line = ALL_LINES
            .iter()
            .any(|l| (l.wavelength - 557.7).abs() < 0.1);
        assert!(has_green_line, "Missing O I 557.7 nm green line in catalogue");

        // Check that Na D lines are present
        let has_na_d = ALL_LINES
            .iter()
            .any(|l| l.name.contains("Na D") && (l.wavelength - 589.0).abs() < 1.0);
        assert!(has_na_d, "Missing Na D lines in catalogue");
    }

    #[test]
    fn test_airglow_wavelength_ordering() {
        // Spot-check that known wavelengths match expectations
        let green_line = ALL_LINES
            .iter()
            .find(|l| l.name.contains("557.7"))
            .expect("Green line not found");
        assert_eq!(green_line.wavelength, 557.7);

        let na_d1 = ALL_LINES
            .iter()
            .find(|l| l.name.contains("D₁"))
            .expect("Na D1 not found");
        assert_eq!(na_d1.wavelength, 589.0);

        let na_d2 = ALL_LINES
            .iter()
            .find(|l| l.name.contains("D₂"))
            .expect("Na D2 not found");
        assert_eq!(na_d2.wavelength, 589.6);
    }
}
