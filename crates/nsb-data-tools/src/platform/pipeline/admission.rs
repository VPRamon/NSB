//! Fail-closed aggregation of production-readiness evidence.

use super::contracts::{Gate, PIPELINE_SCHEMA_VERSION};
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Result of evaluating all production-admission evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    /// Every required gate executed and passed, with no explicit blockers.
    Ready,
    /// Admission is blocked by the listed deterministic reasons.
    Blocked(Vec<String>),
}

impl AdmissionDecision {
    /// Process exit code for a command whose contract is production admission.
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Ready => 0,
            Self::Blocked(_) => 2,
        }
    }

    /// Whether the decision admits production work.
    pub const fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Blockers associated with a rejected decision.
    pub fn blockers(&self) -> &[String] {
        match self {
            Self::Ready => &[],
            Self::Blocked(blockers) => blockers,
        }
    }
}

/// Versioned admission report persisted by production orchestrators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProductionAdmission {
    /// Contract schema version.
    pub schema_version: u32,
    /// Individually named gate outcomes.
    pub gates: Vec<Gate>,
    /// Additional blockers not represented by a single gate.
    pub blockers: Vec<String>,
}

impl ProductionAdmission {
    /// Create an empty admission report using the current schema.
    pub fn new(gates: Vec<Gate>, blockers: Vec<String>) -> Self {
        Self {
            schema_version: PIPELINE_SCHEMA_VERSION,
            gates,
            blockers,
        }
    }

    /// Validate the report and return a deterministic fail-closed decision.
    pub fn evaluate(&self) -> Result<AdmissionDecision> {
        self.validate()?;
        let mut blockers = self.blockers.clone();
        for gate in &self.gates {
            if gate.required_for_production && !gate.status.is_passed() {
                let detail = gate
                    .status
                    .blocker_reason()
                    .unwrap_or("gate did not produce an executed pass");
                blockers.push(format!("{}: {detail}", gate.name));
            }
        }
        blockers.sort();
        blockers.dedup();
        if blockers.is_empty() {
            Ok(AdmissionDecision::Ready)
        } else {
            Ok(AdmissionDecision::Blocked(blockers))
        }
    }

    /// Validate schema, gate uniqueness, and blocker detail.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != PIPELINE_SCHEMA_VERSION {
            bail!(
                "unsupported production-admission schema version {}; expected {}",
                self.schema_version,
                PIPELINE_SCHEMA_VERSION
            );
        }
        let mut names = BTreeSet::new();
        for gate in &self.gates {
            gate.validate()?;
            if !names.insert(gate.name.as_str()) {
                bail!("duplicate production-admission gate {:?}", gate.name);
            }
        }
        if self.blockers.iter().any(|value| value.trim().is_empty()) {
            bail!("production-admission blockers must be non-empty");
        }
        Ok(())
    }
}
