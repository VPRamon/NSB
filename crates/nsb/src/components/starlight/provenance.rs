use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
/// Provenance carried by every starlight map.
pub struct StarlightProvenance {
    /// Human-readable dataset name.
    pub dataset_name: String,
    /// Dataset version identifier.
    pub version: String,
    /// UTC generation date or timestamp.
    pub generation_date: String,
    /// Source stellar catalogue.
    pub source_catalogue: String,
    /// Source catalogue or derived-product license.
    pub license: String,
    /// Applied magnitude selection.
    pub magnitude_limit: String,
    /// Integrated and diagnostic band definition.
    pub band_definition: String,
    /// Map grid resolution and ordering.
    pub map_resolution: String,
    /// Source catalogue checksum when supplied.
    pub checksum: Option<String>,
    /// Source catalogue release identifier.
    pub source_catalogue_release: Option<String>,
    /// Photometric conversion model identifier.
    pub photometry_model: Option<String>,
    /// Angular smoothing description.
    pub smoothing: Option<String>,
    /// Generator program and version information.
    pub generated_by: Option<String>,
}

impl StarlightProvenance {
    /// Construct required provenance fields for a caller-provided map.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dataset_name: impl Into<String>,
        version: impl Into<String>,
        generation_date: impl Into<String>,
        source_catalogue: impl Into<String>,
        license: impl Into<String>,
        magnitude_limit: impl Into<String>,
        band_definition: impl Into<String>,
        map_resolution: impl Into<String>,
        checksum: Option<impl Into<String>>,
    ) -> Self {
        Self {
            dataset_name: dataset_name.into(),
            version: version.into(),
            generation_date: generation_date.into(),
            source_catalogue: source_catalogue.into(),
            license: license.into(),
            magnitude_limit: magnitude_limit.into(),
            band_definition: band_definition.into(),
            map_resolution: map_resolution.into(),
            checksum: checksum.map(Into::into),
            source_catalogue_release: None,
            photometry_model: None,
            smoothing: None,
            generated_by: None,
        }
    }

    /// Merge machine-readable CSV header metadata over fallback provenance.
    pub fn from_header_metadata(metadata: &BTreeMap<String, String>, fallback: Self) -> Self {
        let nside = metadata.get("nside");
        let ordering = metadata.get("ordering");
        let map_resolution = metadata
            .get("map_resolution")
            .cloned()
            .or_else(|| match (nside, ordering) {
                (Some(nside), Some(ordering)) => {
                    Some(format!("HEALPix nside={nside} ordering={ordering}"))
                }
                (Some(nside), None) => Some(format!("HEALPix nside={nside}")),
                _ => None,
            })
            .unwrap_or(fallback.map_resolution);

        Self {
            dataset_name: metadata
                .get("dataset_name")
                .cloned()
                .unwrap_or(fallback.dataset_name),
            version: metadata.get("version").cloned().unwrap_or(fallback.version),
            generation_date: metadata
                .get("generation_date_utc")
                .or_else(|| metadata.get("generation_date"))
                .cloned()
                .unwrap_or(fallback.generation_date),
            source_catalogue: metadata
                .get("source_catalogue")
                .or_else(|| metadata.get("source_catalog_name"))
                .cloned()
                .unwrap_or(fallback.source_catalogue),
            license: metadata
                .get("source_catalogue_license")
                .or_else(|| metadata.get("license"))
                .cloned()
                .unwrap_or(fallback.license),
            magnitude_limit: metadata
                .get("magnitude_limit")
                .cloned()
                .unwrap_or(fallback.magnitude_limit),
            band_definition: metadata
                .get("band_definition")
                .cloned()
                .unwrap_or(fallback.band_definition),
            map_resolution,
            checksum: metadata
                .get("source_catalogue_checksum")
                .or_else(|| metadata.get("checksum"))
                .cloned()
                .or(fallback.checksum),
            source_catalogue_release: metadata
                .get("source_catalogue_release")
                .cloned()
                .or(fallback.source_catalogue_release),
            photometry_model: metadata
                .get("photometry_model")
                .cloned()
                .or(fallback.photometry_model),
            smoothing: metadata
                .get("smoothing_fwhm_deg")
                .or_else(|| metadata.get("smoothing"))
                .cloned()
                .or(fallback.smoothing),
            generated_by: metadata
                .get("generated_by")
                .cloned()
                .or(fallback.generated_by),
        }
    }

    /// Fallback provenance contract for the bundled experimental seed.
    pub fn experimental_seed_v1() -> Self {
        Self {
            dataset_name: "NSB catalogue-derived Galactic starlight map".to_string(),
            version: "v1".to_string(),
            generation_date: "read from bundled map header".to_string(),
            source_catalogue: "read from bundled map header".to_string(),
            license: "read from bundled map header".to_string(),
            magnitude_limit: "read from bundled map header".to_string(),
            band_definition: "integrated 300-650 nm photon radiance plus B/V S10 diagnostics"
                .to_string(),
            map_resolution: "read from bundled map header".to_string(),
            checksum: None,
            source_catalogue_release: None,
            photometry_model: Some("v_s10_scaled_integrated_v1".to_string()),
            smoothing: None,
            generated_by: Some("nsb-data-tools using siderust".to_string()),
        }
    }

    /// Provenance for deterministic test-only maps.
    pub fn test_fixture() -> Self {
        Self {
            dataset_name: "NSB test fixture starlight map".to_string(),
            version: "fixture".to_string(),
            generation_date: "2026-06-17".to_string(),
            source_catalogue: "synthetic unit-test fixture".to_string(),
            license: "test-only".to_string(),
            magnitude_limit: "test-only".to_string(),
            band_definition: "integrated 300-650 nm photon radiance plus B/V S10".to_string(),
            map_resolution: "90 deg lon x 45 deg lat".to_string(),
            checksum: None,
            source_catalogue_release: Some("test".to_string()),
            photometry_model: Some("fixture".to_string()),
            smoothing: None,
            generated_by: Some("test".to_string()),
        }
    }
}
