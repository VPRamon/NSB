use super::{DatasetName, Executor};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub schema_version: u32,
    pub dataset: DatasetName,
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub sources: Vec<SourceConfig>,
    pub publish: Option<PublishConfig>,
    pub starlight: Option<crate::starlight::config::StarlightConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceConfig {
    pub name: String,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
    pub sha256: String,
    #[serde(default)]
    pub partition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionConfig {
    #[serde(default = "default_executor")]
    pub executor: Executor,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
    #[serde(default = "default_lease_timeout_seconds")]
    pub lease_timeout_seconds: u64,
    pub slurm: Option<SlurmConfig>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            executor: default_executor(),
            concurrency: default_concurrency(),
            lease_timeout_seconds: default_lease_timeout_seconds(),
            slurm: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlurmConfig {
    pub partition: Option<String>,
    pub account: Option<String>,
    pub time_limit: Option<String>,
    pub memory: Option<String>,
    #[serde(default = "default_array_parallelism")]
    pub array_parallelism: usize,
    #[serde(default = "default_max_array_size")]
    pub max_array_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishConfig {
    pub repository_root: PathBuf,
}

fn default_executor() -> Executor {
    Executor::Local
}

fn default_concurrency() -> usize {
    1
}

fn default_array_parallelism() -> usize {
    1
}

fn default_max_array_size() -> usize {
    1000
}

fn default_lease_timeout_seconds() -> u64 {
    24 * 60 * 60
}

impl RunConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let absolute_path = fs::canonicalize(path)
            .with_context(|| format!("failed to resolve config {}", path.display()))?;
        let raw = fs::read_to_string(&absolute_path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Self =
            toml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))?;
        if config.schema_version != 1 {
            bail!(
                "unsupported config schema_version {}",
                config.schema_version
            );
        }
        if config.sources.is_empty()
            && config.starlight.as_ref().is_none_or(|starlight| {
                starlight.mode != crate::starlight::config::StarlightMode::Production
            })
        {
            bail!("configuration requires at least one source");
        }
        if config.execution.concurrency == 0 {
            bail!("execution.concurrency must be greater than zero");
        }
        if config.execution.lease_timeout_seconds == 0 {
            bail!("execution.lease_timeout_seconds must be greater than zero");
        }
        let base = absolute_path
            .parent()
            .context("configuration path has no parent")?;
        config.workspace.root = resolve(base, &config.workspace.root);
        for source in &mut config.sources {
            match (&mut source.path, &source.url) {
                (Some(path), None) => *path = resolve(base, path),
                (None, Some(url)) if url.starts_with("https://") => {}
                (Some(_), Some(_)) => bail!("source must define exactly one of path or url"),
                (None, Some(_)) => bail!("source URL must use HTTPS"),
                (None, None) => bail!("source must define path or url"),
            }
            if source.name.trim().is_empty() || source.sha256.len() != 64 {
                bail!("every source requires a name and lowercase SHA-256");
            }
        }
        if let Some(uv) = config
            .starlight
            .as_mut()
            .and_then(|starlight| starlight.ultraviolet_correction.as_mut())
        {
            uv.artifact_path = resolve(base, &uv.artifact_path);
        }
        if let Some(publish) = &mut config.publish {
            publish.repository_root = resolve(base, &publish.repository_root);
        }
        Ok(config)
    }
}

fn resolve(base: &Path, value: &Path) -> PathBuf {
    let joined = if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.join(value)
    };
    normalize_absolute(&joined)
}

fn normalize_absolute(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}
