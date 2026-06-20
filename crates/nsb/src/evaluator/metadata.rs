use super::{MoonlightModel, Observer, StarlightModel};
use crate::site::SiteProfileId;
use crate::NSB_S10_ZP;
use std::borrow::Cow;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentCalibrationStatus {
    Production,
    GenericClearSky,
    PlanningPreset,
    Proxy,
    Legacy,
    Experimental,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandDiagnostic {
    pub convention: &'static str,
    pub b_reference_nm: f64,
    pub v_reference_nm: f64,
    pub zero_point: f64,
}

impl BandDiagnostic {
    pub const MONOCHROMATIC_S10_PROXY: Self = Self {
        convention: "monochromatic-central-wavelength-s10-proxy",
        b_reference_nm: 445.0,
        v_reference_nm: 551.0,
        zero_point: NSB_S10_ZP,
    };
}

#[derive(Debug, Clone, PartialEq)]
pub struct NsbComponentMetadata {
    pub status: ComponentCalibrationStatus,
    pub provenance: Cow<'static, str>,
    pub validated_domain: Cow<'static, str>,
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

pub(super) fn airglow_metadata(site_profile: SiteProfileId, observer: Observer) -> NsbComponentMetadata {
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

pub(super) fn starlight_metadata(model: &StarlightModel) -> NsbComponentMetadata {
    match model {
        StarlightModel::BundledCatalogueMap => NsbComponentMetadata {
            status: ComponentCalibrationStatus::Experimental,
            provenance: "future bundled catalogue-derived Galactic starlight map: data/starlight_galactic_map_v1.csv".into(),
            validated_domain: "not production-valid until the catalogue map is generated, bundled, and quantitatively validated".into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        },
        StarlightModel::CustomMap(map) => {
            let provenance = map.provenance();
            let checksum = provenance.checksum.as_deref().unwrap_or("not recorded");
            NsbComponentMetadata {
                status: ComponentCalibrationStatus::Experimental,
                provenance: Cow::Owned(format!(
                    "{}; version {}; generated {}; source {}; license {}; magnitude limit {}; band {}; resolution {}; checksum {}",
                    provenance.dataset_name.as_str(),
                    provenance.version.as_str(),
                    provenance.generation_date.as_str(),
                    provenance.source_catalogue.as_str(),
                    provenance.license.as_str(),
                    provenance.magnitude_limit.as_str(),
                    provenance.band_definition.as_str(),
                    provenance.map_resolution.as_str(),
                    checksum
                )),
                validated_domain: Cow::Owned(format!(
                    "caller-provided map provenance: dataset {}; version {}; checksum {}",
                    provenance.dataset_name.as_str(),
                    provenance.version.as_str(),
                    checksum
                )),
                band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
            }
        }
        StarlightModel::Disabled => NsbComponentMetadata {
            status: ComponentCalibrationStatus::Experimental,
            provenance: "no starlight model configured".into(),
            validated_domain: "not evaluable".into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        },
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
            status: ComponentCalibrationStatus::Legacy,
            provenance: "Krisciunas & Schaefer 1991 analytic V-band moonlight model".into(),
            validated_domain: "legacy regression/parity model, not the current wavelength-resolved default".into(),
            band_diagnostic: BandDiagnostic::MONOCHROMATIC_S10_PROXY,
        },
    }
}
