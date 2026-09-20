use nsb_public_api_gate::{run_check, CheckOptions, GateStatus, HistoricalMode};
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

fn temporary_repo() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "nsb-public-api-gate-prefreeze-{}-{nonce}",
        std::process::id()
    ))
}

#[test]
fn pre_freeze_check_does_not_require_snapshot_or_semver_base() {
    let repo = temporary_repo();
    fs::create_dir_all(repo.join("crates/nsb/src")).expect("create temporary repo");
    fs::write(
        repo.join("crates/nsb/src/lib.rs"),
        "pub fn api_can_change() {}\n",
    )
    .expect("write temporary source");

    let outcome = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect("pre-freeze policy must pass without a snapshot");

    assert_eq!(outcome.status, GateStatus::Pass);
    assert!(outcome.message.contains("pre-freeze"));
    assert!(matches!(
        outcome.historical,
        Some(HistoricalMode::Bootstrap { .. })
    ));
    assert!(!repo.join("crates/nsb/api/public-api.txt").exists());

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_rejects_airglow_compatibility_debt() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/airglow");
    let nested = source.join("models");
    fs::create_dir_all(&nested).expect("create temporary source");
    fs::write(
        source.join("model.rs"),
        "#[allow(dead_code)]\nfn stale() {}\nfn with_f10_7() {}\n",
    )
    .expect("write stale compatibility debt");
    fs::write(
        nested.join("legacy.rs"),
        "enum LegacyDefault { Old }\nenum AirglowScientificProfile { Old }\n",
    )
    .expect("write nested compatibility debt");

    let error = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect_err("compatibility debt must be rejected");
    assert!(error.to_string().contains("#[allow(dead_code)]"));
    assert!(error.to_string().contains("with_f10_7"));
    assert!(error.to_string().contains("LegacyDefault"));
    assert!(error.to_string().contains("AirglowScientificProfile"));

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_allows_supported_airglow_model_names_containing_legacy() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/airglow/models");
    fs::create_dir_all(&source).expect("create temporary source");
    fs::write(
        source.join("reference.rs"),
        "pub enum ParanalLegacyReference { Reference }\n",
    )
    .expect("write supported scientific model name");

    let outcome = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect("scientific model names must not be rejected solely for containing Legacy");
    assert_eq!(outcome.status, GateStatus::Pass);

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_rejects_public_moonlight_implementation_surface() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/moonlight");
    fs::create_dir_all(&source).expect("create temporary source");
    fs::write(
        source.join("model.rs"),
        concat!(
            "pub struct Jones2013Spectral;\n",
            "pub struct KrisciunasSchaefer1991;\n",
            "pub struct MoonOutputs;\n",
            "pub const DEFAULT_K_EXT: f64 = 0.172;\n",
            "pub const DEFAULT_PERIOD_SEARCH_STEP: u64 = 600;\n",
            "pub fn with_extinction_scale() {}\n",
            "pub fn periods_in_range() {}\n",
        ),
    )
    .expect("write accidental Moonlight public surface");

    let error = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect_err("accidental Moonlight implementation API must be rejected");
    for expected in [
        "pub struct Jones2013Spectral",
        "pub struct KrisciunasSchaefer1991",
        "pub struct MoonOutputs",
        "pub const DEFAULT_K_EXT",
        "pub const DEFAULT_PERIOD_SEARCH_STEP",
        "pub fn with_extinction_scale",
        "pub fn periods_in_range",
    ] {
        assert!(
            error.to_string().contains(expected),
            "missing guard for {expected}"
        );
    }

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_allows_internal_supported_moonlight_implementations() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/moonlight");
    fs::create_dir_all(&source).expect("create temporary source");
    fs::write(
        source.join("model.rs"),
        concat!(
            "pub enum MoonlightModel { Jones2013Spectral, KrisciunasSchaefer1991 }\n",
            "pub(crate) struct Jones2013Spectral;\n",
            "pub(crate) struct KrisciunasSchaefer1991;\n",
            "pub(crate) struct MoonOutputs;\n",
            "const DEFAULT_K_EXT: f64 = 0.172;\n",
        ),
    )
    .expect("write intentional Moonlight surface");

    let outcome = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect("supported model identity and internal implementations must be allowed");
    assert_eq!(outcome.status, GateStatus::Pass);

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_rejects_public_starlight_implementation_surface() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/starlight");
    let evaluator = repo.join("crates/nsb/src/evaluator");
    fs::create_dir_all(&source).expect("create temporary Starlight source");
    fs::create_dir_all(&evaluator).expect("create temporary evaluator source");
    fs::write(
        source.join("model.rs"),
        concat!(
            "pub struct Starlight { value: f64 }\n",
            "pub struct StarlightOutputs { value: f64 }\n",
        ),
    )
    .expect("write accidental Starlight implementation surface");
    fs::write(
        source.join("mod.rs"),
        "pub use model::Starlight;\npub use output::StarlightOutputs;\n",
    )
    .expect("write accidental Starlight re-exports");
    fs::write(
        evaluator.join("types.rs"),
        concat!(
            "pub enum StarlightModel { Bundled }\n",
            "pub struct Config { pub starlight_model: Option<StarlightModel> }\n",
            "impl Config { pub fn with_starlight_model(self) -> Self { self } }\n",
        ),
    )
    .expect("write removed Starlight configuration surface in its historical location");

    let error = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect_err("accidental Starlight implementation API must be rejected");
    for expected in [
        "pub struct Starlight {",
        "pub struct StarlightOutputs {",
        "pub enum StarlightModel {",
        "pub fn with_starlight_model",
        "pub starlight_model:",
        "pub use model::Starlight;",
        "pub use output::StarlightOutputs;",
    ] {
        assert!(
            error.to_string().contains(expected),
            "missing guard for {expected}"
        );
    }

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_allows_starlight_product_and_advanced_records() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/starlight");
    fs::create_dir_all(&source).expect("create temporary source");
    fs::write(
        source.join("product.rs"),
        concat!(
            "#[non_exhaustive]\n",
            "pub enum StarlightProduct { BundledProductionGaiaDr3, ExperimentalMap }\n",
            "#[non_exhaustive]\n",
            "pub struct StarlightPixel { pub integrated: f64 }\n",
            "#[non_exhaustive]\n",
            "pub struct StarlightProvenance { pub dataset_name: String }\n",
            "pub(crate) struct Starlight;\n",
            "pub(crate) struct StarlightOutputs;\n",
        ),
    )
    .expect("write intentional Starlight surface");

    let outcome = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect("product selection and advanced records must remain allowed");
    assert_eq!(outcome.status, GateStatus::Pass);

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_rejects_public_zodiacal_implementation_surface() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/zodiacal");
    fs::create_dir_all(&source).expect("create temporary Zodiacal source");
    fs::write(
        source.join("model.rs"),
        concat!(
            "pub struct ZodiacalLight { value: f64 }\n",
            "pub struct ZodiacalBrightnessGrid { value: f64 }\n",
            "pub enum ZodiacalBrightnessModel { Leinert1998 }\n",
            "impl ZodiacalLight {\n",
            "    pub fn with_solar_spectrum(self) -> Self { self }\n",
            "    pub fn with_brightness_model() {}\n",
            "}\n",
        ),
    )
    .expect("write accidental Zodiacal implementation surface");
    fs::write(
        source.join("output.rs"),
        "pub struct ZodiacalOutputs { value: f64 }\npub struct ZodiacalSpectrum { value: f64 }\n",
    )
    .expect("write accidental Zodiacal output surface");
    fs::write(
        source.join("mod.rs"),
        "pub use model::ZodiacalLight;\npub use output::ZodiacalOutputs;\n",
    )
    .expect("write accidental Zodiacal re-exports");
    fs::write(
        repo.join("crates/nsb/src/lib.rs"),
        "pub use components::zodiacal::{ZodiacalBrightnessGrid, ZodiacalBrightnessModel, ZodiacalLight, ZodiacalOutputs, ZodiacalSpectrum};\n",
    )
    .expect("write historical Zodiacal root exports");

    let error = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect_err("accidental Zodiacal implementation API must be rejected");

    for expected in [
        "pub struct ZodiacalLight {",
        "pub struct ZodiacalBrightnessGrid {",
        "pub enum ZodiacalBrightnessModel {",
        "pub fn with_solar_spectrum(",
        "pub fn with_brightness_model(",
        "pub struct ZodiacalOutputs {",
        "pub struct ZodiacalSpectrum {",
        "ZodiacalSpectrum",
    ] {
        assert!(
            error.to_string().contains(expected),
            "missing Zodiacal guard for {expected}"
        );
    }

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_rejects_zodiacal_aliases_and_multiline_reexports() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/zodiacal");
    fs::create_dir_all(&source).expect("create temporary Zodiacal source");
    fs::write(source.join("aliases.rs"), "pub type ZodiacalOutputs = ();\n")
        .expect("write accidental Zodiacal type alias");
    fs::write(
        source.join("mod.rs"),
        concat!(
            "mod output;\n",
            "pub use output::{\n",
            "    ZodiacalSpectrum\n",
            "};\n",
        ),
    )
    .expect("write accidental multiline Zodiacal re-export");
    fs::write(source.join("output.rs"), "pub struct ZodiacalSpectrum;\n")
        .expect("write accidental Zodiacal spectrum");
    fs::write(
        repo.join("crates/nsb/src/lib.rs"),
        concat!(
            "pub use components::zodiacal::{\n",
            "    ZodiacalSpectrum\n",
            "};\n",
        ),
    )
    .expect("write accidental multiline root re-export");

    let error = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect_err("aliases and multiline re-exports of removed Zodiacal APIs must be rejected");

    assert!(error.to_string().contains("ZodiacalOutputs"));
    assert!(error.to_string().contains("ZodiacalSpectrum"));

    fs::remove_dir_all(repo).expect("remove temporary repo");
}

#[test]
fn pre_freeze_check_allows_durable_zodiacal_selection_surface() {
    let repo = temporary_repo();
    let source = repo.join("crates/nsb/src/components/zodiacal");
    fs::create_dir_all(&source).expect("create temporary Zodiacal source");
    fs::write(
        source.join("model.rs"),
        concat!(
            "#[non_exhaustive]\n",
            "pub enum ZodiacalModel { Leinert1998 }\n",
            "pub(crate) struct ZodiacalLight;\n",
            "pub(super) struct ZodiacalBrightnessGrid;\n",
            "pub(super) enum ZodiacalBrightnessModel { Leinert1998 }\n",
            "pub(crate) type ZodiacalSpectrum = ();\n",
        ),
    )
    .expect("write intentional Zodiacal model surface");
    fs::write(
        source.join("extinction.rs"),
        "#[non_exhaustive]\npub enum ZodiacalExtinction { None, Noll2012Approx }\n",
    )
    .expect("write intentional Zodiacal extinction surface");
    fs::write(
        source.join("output.rs"),
        "pub(crate) struct ZodiacalOutputs;\n",
    )
    .expect("write internal Zodiacal output");
    fs::write(
        source.join("mod.rs"),
        "pub use extinction::ZodiacalExtinction;\npub use model::ZodiacalModel;\npub(crate) use model::{ZodiacalLight, ZodiacalSpectrum};\n",
    )
    .expect("write intentional Zodiacal re-exports");
    fs::write(
        repo.join("crates/nsb/src/lib.rs"),
        "pub use components::zodiacal::{ZodiacalExtinction, ZodiacalModel};\n",
    )
    .expect("write intentional Zodiacal root exports");

    let outcome = run_check(&CheckOptions {
        repo: repo.clone(),
        write: false,
        base: None,
        base_explicit: false,
    })
    .expect("durable Zodiacal selectors and internal implementation must be allowed");
    assert_eq!(outcome.status, GateStatus::Pass);

    fs::remove_dir_all(repo).expect("remove temporary repo");
}
