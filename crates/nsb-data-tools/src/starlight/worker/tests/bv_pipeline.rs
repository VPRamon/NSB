// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use super::*;
use crate::dataset::{self, DatasetName, Executor, Operation, RunConfig, ValidationReport};
use crate::starlight::bright_stars::{test_support::*, *};
use crate::starlight::map::product::MergeReport;
use crate::starlight::pack::{verify_bv_candidate, BV_FILES};
use crate::starlight::xp::{calibrate::interval_tests::inputs, canonical};
use nsb::components::starlight::{StarlightMap, StarlightProvenance};
use nsb::{ComponentMask, NsbEvaluator, NsbModelConfig, PointQuery, StarlightProduct, Target, DEG};
use std::collections::BTreeMap;

#[test]
fn combined_sources_to_evaluator_preserve_integrated_map() -> Result<()> {
    combined_case(None)
}

#[test]
#[ignore = "requires locally verified real Gaia subsets and a fitted B/V shape artifact"]
fn real_gaia_shape_candidate_to_evaluator() -> Result<()> {
    let pilot = PathBuf::from(std::env::var_os("NSB_BV_REAL_PILOT").context("NSB_BV_REAL_PILOT")?);
    combined_case(Some(&pilot))
}

fn combined_case(pilot: Option<&Path>) -> Result<()> {
    const BAND: StarlightProductBand = StarlightProductBand::Combined300To650;
    let partition = if pilot.is_some() {
        "466830-466850"
    } else {
        "000000-003111"
    };
    let metadata: Option<serde_json::Value> = pilot
        .map(|path| -> Result<_> {
            Ok(serde_json::from_slice(&fs::read(
                path.join("sample.json"),
            )?)?)
        })
        .transpose()?;
    let replacement = metadata
        .as_ref()
        .map(|m| m["replacement"].as_u64().unwrap())
        .unwrap_or(105);
    let [ra, dec] = metadata
        .as_ref()
        .map(|m| [m["ra"].as_f64().unwrap(), m["dec"].as_f64().unwrap()])
        .unwrap_or([266.41683, -29.00781]);
    let root = tempfile::tempdir()?;
    let workspace = root.path();
    let pin = |name: &str, bytes: &[u8]| -> Result<ArtifactPinConfig> {
        let path = workspace.join(name);
        fs::write(&path, bytes)?;
        Ok(ArtifactPinConfig {
            artifact_path: path,
            sha256: checksum_io::sha256_bytes(bytes),
        })
    };
    let (calibrator, mut record) = inputs();
    record.source_id = "100".into();
    let xp = workspace.join("xp.csv");
    canonical::write_gaiaxpy_datalink_csv(&xp, &record)?;
    let xp_csv = fs::read(&xp)?;
    let products = vec![
        product("gaia-source", "GaiaSource_"),
        product("xp-continuous", "XpContinuousMeanSpectrum_"),
    ];
    let catalogue = b"source_id,ra,dec,phot_g_mean_mag,phot_bp_mean_mag,phot_rp_mean_mag,bp_rp,x,duplicated_source
100,266.41683,-29.00781,15,16,14,2,1,false
101,266.41683,-29.00781,15,16,14,2,1,false
102,318,48,15,16,,,1,false
103,318,48,15,,14,,1,false
104,86,29,15,,,,1,false
105,0,0,2,,,,1,false
106,318,48,15,16,14,2,1,true
";
    let xp_object = match pilot {
        Some(path) => fs::read(path.join("xp.csv.gz"))?,
        None => gzip_bytes(&xp_csv)?,
    };
    let gaia_object = match pilot {
        Some(path) => fs::read(path.join("gaia.csv.gz"))?,
        None => gzip_bytes(catalogue)?,
    };
    install_object_and_receipt(
        workspace,
        &products[1],
        partition,
        &xp_object,
        &format!("XpContinuousMeanSpectrum_{partition}.csv.gz"),
    )?;
    install_object_and_receipt(
        workspace,
        &products[0],
        partition,
        &gaia_object,
        &format!("GaiaSource_{partition}.csv.gz"),
    )?;

    let mut synthetic_uv: Value = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/uv_synthetic_non_production/artifact.json"
    ))?;
    // Exercise the unchanged combined-band admission gate with labelled
    // synthetic bytes confined to this temporary test workspace.
    synthetic_uv["calibration_status"] = "validated".into();
    if pilot.is_some() {
        synthetic_uv["predictors"][0]["name"] = "phot_g_mean_mag".into();
        synthetic_uv["predictors"][0]["domain"]["maximum"] = 22.0.into();
    }
    let uv_pin = pin("uv.json", &serde_json::to_vec(&synthetic_uv)?)?;
    let photometric = pin(
        "photometric.json",
        include_bytes!("../../../../tests/fixtures/photometric_xp_anchored_v1/artifact.json"),
    )?;
    let selection = pin(
        "selection.json",
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1, "model_id": "synthetic-selection", "calibration_status": "validated",
            "reference_dataset": {"name": "fixture", "release": "fixture", "licence": "AGPL-3.0-only", "doi": "fixture", "files": [{"name": "fixture", "sha256": "d".repeat(64)}]},
            "weight_cap": 5, "magnitude_bins": [10,22], "colour_bins": [-1,6],
            "healpix_nside": 1,
            "completeness_table": (0..12).map(|pixel| serde_json::json!({
                "healpix": pixel, "magnitude_bin": 0, "colour_bin": 0, "completeness": 0.5
            })).collect::<Vec<_>>(),
            "colour_marginalisation": {"policy": "marginalise-uniform"},
            "faint_tail": {"enabled": true, "magnitude_limit_g": 14, "residual_fraction_per_pixel": 0.08, "systematic_fraction": 0.15},
            "training_command": "fixture", "software_version": "fixture"
        }))?,
    )?;
    let (mut model, hip, xhip) = reconstruction_fixture();
    model.templates[0].f_lambda_si = vec![0.8, 0.8, 1.0, 1.8, 2.0];
    let spectra = reconstruct_spectral_estimates_with_bv(
        std::slice::from_ref(&hip),
        &xhip,
        &model,
        BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID,
        true,
    )?;
    let gaia = GaiaMatchRow {
        gaia_source_id: replacement,
        ra_deg_j2016: 0.0,
        dec_deg_j2016: 0.0,
        gaia_g_mag: Some(2.0),
        gaia_xp_usable: false,
        gaia_photometric_usable: false,
    };
    let (bright, _) = build_experimental_artifact_for_product_band(
        1,
        &"a".repeat(40),
        vec![],
        &[hip],
        &BTreeMap::new(),
        0,
        &xhip,
        &BTreeMap::from([(
            1,
            vec![HipGaiaIdentityMatch {
                hip: 1,
                gaia_source_id: replacement,
                angular_distance_arcsec: 0.0,
                number_of_neighbours: 1,
            }],
        )]),
        &BTreeMap::from([(replacement, gaia)]),
        &[],
        &spectra,
        &BTreeSet::new(),
        BrightStarPopulationPolicy::v1(),
        BrightStarPrecedencePolicy::v1(),
        BRIGHT_STAR_COMBINED_PRODUCT_BAND_ID,
    )?;
    let supplement = pin("bright.json", &bright.to_json_pretty()?)?;
    let bright = load_bright_star_artifact(&supplement.artifact_path, &supplement.sha256)?;
    let suppressed = bright.suppressed_gaia_source_ids()?;
    assert_eq!(suppressed, BTreeSet::from([replacement]));

    let centre = healpix::galactic_nested_pixel_from_icrs_position(ra, dec, 1)?;
    let opposite = healpix::galactic_nested_pixel_from_icrs_position(318.0, 48.0, 1)?;
    if pilot.is_none() {
        assert_ne!(centre % 2, opposite % 2, "fixture needs different colours");
    }
    let design = checksum_io::sha256_file(
        &GaiaXpContinuousCalibrator::resolve_design_fixture_path(None, None),
    )?;
    let model_bytes = match pilot {
        Some(path) => fs::read(path.join("bv-shape.json"))?,
        None => serde_json::to_vec(&crate::starlight::bv::tests::calibration_fixture(
            &[
                centre,
                opposite,
                healpix::galactic_nested_pixel_from_icrs_position(86.0, 29.0, 1)?,
            ],
            &design,
        ))?,
    };
    let inference = pin("inference.json", &model_bytes)?;
    let mut run = RunConfig::load(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("config/starlight-production-300-650.ladon.toml"),
    )?;
    run.workspace.root = workspace.to_path_buf();
    run.publish = None;
    let config = run.starlight.as_mut().unwrap();
    config.map.canonical_nside = 1;
    config.gaia_products = products.clone();
    config.ultraviolet_correction = Some(uv_pin);
    config.photometric_inference = Some(photometric);
    config.selection_function = Some(selection);
    config.bright_star_supplement = Some(supplement.clone());
    let path = workspace.join("run-config.toml");
    let execute = |operation| {
        dataset::execute(
            &path,
            DatasetName::Starlight,
            operation,
            Some(Executor::Local),
            Some(1),
            &[],
        )
    };
    let configure =
        |config: &RunConfig| -> Result<()> { Ok(fs::write(&path, toml::to_string(config)?)?) };
    configure(&run)?;
    execute(Operation::Build)?;
    let validation = || -> Result<ValidationReport> {
        Ok(serde_json::from_slice(&fs::read(
            workspace.join("validation.json"),
        )?)?)
    };
    let validate = || -> Result<()> {
        if let Err(error) = execute(Operation::Validate) {
            let report = validation()?;
            let failed: Vec<_> = report.gates.iter().filter(|g| !g.passed).collect();
            // A single real partition exercises the packaging and worker gates,
            // while the normal full-sky plane coverage guard must still fail.
            if pilot.is_none()
                || failed.len() != 1
                || failed[0].name != "pixel-coverage-galactic-plane"
            {
                anyhow::bail!("{error}: {:?}", report.gates);
            }
        }
        Ok(())
    };
    validate()?;
    let legacy = validation()?;
    let mut enabled = run.clone();
    enabled.starlight.as_mut().unwrap().bv_photometry = Some(inference.clone());
    let config = enabled.starlight.as_ref().unwrap();
    let pins = BvInputPins::new(
        &inference,
        config.photometric_inference.as_ref(),
        config.selection_function.as_ref(),
        config.bright_star_supplement.as_ref(),
        &products,
    );
    configure(&enabled)?;
    let map_path = workspace.join("outputs/starlight_nside1.csv");
    let integrated = fs::read(&map_path)?;
    assert_eq!(legacy.artifacts.len(), 2);
    assert!(
        execute(Operation::Validate).is_err(),
        "integrated-only shards cannot be reused"
    );
    execute(Operation::Build)?;
    validate()?;
    let valid = validation()?;
    assert!(valid
        .gates
        .iter()
        .any(|g| g.name == "starlight-bv-candidate-integrity" && g.passed));
    let artifacts = valid.artifacts;
    assert!(execute(Operation::Publish)
        .unwrap_err()
        .to_string()
        .contains("checksum-specific approval"));
    configure(&run)?;
    assert!(execute(Operation::Publish)
        .unwrap_err()
        .to_string()
        .contains("ineligible"));
    configure(&enabled)?;
    assert_eq!(integrated, fs::read(&map_path)?);
    assert_eq!(artifacts.len(), 5);
    let compact = artifacts.iter().find(|a| a.name == BV_FILES[0]).unwrap();
    let manifest = artifacts.iter().find(|a| a.name == BV_FILES[2]).unwrap();
    let digest = &manifest.sha256;
    let (candidate, evidence) = verify_bv_candidate(workspace, digest)?;
    let admitted = candidate.admitted_sources().values().sum::<u64>();
    let observable = |key: &str| {
        metadata
            .as_ref()
            .map(|m| m["counts"][key].as_u64().unwrap())
            .unwrap_or(1)
    };
    assert_eq!(
        admitted,
        observable("xp")
            + observable("full")
            + observable("g_and_bp")
            + observable("g_and_rp")
            + observable("g_only")
            + 1
    );
    let mut counts = BTreeMap::new();
    for pixel in evidence.pixels.values() {
        for (route, n) in &pixel.source_counts {
            *counts.entry(*route).or_insert(0) += n;
        }
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            (BvRoute::GaiaXp, observable("xp")),
            (BvRoute::FullColour, observable("full")),
            (BvRoute::GAndBp, observable("g_and_bp")),
            (BvRoute::GAndRp, observable("g_and_rp")),
            (BvRoute::GOnly, observable("g_only")),
            (BvRoute::HipparcosCk04, 1),
        ])
    );
    let report: MergeReport =
        serde_json::from_slice(&fs::read(workspace.join("outputs/merge_report.json"))?)?;
    assert_eq!(report.excluded_sources, if pilot.is_some() { 1 } else { 2 });
    assert_eq!(report.source_record_accounting.replacement_records, 1);
    let actual_centre = evidence.pixels[&centre].flux_w_m2_nm;
    if pilot.is_some() {
        assert_eq!(
            evidence
                .pixels
                .values()
                .flat_map(|pixel| pixel.inference_support.values())
                .sum::<u64>(),
            admitted - observable("xp") - 1
        );
    } else {
        let midway = calibrator.bv_sampler()?.sample(&record)?.flux_w_m2_nm;
        let colour = |pixel| {
            if pixel % 2 == 0 {
                [1e-14, 2e-14]
            } else {
                [4e-14, 1e-14]
            }
        };
        let actual_opposite = evidence.pixels[&opposite].flux_w_m2_nm;
        for band in 0..2 {
            let expected_centre = 2.0 * midway[band] + 2.0 * colour(centre)[band];
            assert!((actual_centre[band] / expected_centre - 1.0).abs() < 1e-14);
            assert!((actual_opposite[band] / (4.0 * colour(opposite)[band]) - 1.0).abs() < 1e-14);
        }
        assert_ne!(
            actual_centre[0] / actual_centre[1],
            actual_opposite[0] / actual_opposite[1]
        );
    }

    let bytes = fs::read(&compact.path)?;
    let map = StarlightMap::from_compact_bytes(
        &bytes,
        StarlightProvenance::new(
            "synthetic test",
            "2",
            "fixture",
            "fixture",
            "fixture",
            "fixture",
            "fixture",
            "fixture",
            Some(compact.sha256.clone()),
        ),
    )?;
    let query = PointQuery::new(
        siderust::catalogs::observatories::EL_PARANAL.geodetic(),
        tempoch::Time::<tempoch::UTC>::from_chrono(
            chrono::DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")?
                .with_timezone(&chrono::Utc),
        ),
        Target::new(ra * DEG, dec * DEG),
    )
    .with_components(ComponentMask::STARLIGHT);
    let result = NsbEvaluator::with_config(
        NsbModelConfig::generic_clear_sky()
            .with_starlight_product(StarlightProduct::with_experimental_map(map)),
    )?
    .evaluate(&query)?;
    let star = &result.components[0];
    assert!(star.bv_diagnostics_available);
    for (value, flux) in [star.b_flux_s10.value(), star.v_flux_s10.value()]
        .into_iter()
        .zip(actual_centre)
    {
        assert!((value * std::f64::consts::PI / 3.0 * 1.28e-11 / flux - 1.0).abs() < 1e-14);
    }
    assert!(result.b_mag.value().is_finite() && result.v_mag.value().is_finite());

    crate::starlight::validation::bv::tests::check_comparison(workspace, digest, &evidence)?;
    let regenerated = crate::starlight::map::product::emit_maps(
        workspace,
        &[partition.into(), "bright-star-supplement".into()],
        1,
        BAND,
        config
            .ultraviolet_correction
            .as_ref()
            .map(|p| p.sha256.as_str()),
        Some(crate::starlight::map::product::SelectionPopulationPolicy {
            model_id: "synthetic-selection".into(),
            weight_cap: 5.0,
            residual_faint_tail_estimated: true,
        }),
        Some(&pins),
    )?;
    assert_eq!(
        regenerated
            .iter()
            .find(|a| a.name == BV_FILES[0])
            .unwrap()
            .sha256,
        compact.sha256
    );
    assert_eq!(fs::read(&compact.path)?, bytes);
    if let Some(path) = pilot {
        let result = path.join("candidate");
        fs::create_dir_all(&result)?;
        for name in BV_FILES {
            let file = workspace.join(name);
            fs::copy(&file, result.join(file.file_name().unwrap()))?;
        }
        fs::copy(
            workspace.join("validation.json"),
            result.join("validation.json"),
        )?;
        fs::copy(&map_path, result.join("integrated.csv"))?;
        fs::copy(
            workspace.join("outputs/merge_report.json"),
            result.join("merge_report.json"),
        )?;
    }
    assert!(verify_bv_candidate(workspace, &"0".repeat(64)).is_err());
    let mut manifest: serde_json::Value = serde_json::from_slice(&fs::read(&manifest.path)?)?;
    manifest["calibration_status"] = "validated".into();
    let tampered = serde_json::to_vec(&manifest)?;
    fs::write(workspace.join(BV_FILES[2]), &tampered)?;
    assert!(verify_bv_candidate(workspace, &checksum_io::sha256_bytes(&tampered)).is_err());

    let mut no_bv = bright.clone();
    no_bv.sources[0].bv = None;
    assert!(
        bright_star_supplement_shard(&no_bv, &supplement.sha256, 1, BAND, Some(&pins)).is_err()
    );
    let mut duplicate = xp_csv.clone();
    duplicate.extend_from_slice(&xp_csv[xp_csv.iter().position(|x| *x == b'\n').unwrap() + 1..]);
    install_object_and_receipt(
        workspace,
        &products[1],
        partition,
        &gzip_bytes(&duplicate)?,
        &format!("XpContinuousMeanSpectrum_{partition}.csv.gz"),
    )?;
    assert!(build_partitions(
        workspace,
        &products,
        &[partition.into()],
        1,
        1,
        BAND,
        config.ultraviolet_correction.as_ref(),
        config.photometric_inference.as_ref(),
        config.selection_function.as_ref(),
        config.bright_star_supplement.as_ref(),
        Some(&inference),
    )
    .unwrap_err()
    .to_string()
    .contains("duplicate XP source_id"));
    Ok(())
}
