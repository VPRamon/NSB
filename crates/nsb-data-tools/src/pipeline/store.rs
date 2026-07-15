//! Transactional persistence for strict pipeline state records.

use super::state::PartitionState;
use crate::artifact_io;
use anyhow::Result;
use std::path::Path;

/// Atomically persist a validated partition state record.
pub fn write_partition_state(path: &Path, state: &PartitionState) -> Result<()> {
    state.validate()?;
    artifact_io::write_json_atomic(path, state)
}

/// Read and strictly validate a persisted partition state record.
pub fn read_partition_state(path: &Path) -> Result<PartitionState> {
    let state: PartitionState = artifact_io::read_json(path)?;
    state.validate()?;
    Ok(state)
}
