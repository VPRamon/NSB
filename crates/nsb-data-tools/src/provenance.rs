//! Canonical software-version and timestamp resolution for generated artefacts.

use anyhow::{bail, Result};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::process::Command;

/// Versioned execution provenance shared by generated reports and manifests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionProvenance {
    /// Provenance schema version.
    pub schema_version: u32,
    /// Git commit, or `unknown` only for non-production developer runs.
    pub software_commit: String,
    /// UTC generation timestamp in RFC3339 seconds form.
    pub generated_at_utc: String,
}

impl ExecutionProvenance {
    /// Capture canonical provenance using the shared resolution order.
    pub fn capture() -> Self {
        Self {
            schema_version: 1,
            software_commit: resolve_software_commit(),
            generated_at_utc: utc_now_rfc3339_seconds(),
        }
    }

    /// Reject incomplete provenance before production admission.
    pub fn validate_for_production(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!(
                "unsupported execution provenance schema {}",
                self.schema_version
            );
        }
        if self.software_commit == "unknown" || self.software_commit.trim().is_empty() {
            bail!("production execution provenance requires a known software commit");
        }
        if chrono::DateTime::parse_from_rfc3339(&self.generated_at_utc).is_err() {
            bail!("execution provenance timestamp must be RFC3339");
        }
        Ok(())
    }
}

/// Return the current UTC timestamp with deterministic second precision.
pub fn utc_now_rfc3339_seconds() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Resolve the software commit consistently across local and CI execution.
///
/// Resolution order is `NSB_SOFTWARE_COMMIT`, `GITHUB_SHA`, local
/// `git rev-parse HEAD`, then `unknown` for candidate/developer operation.
pub fn resolve_software_commit() -> String {
    resolve_software_commit_from(
        std::env::var("NSB_SOFTWARE_COMMIT").ok().as_deref(),
        std::env::var("GITHUB_SHA").ok().as_deref(),
        git_head().as_deref(),
    )
}

fn git_head() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn resolve_software_commit_from(
    explicit: Option<&str>,
    github: Option<&str>,
    git: Option<&str>,
) -> String {
    [explicit, github, git]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolution_order_is_stable() {
        assert_eq!(
            resolve_software_commit_from(Some(" explicit "), Some("github"), Some("git")),
            "explicit"
        );
        assert_eq!(
            resolve_software_commit_from(None, Some(" github "), Some("git")),
            "github"
        );
        assert_eq!(resolve_software_commit_from(None, None, None), "unknown");
    }

    #[test]
    fn production_rejects_unknown_commit() {
        let provenance = ExecutionProvenance {
            schema_version: 1,
            software_commit: "unknown".to_string(),
            generated_at_utc: "2026-07-12T16:00:00Z".to_string(),
        };
        assert!(provenance.validate_for_production().is_err());
    }
}
