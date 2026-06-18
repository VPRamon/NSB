#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarlightProvenance {
    pub dataset_name: String,
    pub version: String,
    pub generation_date: String,
    pub source_catalogue: String,
    pub license: String,
    pub magnitude_limit: String,
    pub band_definition: String,
    pub map_resolution: String,
    pub checksum: Option<String>,
}

impl StarlightProvenance {
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
        }
    }

    pub fn standard_galactic_model_v1() -> Self {
        Self::new(
            "NSB standard Galactic starlight map",
            "v1",
            "not generated",
            "not bundled",
            "not recorded",
            "not recorded",
            "integrated 300-650 nm photon radiance plus B/V S10",
            "not recorded",
            None::<String>,
        )
    }

    pub fn test_fixture() -> Self {
        Self::new(
            "NSB test fixture starlight map",
            "fixture",
            "2026-06-17",
            "synthetic unit-test fixture",
            "test-only",
            "test-only",
            "integrated 300-650 nm photon radiance plus B/V S10",
            "90 deg lon x 45 deg lat",
            None::<String>,
        )
    }
}
