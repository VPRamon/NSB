//! Shared helpers for `nsb-cli` integration contract suites.
//!
//! Each integration binary compiles this module independently and only uses a
//! subset of helpers, so unused-item linting is expected here.
#![allow(dead_code)]

use nsb::{
    AirglowWavelengthApplicability, ValidatedZenithDomain, VerticalEmissionProfile,
    VerticalEmissionProfileDefinition, VerticalProfileNormalization,
    VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
};
use siderust::checksum::{sha256, to_hex};
use siderust::coordinates::frames::Galactic;
use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};
use siderust::qtty::{Degrees, Kilometers, Nanometers};
use std::fs;

pub(crate) const AIRGLOW_CSV_PROVENANCE_COLUMNS: [&str; 17] = [
    "airglow_geometry_model",
    "airglow_geometry_version",
    "airglow_geometry_emission_height_km",
    "airglow_profile_id",
    "airglow_profile_schema_version",
    "airglow_profile_checksum_sha256",
    "airglow_profile_normalization",
    "airglow_profile_altitude_min_km",
    "airglow_profile_altitude_max_km",
    "airglow_profile_wavelength_min_nm",
    "airglow_profile_wavelength_max_nm",
    "airglow_profile_wavelength_band",
    "airglow_geometry_assumptions",
    "airglow_profile_validated_zenith_min_deg",
    "airglow_profile_validated_zenith_max_deg",
    "airglow_geometry_provenance",
    "airglow_profile_license",
];

pub(crate) fn write_validated_fixture_schema(
    map_path: &std::path::Path,
    manifest_path: &std::path::Path,
    with_uncertainty: bool,
) {
    let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
    let mut map = String::from(concat!(
        "# map_type=healpix\n",
        "# coordinate_frame=galactic\n",
        "# nside=8\n",
        "# ordering=ring\n",
        "# dataset_name=CLI validated fixture\n",
        "# version=fixture-v1\n",
        "# generation_date_utc=2026-06-24T00:00:00Z\n",
        "# source_catalogue=synthetic fixture catalogue\n",
        "# source_catalogue_release=fixture-release\n",
        "# source_catalogue_license=CC0-1.0\n",
        "# source_catalogue_checksum=sha256:1111111111111111111111111111111111111111111111111111111111111111\n",
        "# source_selection=complete synthetic plane-enhanced fixture\n",
        "# magnitude_limit=not applicable\n",
        "# map_resolution=HEALPix nside=8 ordering=ring\n",
        "# calibration_status=production\n",
        "# photometry_model=synthetic-passband-integrated-v1\n",
        "# band_definition=synthetic integrated 300-650 nm test band\n",
        "# smoothing=none\n",
        "# generated_by=CLI integration test\n",
        "# generation_command=synthetic fixture builder\n",
        "# validation_report=test admission report\n",
        "# independent_comparison=synthetic trusted reference fixture\n",
        "# s10_diagnostics=not_provided\n",
    ));
    map.push_str(concat!(
        "healpix_index,integrated_ph_cm2_ns_sr,",
        "statistical_uncertainty_ph_cm2_ns_sr,",
        "systematic_uncertainty_ph_cm2_ns_sr,",
        "total_uncertainty_ph_cm2_ns_sr\n",
    ));
    let mut source_flux = 0.0;
    for index in 0..grid.npix() {
        let latitude = grid
            .pixel_center_spherical::<Galactic>(HealpixIndex::new(index))
            .unwrap()
            .b()
            .abs();
        let value = if latitude <= Degrees::new(10.0) {
            2.0
        } else {
            1.0
        };
        source_flux += value * grid.pixel_area_sr();
        if with_uncertainty {
            map.push_str(&format!(
                "{index},{value},{},{},{}\n",
                value * 0.1,
                value * 0.2,
                value * 0.25,
            ));
        } else {
            // Packed schema always carries an uncertainty triplet; zeros mark
            // “no published uncertainty” only when all three are zero together.
            map.push_str(&format!("{index},{value},0.0,0.0,0.0\n"));
        }
    }
    let checksum = format!("sha256:{}", to_hex(&sha256(map.as_bytes())));
    fs::write(map_path, &map).unwrap();
    let manifest = format!(
        r#"schema_version = 1
calibration_status = "production"
dataset_name = "CLI validated fixture"
version = "fixture-v1"
generation_date = "2026-06-24T00:00:00Z"
source_catalogue = "synthetic fixture catalogue"
source_catalogue_release = "fixture-release"
source_catalogue_license = "CC0-1.0"
source_catalogue_checksum = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
source_selection = "complete synthetic plane-enhanced fixture"
magnitude_limit = "not applicable"
map_resolution = "HEALPix nside=8 ordering=ring"
photometry_model = "synthetic-passband-integrated-v1"
band_definition = "synthetic integrated 300-650 nm test band"
smoothing = "none"
generated_by = "CLI integration test"
generation_command = "synthetic fixture builder"
map_sha256 = "{checksum}"
validation_report = "test admission report"
independent_comparison = "synthetic trusted reference fixture"
flux_conservation_validated = true
input_integrated_flux_sum = {source_flux:.17}
integrated_flux_conservation_tolerance = 0.000000001

[header]
map_type = "healpix"
coordinate_frame = "galactic"
nside = "8"
ordering = "ring"
dataset_name = "CLI validated fixture"
version = "fixture-v1"
generation_date_utc = "2026-06-24T00:00:00Z"
source_catalogue = "synthetic fixture catalogue"
source_catalogue_release = "fixture-release"
source_catalogue_license = "CC0-1.0"
source_catalogue_checksum = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
source_selection = "complete synthetic plane-enhanced fixture"
magnitude_limit = "not applicable"
map_resolution = "HEALPix nside=8 ordering=ring"
calibration_status = "production"
photometry_model = "synthetic-passband-integrated-v1"
band_definition = "synthetic integrated 300-650 nm test band"
smoothing = "none"
generated_by = "CLI integration test"
generation_command = "synthetic fixture builder"
validation_report = "test admission report"
independent_comparison = "synthetic trusted reference fixture"
"#,
    );
    fs::write(manifest_path, manifest).unwrap();
}

pub(crate) fn write_validated_fixture(map_path: &std::path::Path, manifest_path: &std::path::Path) {
    write_validated_fixture_schema(map_path, manifest_path, false);
}

pub(crate) fn csv_value(
    headers: &csv::StringRecord,
    row: &csv::StringRecord,
    column: &str,
) -> String {
    let index = headers
        .iter()
        .position(|header| header == column)
        .unwrap_or_else(|| panic!("missing CSV column {column}"));
    row.get(index).unwrap().to_string()
}

pub(crate) fn synthetic_vertical_profile(
    id: &str,
    peak_emissivity: f64,
) -> VerticalEmissionProfile {
    VerticalEmissionProfile::new(VerticalEmissionProfileDefinition {
        schema_version: VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,
        profile_id: id.into(),
        altitude_km: vec![
            Kilometers::new(80.0),
            Kilometers::new(90.0),
            Kilometers::new(100.0),
        ],
        relative_emissivity: vec![0.0, peak_emissivity, 0.0],
        normalization: VerticalProfileNormalization::UnitVerticalIntegral,
        wavelength: AirglowWavelengthApplicability {
            min: Nanometers::new(300.0),
            max: Nanometers::new(650.0),
            band: "synthetic NSB optical validation band".into(),
        },
        assumptions: "Synthetic triangular layer for CLI transport validation only".into(),
        provenance: "Generated in crates/nsb-cli/tests/common/mod.rs".into(),
        license: "Synthetic test data; AGPL-3.0-only repository fixture".into(),
        validated_zenith: ValidatedZenithDomain {
            min: Degrees::new(0.0),
            max: Degrees::new(90.0),
        },
    })
    .unwrap()
}

pub(crate) fn write_validated_uncertainty_fixture(
    map_path: &std::path::Path,
    manifest_path: &std::path::Path,
) {
    write_validated_fixture_schema(map_path, manifest_path, true);
}
