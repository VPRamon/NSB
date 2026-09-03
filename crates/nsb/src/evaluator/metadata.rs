use super::{MoonlightModel, Observer, StarlightModel};
use crate::components::airglow::calibration::{
    airglow_continuum_asset, AIRGLOW_CONTINUUM_ASSET_PATH,
};
use crate::components::airglow::NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG;
use crate::components::starlight::StarlightProvenance;
use crate::site::SiteProfileId;
use crate::NSB_S10_ZP;
use qtty::photometry::SurfaceBrightness;
use siderust::qtty::Nanometers;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Scientific calibration/maturity classification.
pub enum ComponentCalibrationStatus {
    /// Validated for its stated release domain.
    Production,
    /// Generic clear-sky assumptions, not site-calibrated.
    GenericClearSky,
    /// Named planning assumptions without dedicated calibration.
    PlanningPreset,
    /// Approximate conversion or engineering diagnostic.
    Proxy,
    /// Supported published comparison model.
    PublishedReference,
    /// Capability without a production validation contract.
    Experimental,
}

impl ComponentCalibrationStatus {
    /// Stable lowercase status identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::GenericClearSky => "generic-clear-sky",
            Self::PlanningPreset => "planning-preset",
            Self::Proxy => "proxy",
            Self::PublishedReference => "published-reference",
            Self::Experimental => "experimental",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
/// Meaning and zero point of reported B/V diagnostics.
pub struct BandDiagnostic {
    /// Stable diagnostic convention identifier.
    pub convention: &'static str,
    /// B diagnostic reference wavelength in nm.
    pub b_reference: Nanometers,
    /// V diagnostic reference wavelength in nm.
    pub v_reference: Nanometers,
    /// S10-to-surface-brightness zero point.
    pub zero_point: SurfaceBrightness,
}

impl BandDiagnostic {
    /// Central-wavelength S10 proxy used by current B/V fields.
    pub const MONOCHROMATIC_S10_PROXY: Self = Self {
        convention: "monochromatic-central-wavelength-s10-proxy",
        b_reference: Nanometers::new(445.0),
        v_reference: Nanometers::new(551.0),
        zero_point: NSB_S10_ZP,
    };
}

#[derive(Debug, Clone, PartialEq)]
/// Scientific interpretation attached to a component result.
pub struct NsbComponentMetadata {
    /// Calibration/maturity classification.
    pub status: ComponentCalibrationStatus,
    /// Source model and data provenance.
    pub provenance: Cow<'static, str>,
    /// Domain for which validation evidence exists.
    pub validated_domain: Cow<'static, str>,
    /// Meaning of B/V fields.
    pub band_diagnostic: BandDiagnostic,
    /// Optional resolved F10.7 provenance for airglow evaluations.
    pub solar_activity: Option<crate::solar_activity::ResolvedSolarActivity>,
    /// Optional emitting-volume geometry provenance for Airglow evaluations.
    pub airglow_geometry: Option<crate::components::airglow::AirglowGeometryMetadata>,
}

pub(super) fn component_status_for_site_profile(
    site_profile: SiteProfileId,
) -> ComponentCalibrationStatus {
    match site_profile {
        SiteProfileId::GenericClearSky => ComponentCalibrationStatus::GenericClearSky,
        SiteProfileId::CtaNorth | SiteProfileId::CtaSouth => {
            ComponentCalibrationStatus::PlanningPreset
        }
    }
}

pub(super) fn zodiacal_metadata() -> NsbComponentMetadata {
    NsbComponentMetadata {
        status: ComponentCalibrationStatus::GenericClearSky,
        provenance: "Leinert+1998 zodiacal S10 table; Noll+2012 approximate extinction; bundled solar spectrum".into(),
        validated_domain: "exoatmospheric Leinert table geometry plus generic Noll-style clear-sky attenuation".into(),
        band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        solar_activity: None,
        airglow_geometry: None,
    }
}

pub(super) fn airglow_metadata(
    site_profile: SiteProfileId,
    observer: Observer,
    solar: Option<&crate::solar_activity::ResolvedSolarActivity>,
    geometry: &crate::components::airglow::AirglowGeometryModel,
) -> NsbComponentMetadata {
    let profile = site_profile.profile(observer);
    let asset = airglow_continuum_asset();
    let baseline_identity = format!(
        "baseline asset {} schema {} sha256 {}; calibration_status {}; generator {}; validation_report {}; source {}; license {}; baseline_source Cerro Paranal / Noll / SkyCalc-derived; site_calibrated false",
        AIRGLOW_CONTINUUM_ASSET_PATH,
        asset.schema,
        asset.sha256,
        asset.calibration_status,
        asset.generator,
        asset.validation_report,
        asset.source,
        asset.license
    );
    let f107_fragment = match solar {
        Some(resolved) => resolved.provenance_fragment(),
        None => "F10.7 resolved per evaluation UTC date via SolarActivitySource (Automatic/Dataset/Explicit); not a site calibration".to_string(),
    };
    NsbComponentMetadata {
        status: component_status_for_site_profile(site_profile),
        provenance: Cow::Owned(format!(
            "{}; site profile {}; template {}; {}; {}",
            profile.airglow.provenance,
            profile.name,
            profile.airglow.template,
            baseline_identity,
            f107_fragment
        )),
        validated_domain: Cow::Owned(format!(
            "Paranal-derived FORS1/Noll/SkyCalc empirical continuum reused as an explicit generic/planning proxy for arbitrary locations (not globally calibrated); astronomical-night domain; integrated 300–650 nm with weaker evidence at the UV end (~300–365/400 nm); applies seasonal, time-of-night, solar-activity, selected emitting-volume LOS geometry ({}), and independent Noll-2012 effective Rayleigh/Mie airglow scattering (Noll §4.1; fitted primarily for zenith distances z≲{}°, larger angles are parametric extrapolation) using site-profile atmospheric pressure/Rayleigh/Mie assumptions ({}); molecular atmospheric absorption from the full Cerro Paranal ASM/SkyCalc pipeline is not reproduced, so full upstream numerical parity is not claimed; multiplied by site-profile airglow.scale (site scaling only, not calibrated continuum); measured F10.7 or geometry choice does not make Airglow site-calibrated; {}",
            geometry.model_id(),
            NOLL_AIRGLOW_SCATTERING_FIT_MAX_ZENITH_DEG as i32,
            profile.atmosphere_provenance,
            profile.airglow.assumptions
        )),
        band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        solar_activity: solar.cloned(),
        airglow_geometry: Some(geometry.metadata()),
    }
}

pub(super) fn starlight_metadata(
    model: Option<&StarlightModel>,
    provenance: Option<&StarlightProvenance>,
) -> NsbComponentMetadata {
    match model {
        Some(StarlightModel::BundledProductionGaiaDr3) => starlight_map_metadata(
            provenance.expect("bundled production map is loaded during evaluator construction"),
            "bundled Gaia DR3 XP production map",
            ComponentCalibrationStatus::Production,
            "bundled validated Gaia DR3 XP-derived HEALPix map with checksum/header consistency, flux-conservation evidence, plane/pole contrast, longitude wrap, and independent comparison",
        ),
        Some(StarlightModel::ExperimentalMap(_)) => starlight_map_metadata(
            provenance.expect("custom map is loaded during evaluator construction"),
            "caller-provided map",
            ComponentCalibrationStatus::Experimental,
            "caller-provided map without the production manifest contract",
        ),
        Some(StarlightModel::ValidatedExternalMap(_)) => starlight_map_metadata(
            provenance.expect("validated external map is loaded during evaluator construction"),
            "validated external map",
            ComponentCalibrationStatus::Production,
            "complete HEALPix coverage, finite/nonnegative values, checksum/header consistency, flux-conservation evidence, plane/pole contrast, longitude wrap, and independent comparison",
        ),
        None => NsbComponentMetadata {
            status: ComponentCalibrationStatus::Experimental,
            provenance: "no starlight model configured".into(),
            validated_domain: "not evaluable".into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
            solar_activity: None,
            airglow_geometry: None,
        },
    }
}

fn starlight_map_metadata(
    provenance: &crate::components::starlight::StarlightProvenance,
    source: &'static str,
    status: ComponentCalibrationStatus,
    validated_domain: &'static str,
) -> NsbComponentMetadata {
    let checksum = provenance.checksum.as_deref().unwrap_or("not recorded");
    let map_checksum = provenance.map_checksum.as_deref().unwrap_or("not recorded");
    NsbComponentMetadata {
        status,
        provenance: Cow::Owned(format!(
            "{}; version {}; generated {}; source {}; release {}; license {}; source selection {}; magnitude limit {}; band {}; resolution {}; photometry {}; smoothing {}; source checksum {}; map checksum {}; generator {}; command {}; validation report {}; independent comparison {}; calibration status {}; {}",
            provenance.dataset_name.as_str(),
            provenance.version.as_str(),
            provenance.generation_date.as_str(),
            provenance.source_catalogue.as_str(),
            provenance.source_catalogue_release.as_deref().unwrap_or("not recorded"),
            provenance.license.as_str(),
            provenance.source_selection.as_deref().unwrap_or("not recorded"),
            provenance.magnitude_limit.as_str(),
            provenance.band_definition.as_str(),
            provenance.map_resolution.as_str(),
            provenance.photometry_model.as_deref().unwrap_or("not recorded"),
            provenance.smoothing.as_deref().unwrap_or("not recorded"),
            checksum,
            map_checksum,
            provenance.generated_by.as_deref().unwrap_or("not recorded"),
            provenance.generation_command.as_deref().unwrap_or("not recorded"),
            provenance.validation_report.as_deref().unwrap_or("not recorded"),
            provenance.independent_comparison.as_deref().unwrap_or("not recorded"),
            provenance.calibration_status.as_deref().unwrap_or("not recorded"),
            source,
        )),
        validated_domain: Cow::Owned(format!(
            "{}; dataset {}; version {}; source checksum {}; map checksum {}",
            validated_domain,
            provenance.dataset_name.as_str(),
            provenance.version.as_str(),
            checksum,
            map_checksum,
        )),
        band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        solar_activity: None,
        airglow_geometry: None,
    }
}

pub(super) fn moonlight_metadata(
    model: MoonlightModel,
    site_profile: SiteProfileId,
    observer: Observer,
) -> NsbComponentMetadata {
    match model {
        MoonlightModel::Jones2013Spectral => {
            let profile = site_profile.profile(observer);
            NsbComponentMetadata {
                status: component_status_for_site_profile(site_profile),
                provenance: Cow::Owned(format!(
                    "Jones+2013 wavelength-resolved lunar radiance with Siderust lunar geometry; site profile {}; atmosphere: {}",
                    profile.name, profile.atmosphere_provenance
                )),
                validated_domain: Cow::Owned(format!(
                    "{} planning model; site aerosol calibration and external SkyCalc/observational validation remain explicit release gates",
                    profile.name
                )),
                band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
                solar_activity: None,
                airglow_geometry: None,
            }
        }
        MoonlightModel::KrisciunasSchaefer1991 => NsbComponentMetadata {
            status: ComponentCalibrationStatus::PublishedReference,
            provenance: "Krisciunas & Schaefer 1991 analytic V-band moonlight model".into(),
            validated_domain:
                "published analytic V-band reference model; not the wavelength-resolved default"
                    .into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
            solar_activity: None,
            airglow_geometry: None,
        },
    }
}
