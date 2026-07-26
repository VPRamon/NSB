use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

pub const RUN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum DatasetName {
    AirglowContinuum,
    SolarSpectrum,
    MoonlightScattering,
    Starlight,
}

impl DatasetName {
    pub const ALL: [Self; 4] = [
        Self::AirglowContinuum,
        Self::SolarSpectrum,
        Self::MoonlightScattering,
        Self::Starlight,
    ];

    pub fn slug(self) -> &'static str {
        match self {
            Self::AirglowContinuum => "airglow-continuum",
            Self::SolarSpectrum => "solar-spectrum",
            Self::MoonlightScattering => "moonlight-scattering",
            Self::Starlight => "starlight",
        }
    }
}

impl fmt::Display for DatasetName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.slug())
    }
}

impl FromStr for DatasetName {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|dataset| dataset.slug() == value)
            .ok_or_else(|| format!("unsupported dataset {value:?}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Executor {
    Local,
    Slurm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Operation {
    Update,
    Build,
    Validate,
    Publish,
}

impl fmt::Display for Operation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Update => "update",
            Self::Build => "build",
            Self::Validate => "validate",
            Self::Publish => "publish",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildPlan {
    pub dataset: DatasetName,
    pub operation: Operation,
    pub executor: Executor,
    pub partitions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub name: String,
    pub path: PathBuf,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RunStatus {
    Planned,
    Submitted,
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationGate {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationReport {
    pub schema_version: u32,
    pub dataset: DatasetName,
    pub passed: bool,
    pub gates: Vec<ValidationGate>,
    pub artifacts: Vec<Artifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunManifest {
    pub schema_version: u32,
    pub dataset: DatasetName,
    pub operation: Operation,
    pub executor: Executor,
    pub status: RunStatus,
    pub config_path: PathBuf,
    pub config_sha256: String,
    pub software_commit: String,
    pub resolved_workspace: PathBuf,
    pub partitions: Vec<String>,
    pub artifacts: Vec<Artifact>,
    pub validation_report: Option<PathBuf>,
    pub slurm_job_id: Option<String>,
    pub error: Option<String>,
}
