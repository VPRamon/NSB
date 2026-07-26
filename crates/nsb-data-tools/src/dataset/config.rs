use super::{DatasetName, Executor};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunConfig {
    pub schema_version: u32,
    pub dataset: DatasetName,
    pub workspace: WorkspaceConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    pub sources: Vec<SourceConfig>,
    pub publish: Option<PublishConfig>,
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
    pub slurm: Option<SlurmConfig>,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            executor: default_executor(),
            concurrency: default_concurrency(),
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

impl RunConfig {
    pub fn load(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        let mut config: Self =
            toml::from_str(&raw).with_context(|| format!("invalid config {}", path.display()))?;
        if config.schema_version != 1 {
            bail!(
                "unsupported config schema_version {}",
                config.schema_version
            );
        }
        if config.sources.is_empty() {
            bail!("configuration requires at least one source");
        }
        if config.execution.concurrency == 0 {
            bail!("execution.concurrency must be greater than zero");
        }
        let base = path.parent().unwrap_or_else(|| Path::new("."));
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
        if let Some(publish) = &mut config.publish {
            publish.repository_root = resolve(base, &publish.repository_root);
        }
        Ok(config)
    }
}

fn resolve(base: &Path, value: &Path) -> PathBuf {
    if value.is_absolute() {
        value.to_path_buf()
    } else {
        base.join(value)
    }
}
