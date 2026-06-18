use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    pub observer: Option<ObserverConfig>,
    pub target: Option<TargetConfig>,
    pub components: Option<ComponentsConfig>,
    pub constraints: Option<ConstraintsConfig>,
    pub nsb: Option<NsbBoundsConfig>,
    pub output: Option<OutputConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObserverConfig {
    pub site: Option<String>,
    pub lon_deg: Option<f64>,
    pub lat_deg: Option<f64>,
    pub height_m: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetConfig {
    pub ra_deg: f64,
    pub dec_deg: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentsConfig {
    pub zodiacal: bool,
    pub starlight: bool,
    pub airglow: bool,
    pub moon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintsConfig {
    pub sun_altitude_max_deg: f64,
    pub target_altitude_min_deg: f64,
    pub sample_step_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NsbBoundsConfig {
    pub min_ph_cm2_ns_sr: Option<f64>,
    pub max_ph_cm2_ns_sr: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfig {
    pub format: String,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            observer: Some(ObserverConfig {
                site: Some("CTAO-S".to_string()),
                lon_deg: None,
                lat_deg: None,
                height_m: None,
            }),
            target: Some(TargetConfig {
                ra_deg: 83.6331,
                dec_deg: 22.0145,
            }),
            components: Some(ComponentsConfig {
                zodiacal: true,
                starlight: false,
                airglow: true,
                moon: true,
            }),
            constraints: Some(ConstraintsConfig {
                sun_altitude_max_deg: -18.0,
                target_altitude_min_deg: 0.0,
                sample_step_seconds: 600.0,
            }),
            nsb: Some(NsbBoundsConfig {
                min_ph_cm2_ns_sr: None,
                max_ph_cm2_ns_sr: 0.25,
            }),
            output: Some(OutputConfig {
                format: "table".to_string(),
            }),
        }
    }
}
