//! Airglow continuum spectrum.
//!
//! The reference file `data/airglow_cont.dat` is a multi-block format with
//! per-season and per-time-of-night scaling factors. The loader below extracts
//! only the global scale and the per-wavelength relative mean profile, which
//! is enough for a first-pass airglow component. Per-season/time corrections
//! are applied later in `components::airglow`.

use crate::error::{NsbError, Result};
use siderust::qtty::{length::Meter, Nanometer};
use siderust::spectra::{Interpolation, OutOfRange, Provenance, SampledSpectrum};

const RAW: &str = include_str!("../../data/airglow_cont.dat");

// Pinned SHA-256 of the airglow continuum reference file.
siderust::assert_data_checksum!(
    "NSB/data/airglow_cont.dat",
    RAW.as_bytes(),
    "d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f"
);

#[derive(Debug, Clone)]
pub struct AirglowContinuum {
    /// Global scale factor (`scale` block in the file).
    pub global_scale: f64,
    /// Wavelength [nm] vs relative mean radiance.
    pub spectrum: SampledSpectrum<Nanometer, Meter, f64>,
    /// Number of seasons / time windows in the file.
    pub n_season: usize,
    pub n_time: usize,
}

/// Parse the airglow continuum reference file.
///
/// File format (excerpted from the file header):
/// 1. `nseason ntime` — counts of season/time bins.
/// 2. `ndat` — number of wavelength samples that follow.
/// 3. `height` — emission height [km].
/// 4. `scale` — global scale factor.
/// 5. `ndat` rows of `wavelength_um  relative_mean  ...corrections...`.
///
/// The current implementation parses the wavelengths and relative mean
/// values; per-bin corrections are read on demand by `components::airglow`.
pub fn load() -> Result<AirglowContinuum> {
    let mut iter = RAW.lines().filter_map(|l| {
        let t = l.trim();
        if t.is_empty() || t.starts_with('#') {
            None
        } else {
            Some(t)
        }
    });

    let header = iter.next().ok_or_else(|| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: "missing nseason/ntime".into(),
    })?;
    let mut hp = header.split_whitespace();
    let n_season: usize =
        hp.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "nseason".into(),
            })?;
    let n_time: usize =
        hp.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "ntime".into(),
            })?;

    let n_dat: usize =
        iter.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "ndat".into(),
            })?;
    let _height: f64 =
        iter.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "height".into(),
            })?;
    let global_scale: f64 =
        iter.next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "scale".into(),
            })?;

    let mut lam = Vec::with_capacity(n_dat);
    let mut rel = Vec::with_capacity(n_dat);
    for _ in 0..n_dat {
        let row = iter.next().ok_or_else(|| NsbError::DataParse {
            file: "airglow_cont.dat",
            message: "premature EOF in data block".into(),
        })?;
        let mut p = row.split_whitespace();
        let l_um: f64 =
            p.next()
                .and_then(|x| x.parse().ok())
                .ok_or_else(|| NsbError::DataParse {
                    file: "airglow_cont.dat",
                    message: "lambda".into(),
                })?;
        let r: f64 = p
            .next()
            .and_then(|x| x.parse().ok())
            .ok_or_else(|| NsbError::DataParse {
                file: "airglow_cont.dat",
                message: "rel_mean".into(),
            })?;
        lam.push(l_um * 1000.0);
        rel.push(r);
    }
    let spectrum = SampledSpectrum::<Nanometer, Meter, f64>::from_raw(
        lam,
        rel,
        Interpolation::Linear,
        OutOfRange::ClampToEndpoints,
        Some(Provenance::bundled_file("NSB/data/airglow_cont.dat")),
    )
    .map_err(|e| NsbError::DataParse {
        file: "airglow_cont.dat",
        message: e.to_string(),
    })?;
    Ok(AirglowContinuum {
        global_scale,
        spectrum,
        n_season,
        n_time,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pinned_sha256_matches_runtime_hash() {
        use siderust::provenance::checksum::{sha256, to_hex};
        assert_eq!(
            to_hex(&sha256(RAW.as_bytes())),
            "d684fcd5d4589a0e79c9c6adc8be001fbc8fbaa599b4f6ef6a32a4740329905f",
        );
    }
}
