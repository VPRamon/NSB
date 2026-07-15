//! Shared infrastructure for durable data-product workflows.

pub mod artifact_io;
pub mod checksum_io;
#[deny(missing_docs)]
pub mod pipeline;
pub mod tool_catalog;
#[deny(missing_docs)]
pub mod tool_logging;
pub mod verify_assets;
