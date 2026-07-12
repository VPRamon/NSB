//! GaiaXPy 2.1.4 parity gate for in-process XP continuous calibration.

use nsb_data_tools::gaia_xp_continuous_calibrate::GaiaXpContinuousCalibrator;
use nsb_data_tools::gaia_xp_continuous_canonical::CanonicalXpContinuousRecord;
use serde::Deserialize;
use std::path::PathBuf;

const FLUX_RTOL: f64 = 1e-8;
const UNCERTAINTY_RTOL: f64 = 1e-6;

#[derive(Debug, Deserialize)]
struct OracleFixture {
    schema_version: u32,
    records: Vec<OracleRecord>,
}

#[derive(Debug, Deserialize)]
struct OracleRecord {
    canonical: CanonicalXpContinuousRecord,
    oracle: OracleOutcome,
}

#[derive(Debug, Deserialize)]
struct OracleOutcome {
    flux_336_650_ph_m2_s: f64,
    statistical_uncertainty_336_650_ph_m2_s: f64,
}

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/gaiaxpy_oracle")
}

#[test]
fn gaia_xp_continuous_calibrate_parity() {
    let oracle_path = fixture_root().join("continuous_parity_v1.json");
    if !oracle_path.is_file() {
        eprintln!(
            "skip gaia_xp_continuous_calibrate_parity: missing {}",
            oracle_path.display()
        );
        return;
    }
    let design_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json");
    let calibrator =
        GaiaXpContinuousCalibrator::from_design_fixture(&design_path).expect("design fixture");
    let fixture: OracleFixture =
        serde_json::from_str(&std::fs::read_to_string(&oracle_path).expect("oracle json"))
            .expect("oracle schema");
    assert_eq!(fixture.schema_version, 1);
    assert!(!fixture.records.is_empty(), "oracle must contain records");

    for entry in &fixture.records {
        let got = calibrator
            .calibrate_record(&entry.canonical)
            .expect("rust calibrate");
        let flux_ref = entry.oracle.flux_336_650_ph_m2_s;
        let unc_ref = entry.oracle.statistical_uncertainty_336_650_ph_m2_s;
        let flux_rel = relative_error(got.flux_336_650_ph_m2_s, flux_ref);
        let unc_rel = relative_error(got.statistical_uncertainty_336_650_ph_m2_s, unc_ref);
        assert!(
            flux_rel <= FLUX_RTOL,
            "source {} flux rel err {flux_rel} > {FLUX_RTOL} (got={}, ref={})",
            entry.canonical.source_id,
            got.flux_336_650_ph_m2_s,
            flux_ref
        );
        assert!(
            unc_rel <= UNCERTAINTY_RTOL,
            "source {} uncertainty rel err {unc_rel} > {UNCERTAINTY_RTOL} (got={}, ref={})",
            entry.canonical.source_id,
            got.statistical_uncertainty_336_650_ph_m2_s,
            unc_ref
        );
    }
}

fn relative_error(got: f64, reference: f64) -> f64 {
    if reference == 0.0 {
        return got.abs();
    }
    ((got - reference) / reference).abs()
}
