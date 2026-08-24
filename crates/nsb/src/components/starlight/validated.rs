use super::map::parse_header_metadata;
use super::{StarlightMap, StarlightProvenance};
use crate::error::{NsbError, Result};
use chrono::DateTime;
use serde::Deserialize;
use siderust::checksum::{sha256, to_hex};
use std::collections::BTreeMap;
use std::path::Path;

const MANIFEST_SCHEMA_VERSION: u32 = 1;
const GAIA_DR3_SOURCE_MANIFEST_SHA256: &str =
    "9ec782f9c83b29885924c7d47bba18d70c86b8cbefbc408b19090b6a76e8e369";
const GAIA_DR3_XP_CONTINUOUS_MANIFEST_SHA256: &str =
    "f23df1ffb45b19fc3f34d6f37791179cef1ebec6c5b9fd613a488b3be580fccd";

#[derive(Debug, Clone, PartialEq)]
/// Diagnostics proven before an external starlight map can enter production mode.
pub struct StarlightValidationDiagnostics {
    /// Number of complete HEALPix pixels.
    pub pixel_count: usize,
    /// Radiance field used by production validation.
    pub radiance_field: &'static str,
    /// Mean integrated-radiance brightness ratio between the Galactic plane and poles.
    pub plane_pole_ratio: f64,
    /// Relative integrated-radiance jump across the Galactic longitude seam.
    pub longitude_wrap_relative_jump: f64,
    /// Whether flux conservation was validated from recorded source totals.
    pub flux_conservation_recomputed: bool,
}

#[derive(Debug, Clone)]
/// A production external map admitted only after manifest and science validation.
pub struct ValidatedStarlightMap {
    map: StarlightMap,
    diagnostics: StarlightValidationDiagnostics,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExternalManifest {
    schema_version: u32,
    calibration_status: String,
    dataset_name: String,
    version: String,
    generation_date: String,
    source_catalogue: String,
    source_catalogue_release: String,
    source_catalogue_license: String,
    source_catalogue_checksum: String,
    source_selection: String,
    magnitude_limit: String,
    map_resolution: String,
    photometry_model: String,
    band_definition: String,
    smoothing: String,
    generated_by: String,
    generation_command: String,
    map_sha256: String,
    validation_report: String,
    independent_comparison: String,
    flux_conservation_validated: bool,
    input_integrated_flux_sum: Option<f64>,
    integrated_flux_conservation_tolerance: Option<f64>,
    input_b_flux_sum: Option<f64>,
    input_v_flux_sum: Option<f64>,
    flux_conservation_tolerance: Option<f64>,
    header: BTreeMap<String, String>,
    #[serde(default)]
    source_candidate: Option<SourceCandidateSection>,
    #[serde(default)]
    upstream_inputs: Vec<UpstreamInput>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceCandidateSection {
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpstreamInput {
    id: String,
    release: String,
    checksum_manifest_sha256: String,
}

impl ValidatedStarlightMap {
    /// Load and validate a production map and its TOML provenance sidecar.
    pub fn from_files(map_path: impl AsRef<Path>, manifest_path: impl AsRef<Path>) -> Result<Self> {
        let map_bytes = std::fs::read(map_path.as_ref())?;
        let manifest_raw = std::fs::read_to_string(manifest_path.as_ref())?;
        Self::from_bytes_and_manifest(&map_bytes, &manifest_raw)
    }

    /// Validate map bytes against a TOML provenance sidecar.
    pub fn from_bytes_and_manifest(map_bytes: &[u8], manifest_raw: &str) -> Result<Self> {
        let manifest: ExternalManifest = toml::from_str(manifest_raw)
            .map_err(|err| invalid(format!("invalid external starlight manifest: {err}")))?;
        manifest.validate_contract()?;

        let actual_checksum = format!("sha256:{}", to_hex(&sha256(map_bytes)));
        if !normalize_checksum(&manifest.map_sha256)
            .eq_ignore_ascii_case(normalize_checksum(&actual_checksum))
        {
            return Err(invalid(format!(
                "external starlight map checksum mismatch: expected {}, actual {actual_checksum}",
                manifest.map_sha256
            )));
        }

        let raw = std::str::from_utf8(map_bytes)
            .map_err(|err| invalid(format!("starlight map is not UTF-8: {err}")))?;
        manifest.validate_headers(&parse_header_metadata(raw))?;
        let provenance = manifest.provenance(actual_checksum);
        let map = StarlightMap::from_csv_str(raw, provenance)?;
        let diagnostics = map.validate_production_diagnostics(
            manifest.input_integrated_flux_sum,
            manifest.integrated_flux_conservation_tolerance,
            manifest.input_b_flux_sum,
            manifest.input_v_flux_sum,
            manifest.flux_conservation_tolerance,
        )?;
        Ok(Self { map, diagnostics })
    }

    /// Return the validated immutable map.
    pub fn map(&self) -> &StarlightMap {
        &self.map
    }

    /// Return scientific diagnostics recorded at admission time.
    pub fn diagnostics(&self) -> &StarlightValidationDiagnostics {
        &self.diagnostics
    }
}

impl ExternalManifest {
    fn validate_contract(&self) -> Result<()> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(invalid(format!(
                "unsupported external starlight manifest schema {}; expected {MANIFEST_SCHEMA_VERSION}",
                self.schema_version
            )));
        }
        for (name, value) in [
            ("dataset_name", &self.dataset_name),
            ("version", &self.version),
            ("generation_date", &self.generation_date),
            ("source_catalogue", &self.source_catalogue),
            ("source_catalogue_release", &self.source_catalogue_release),
            ("source_catalogue_license", &self.source_catalogue_license),
            ("source_catalogue_checksum", &self.source_catalogue_checksum),
            ("source_selection", &self.source_selection),
            ("magnitude_limit", &self.magnitude_limit),
            ("map_resolution", &self.map_resolution),
            ("photometry_model", &self.photometry_model),
            ("band_definition", &self.band_definition),
            ("smoothing", &self.smoothing),
            ("generated_by", &self.generated_by),
            ("generation_command", &self.generation_command),
            ("map_sha256", &self.map_sha256),
            ("validation_report", &self.validation_report),
            ("independent_comparison", &self.independent_comparison),
        ] {
            if value.trim().is_empty() {
                return Err(invalid(format!(
                    "external starlight manifest field {name} must not be empty"
                )));
            }
        }
        if !self.calibration_status.eq_ignore_ascii_case("production") {
            return Err(invalid(
                "validated external starlight requires calibration_status=production",
            ));
        }
        DateTime::parse_from_rfc3339(&self.generation_date).map_err(|err| {
            invalid(format!(
                "external starlight generation_date must be RFC3339: {err}"
            ))
        })?;
        validate_sha256("source_catalogue_checksum", &self.source_catalogue_checksum)?;
        validate_sha256("map_sha256", &self.map_sha256)?;
        let license = self.source_catalogue_license.to_ascii_lowercase();
        if ["unknown", "not recorded", "review required", "unreviewed"]
            .iter()
            .any(|blocked| license.contains(blocked))
        {
            return Err(invalid(
                "validated external starlight requires a reviewed source catalogue license",
            ));
        }
        reject_placeholder("source_catalogue_release", &self.source_catalogue_release)?;
        reject_placeholder("validation_report", &self.validation_report)?;
        reject_placeholder("independent_comparison", &self.independent_comparison)?;
        let photometry = self.photometry_model.to_ascii_lowercase();
        if photometry.contains("proxy") || photometry.contains("experimental") {
            return Err(invalid(
                "proxy or experimental photometry cannot enter validated production starlight mode",
            ));
        }
        if !self.flux_conservation_validated {
            return Err(invalid(
                "validated external starlight requires flux_conservation_validated=true",
            ));
        }
        let supplied_integrated_flux_fields = [
            self.input_integrated_flux_sum.is_some(),
            self.integrated_flux_conservation_tolerance.is_some(),
        ];
        if supplied_integrated_flux_fields.iter().any(|value| *value)
            && !supplied_integrated_flux_fields.iter().all(|value| *value)
        {
            return Err(invalid(
                "input_integrated_flux_sum and integrated_flux_conservation_tolerance must be supplied together",
            ));
        }
        if self.input_integrated_flux_sum.is_none() {
            return Err(invalid(
                "validated external starlight requires integrated flux-conservation inputs",
            ));
        }
        self.validate_distinct_provenance()?;
        let supplied_legacy_flux_fields = [
            self.input_b_flux_sum.is_some(),
            self.input_v_flux_sum.is_some(),
            self.flux_conservation_tolerance.is_some(),
        ];
        if supplied_legacy_flux_fields.iter().any(|value| *value)
            && !supplied_legacy_flux_fields.iter().all(|value| *value)
        {
            return Err(invalid(
                "input_b_flux_sum, input_v_flux_sum, and flux_conservation_tolerance must be supplied together",
            ));
        }
        Ok(())
    }

    fn validate_distinct_provenance(&self) -> Result<()> {
        let Some(source_candidate) = &self.source_candidate else {
            return Ok(());
        };
        validate_sha256("source_candidate.sha256", &source_candidate.sha256)?;
        let candidate_digest = normalize_checksum(&source_candidate.sha256);
        if normalize_checksum(&self.source_catalogue_checksum)
            .eq_ignore_ascii_case(candidate_digest)
        {
            return Err(invalid(
                "source_catalogue_checksum must identify an upstream catalogue manifest, not the derived candidate SHA-256",
            ));
        }
        let gaia_source = self
            .upstream_inputs
            .iter()
            .find(|input| input.id == "gaia-source")
            .ok_or_else(|| {
                invalid("source_candidate maps require upstream_inputs id=gaia-source")
            })?;
        let xp = self
            .upstream_inputs
            .iter()
            .find(|input| input.id == "xp-continuous")
            .ok_or_else(|| {
                invalid("source_candidate maps require upstream_inputs id=xp-continuous")
            })?;
        if normalize_checksum(&gaia_source.checksum_manifest_sha256)
            != GAIA_DR3_SOURCE_MANIFEST_SHA256
        {
            return Err(invalid(
                "gaia-source checksum_manifest_sha256 does not match the pinned Gaia DR3 GaiaSource acquisition manifest",
            ));
        }
        if normalize_checksum(&xp.checksum_manifest_sha256)
            != GAIA_DR3_XP_CONTINUOUS_MANIFEST_SHA256
        {
            return Err(invalid(
                "xp-continuous checksum_manifest_sha256 does not match the pinned Gaia DR3 XP continuous acquisition manifest",
            ));
        }
        if gaia_source.release != "Gaia DR3" || xp.release != "Gaia DR3" {
            return Err(invalid(
                "Gaia upstream_inputs must declare release=\"Gaia DR3\"",
            ));
        }
        if self
            .header
            .get("source_candidate_sha256")
            .map(|value| normalize_checksum(value))
            != Some(candidate_digest)
        {
            return Err(invalid(
                "header source_candidate_sha256 must match [source_candidate].sha256",
            ));
        }
        Ok(())
    }

    fn validate_headers(&self, actual: &BTreeMap<String, String>) -> Result<()> {
        for required in [
            "map_type",
            "coordinate_frame",
            "nside",
            "ordering",
            "dataset_name",
            "version",
            "generation_date_utc",
            "source_catalogue",
            "source_catalogue_release",
            "source_catalogue_license",
            "source_catalogue_checksum",
            "source_selection",
            "magnitude_limit",
            "map_resolution",
            "calibration_status",
            "photometry_model",
            "band_definition",
            "smoothing",
            "generated_by",
            "generation_command",
            "validation_report",
            "independent_comparison",
        ] {
            if !self.header.contains_key(required) {
                return Err(invalid(format!(
                    "external manifest header contract is missing {required:?}"
                )));
            }
        }
        for (key, expected) in &self.header {
            let Some(found) = actual.get(key) else {
                return Err(invalid(format!("starlight map header is missing {key:?}")));
            };
            if found != expected {
                return Err(invalid(format!(
                    "starlight map header mismatch for {key:?}: expected {expected:?}, found {found:?}"
                )));
            }
        }
        if self.header.get("map_type").map(String::as_str) != Some("healpix")
            || self.header.get("coordinate_frame").map(String::as_str) != Some("galactic")
        {
            return Err(invalid(
                "validated external starlight must be a Galactic HEALPix map",
            ));
        }
        let expected_resolution = format!(
            "HEALPix nside={} ordering={}",
            self.header["nside"], self.header["ordering"]
        );
        if self.map_resolution != expected_resolution {
            return Err(invalid(format!(
                "map_resolution must be {expected_resolution:?}"
            )));
        }
        for (key, value) in [
            ("dataset_name", &self.dataset_name),
            ("version", &self.version),
            ("generation_date_utc", &self.generation_date),
            ("source_catalogue", &self.source_catalogue),
            ("source_catalogue_release", &self.source_catalogue_release),
            ("source_catalogue_license", &self.source_catalogue_license),
            ("source_catalogue_checksum", &self.source_catalogue_checksum),
            ("source_selection", &self.source_selection),
            ("magnitude_limit", &self.magnitude_limit),
            ("map_resolution", &self.map_resolution),
            ("photometry_model", &self.photometry_model),
            ("band_definition", &self.band_definition),
            ("smoothing", &self.smoothing),
            ("generated_by", &self.generated_by),
            ("generation_command", &self.generation_command),
            ("validation_report", &self.validation_report),
            ("independent_comparison", &self.independent_comparison),
        ] {
            if self.header.get(key) != Some(value) {
                return Err(invalid(format!(
                    "manifest field {key:?} does not match its header contract"
                )));
            }
        }
        if self.header.get("calibration_status").map(String::as_str) != Some("production") {
            return Err(invalid(
                "manifest header calibration_status must be exactly production",
            ));
        }
        Ok(())
    }

    fn provenance(&self, map_checksum: String) -> StarlightProvenance {
        StarlightProvenance {
            dataset_name: self.dataset_name.clone(),
            version: self.version.clone(),
            generation_date: self.generation_date.clone(),
            source_catalogue: self.source_catalogue.clone(),
            license: self.source_catalogue_license.clone(),
            magnitude_limit: self.magnitude_limit.clone(),
            band_definition: self.band_definition.clone(),
            map_resolution: self.map_resolution.clone(),
            checksum: Some(self.source_catalogue_checksum.clone()),
            map_checksum: Some(map_checksum),
            source_catalogue_release: Some(self.source_catalogue_release.clone()),
            photometry_model: Some(self.photometry_model.clone()),
            smoothing: Some(self.smoothing.clone()),
            generated_by: Some(self.generated_by.clone()),
            source_selection: Some(self.source_selection.clone()),
            generation_command: Some(self.generation_command.clone()),
            validation_report: Some(self.validation_report.clone()),
            calibration_status: Some("production".to_string()),
            independent_comparison: Some(self.independent_comparison.clone()),
        }
    }
}

fn normalize_checksum(value: &str) -> &str {
    value.trim().strip_prefix("sha256:").unwrap_or(value.trim())
}

fn validate_sha256(name: &str, value: &str) -> Result<()> {
    let digest = normalize_checksum(value);
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid(format!(
            "external starlight manifest field {name} must be a 64-digit SHA-256"
        )));
    }
    Ok(())
}

fn reject_placeholder(name: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if [
        "none",
        "unknown",
        "pending",
        "missing",
        "not available",
        "not recorded",
        "not performed",
        "review required",
        "unreviewed",
    ]
    .iter()
    .any(|blocked| normalized == *blocked || normalized.starts_with(&format!("{blocked}:")))
    {
        return Err(invalid(format!(
            "external starlight manifest field {name} contains placeholder evidence"
        )));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> NsbError {
    NsbError::InvalidMap {
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::cartesian::Direction;
    use siderust::coordinates::frames::Galactic;
    use siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, Nside};

    fn fixture() -> (Vec<u8>, String) {
        let grid = HealpixGrid::new(Nside::new(8).unwrap(), HealpixOrdering::Ring).unwrap();
        let mut raw = String::from(concat!(
            "# map_type=healpix\n",
            "# coordinate_frame=galactic\n",
            "# nside=8\n",
            "# ordering=ring\n",
            "# dataset_name=synthetic validated admission fixture\n",
                "# version=fixture-v1\n",
                "# generation_date_utc=2026-06-24T00:00:00Z\n",
            "# source_catalogue=synthetic fixture catalogue\n",
            "# source_catalogue_release=fixture-release\n",
                "# source_catalogue_license=CC0-1.0\n",
                "# source_catalogue_checksum=sha256:1111111111111111111111111111111111111111111111111111111111111111\n",
            "# source_selection=complete synthetic plane-enhanced fixture\n",
            "# magnitude_limit=not applicable to synthetic fixture\n",
            "# map_resolution=HEALPix nside=8 ordering=ring\n",
            "# calibration_status=production\n",
            "# photometry_model=synthetic-passband-integrated-v1\n",
                "# band_definition=synthetic integrated 300-650 nm test band\n",
                "# smoothing=none\n",
                "# generated_by=unit test\n",
                "# generation_command=synthetic fixture builder\n",
                "# validation_report=test admission report\n",
                "# independent_comparison=synthetic trusted reference fixture\n",
            "healpix_index,integrated_ph_cm2_ns_sr,b_s10,v_s10\n",
        ));
        let mut source_flux = 0.0;
        for index in 0..grid.npix() {
            let direction: Direction<Galactic> =
                grid.pixel_center(HealpixIndex::new(index)).unwrap();
            let latitude = direction.as_array()[2].asin().to_degrees().abs();
            let value = if latitude <= 10.0 { 2.0 } else { 1.0 };
            source_flux += value * grid.pixel_area_sr();
            raw.push_str(&format!("{index},{value},{value},{value}\n"));
        }
        let checksum = format!("sha256:{}", to_hex(&sha256(raw.as_bytes())));
        let manifest = format!(
            r#"schema_version = 1
calibration_status = "production"
dataset_name = "synthetic validated admission fixture"
version = "fixture-v1"
generation_date = "2026-06-24T00:00:00Z"
source_catalogue = "synthetic fixture catalogue"
source_catalogue_release = "fixture-release"
source_catalogue_license = "CC0-1.0"
source_catalogue_checksum = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
source_selection = "complete synthetic plane-enhanced fixture"
magnitude_limit = "not applicable to synthetic fixture"
map_resolution = "HEALPix nside=8 ordering=ring"
photometry_model = "synthetic-passband-integrated-v1"
band_definition = "synthetic integrated 300-650 nm test band"
smoothing = "none"
generated_by = "unit test"
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
dataset_name = "synthetic validated admission fixture"
version = "fixture-v1"
generation_date_utc = "2026-06-24T00:00:00Z"
source_catalogue = "synthetic fixture catalogue"
source_catalogue_release = "fixture-release"
source_catalogue_license = "CC0-1.0"
source_catalogue_checksum = "sha256:1111111111111111111111111111111111111111111111111111111111111111"
source_selection = "complete synthetic plane-enhanced fixture"
magnitude_limit = "not applicable to synthetic fixture"
map_resolution = "HEALPix nside=8 ordering=ring"
calibration_status = "production"
photometry_model = "synthetic-passband-integrated-v1"
band_definition = "synthetic integrated 300-650 nm test band"
smoothing = "none"
generated_by = "unit test"
generation_command = "synthetic fixture builder"
validation_report = "test admission report"
independent_comparison = "synthetic trusted reference fixture"
"#,
        );
        (raw.into_bytes(), manifest)
    }

    fn attach_source_candidate(
        map: Vec<u8>,
        manifest: String,
        candidate_sha: &str,
        extra_toml: &str,
    ) -> (Vec<u8>, String) {
        let text = String::from_utf8(map).unwrap();
        let injected = text.replacen(
            "# independent_comparison=synthetic trusted reference fixture\n",
            &format!(
                "# independent_comparison=synthetic trusted reference fixture\n# source_candidate_sha256=sha256:{candidate_sha}\n"
            ),
            1,
        );
        let checksum = format!("sha256:{}", to_hex(&sha256(injected.as_bytes())));
        let mut rewritten = String::new();
        for line in manifest.lines() {
            if line.starts_with("map_sha256") {
                rewritten.push_str(&format!("map_sha256 = \"{checksum}\"\n"));
            } else {
                rewritten.push_str(line);
                rewritten.push('\n');
            }
        }
        let rewritten = rewritten.replace(
            "integrated_flux_conservation_tolerance = 0.000000001\n\n[header]\n",
            &format!(
                "integrated_flux_conservation_tolerance = 0.000000001\n\n[source_candidate]\nsha256 = \"{candidate_sha}\"\n{extra_toml}[header]\n"
            ),
        );
        let rewritten = format!(
            "{}\nsource_candidate_sha256 = \"sha256:{candidate_sha}\"\n",
            rewritten.trim_end()
        );
        (injected.into_bytes(), rewritten)
    }

    #[test]
    fn admits_complete_validated_external_map() {
        let (map, manifest) = fixture();
        let validated = ValidatedStarlightMap::from_bytes_and_manifest(&map, &manifest).unwrap();
        assert_eq!(validated.diagnostics().pixel_count, 768);
        assert_eq!(
            validated.diagnostics().radiance_field,
            "integrated_ph_cm2_ns_sr"
        );
        assert!(validated.diagnostics().plane_pole_ratio > 1.0);
        assert!(validated.diagnostics().longitude_wrap_relative_jump < 0.1);
        assert!(validated.diagnostics().flux_conservation_recomputed);
        assert_eq!(
            validated.map().provenance().calibration_status.as_deref(),
            Some("production")
        );
        assert!(validated.map().provenance().map_checksum.is_some());
    }

    #[test]
    fn rejects_checksum_drift_and_proxy_photometry() {
        let (mut map, manifest) = fixture();
        map.push(b'\n');
        assert!(ValidatedStarlightMap::from_bytes_and_manifest(&map, &manifest).is_err());

        let (map, manifest) = fixture();
        let proxy = manifest.replace(
            "photometry_model = \"synthetic-passband-integrated-v1\"",
            "photometry_model = \"v_s10_scaled_integrated_proxy_v1\"",
        );
        let err = ValidatedStarlightMap::from_bytes_and_manifest(&map, &proxy).unwrap_err();
        assert!(err.to_string().contains("proxy or experimental photometry"));
    }

    #[test]
    fn rejects_candidate_sha_used_as_catalogue_checksum() {
        let (map, manifest) = fixture();
        let (map, poisoned) = attach_source_candidate(
            map,
            manifest,
            "1111111111111111111111111111111111111111111111111111111111111111",
            "",
        );
        let err = ValidatedStarlightMap::from_bytes_and_manifest(&map, &poisoned).unwrap_err();
        assert!(
            err.to_string()
                .contains("must identify an upstream catalogue manifest"),
            "{err}"
        );
    }

    #[test]
    fn rejects_missing_or_wrong_upstream_pins() {
        let (map, manifest) = fixture();
        let (map, missing) = attach_source_candidate(
            map,
            manifest,
            "2222222222222222222222222222222222222222222222222222222222222222",
            "",
        );
        assert!(
            ValidatedStarlightMap::from_bytes_and_manifest(&map, &missing)
                .unwrap_err()
                .to_string()
                .contains("gaia-source"),
        );

        let (map, manifest) = fixture();
        let (map, wrong) = attach_source_candidate(
            map,
            manifest,
            "2222222222222222222222222222222222222222222222222222222222222222",
            concat!(
                "[[upstream_inputs]]\n",
                "id = \"gaia-source\"\n",
                "release = \"Gaia DR3\"\n",
                "checksum_manifest_sha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\n\n",
                "[[upstream_inputs]]\n",
                "id = \"xp-continuous\"\n",
                "release = \"Gaia DR3\"\n",
                "checksum_manifest_sha256 = \"f23df1ffb45b19fc3f34d6f37791179cef1ebec6c5b9fd613a488b3be580fccd\"\n",
            ),
        );
        let wrong = wrong.replace(
            "source_catalogue_checksum = \"sha256:1111111111111111111111111111111111111111111111111111111111111111\"",
            "source_catalogue_checksum = \"sha256:9ec782f9c83b29885924c7d47bba18d70c86b8cbefbc408b19090b6a76e8e369\"",
        );
        assert!(ValidatedStarlightMap::from_bytes_and_manifest(&map, &wrong)
            .unwrap_err()
            .to_string()
            .contains("gaia-source checksum_manifest_sha256"),);
    }
}
