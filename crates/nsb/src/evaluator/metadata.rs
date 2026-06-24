use super::{MoonlightModel, Observer, StarlightModel};
use crate::components::starlight::StarlightProvenance;
use crate::site::SiteProfileId;
use crate::NSB_S10_ZP;
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
    pub b_reference_nm: f64,
    /// V diagnostic reference wavelength in nm.
    pub v_reference_nm: f64,
    /// S10-to-surface-brightness zero point.
    pub zero_point: f64,
}

impl BandDiagnostic {
    /// Central-wavelength S10 proxy used by current B/V fields.
    pub const MONOCHROMATIC_S10_PROXY: Self = Self {
        convention: "monochromatic-central-wavelength-s10-proxy",
        b_reference_nm: 445.0,
        v_reference_nm: 551.0,
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
    }
}

pub(super) fn airglow_metadata(
    site_profile: SiteProfileId,
    observer: Observer,
) -> NsbComponentMetadata {
    let profile = site_profile.profile(observer);
    NsbComponentMetadata {
        status: component_status_for_site_profile(site_profile),
        provenance: Cow::Owned(format!(
            "{}; site profile {}; template {}",
            profile.airglow.provenance, profile.name, profile.airglow.template
        )),
        validated_domain: Cow::Owned(format!(
            "astronomical-night continuum template with seasonal, time-of-night, solar-activity, and Van Rhijn corrections; {}",
            profile.airglow.assumptions
        )),
        band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
    }
}

pub(super) fn starlight_metadata(
    model: Option<&StarlightModel>,
    provenance: Option<&StarlightProvenance>,
) -> NsbComponentMetadata {
    match model {
        Some(StarlightModel::BundledExperimentalSeed) => starlight_map_metadata(
            provenance.expect("bundled experimental seed is loaded during evaluator construction"),
            "bundled experimental seed",
        ),
        Some(StarlightModel::CustomMap(_)) => starlight_map_metadata(
            provenance.expect("custom map is loaded during evaluator construction"),
            "caller-provided map",
        ),
        None => NsbComponentMetadata {
            status: ComponentCalibrationStatus::Experimental,
            provenance: "no starlight model configured".into(),
            validated_domain: "not evaluable".into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        },
    }
}

fn starlight_map_metadata(
    provenance: &crate::components::starlight::StarlightProvenance,
    source: &'static str,
) -> NsbComponentMetadata {
    let checksum = provenance.checksum.as_deref().unwrap_or("not recorded");
    NsbComponentMetadata {
        status: ComponentCalibrationStatus::Experimental,
        provenance: Cow::Owned(format!(
            "{}; version {}; generated {}; source {}; release {}; license {}; magnitude limit {}; band {}; resolution {}; photometry {}; smoothing {}; checksum {}; {}",
            provenance.dataset_name.as_str(),
            provenance.version.as_str(),
            provenance.generation_date.as_str(),
            provenance.source_catalogue.as_str(),
            provenance.source_catalogue_release.as_deref().unwrap_or("not recorded"),
            provenance.license.as_str(),
            provenance.magnitude_limit.as_str(),
            provenance.band_definition.as_str(),
            provenance.map_resolution.as_str(),
            provenance.photometry_model.as_deref().unwrap_or("not recorded"),
            provenance.smoothing.as_deref().unwrap_or("not recorded"),
            checksum,
            source,
        )),
        validated_domain: Cow::Owned(format!(
            "experimental catalogue-derived starlight map; dataset {}; version {}; checksum {}; external validation remains required before production use",
            provenance.dataset_name.as_str(),
            provenance.version.as_str(),
            checksum,
        )),
        band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
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
            }
        }
        MoonlightModel::KrisciunasSchaefer1991 => NsbComponentMetadata {
            status: ComponentCalibrationStatus::PublishedReference,
            provenance: "Krisciunas & Schaefer 1991 analytic V-band moonlight model".into(),
            validated_domain:
                "published analytic V-band reference model; not the wavelength-resolved default"
                    .into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        },
    }
}
