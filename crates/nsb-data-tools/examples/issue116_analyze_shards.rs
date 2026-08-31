//! Merge partition shards and print NSIDE=2 parent anomaly metrics (issue #116).

use anyhow::Result;
use nsb_data_tools::starlight::diagnostics::analyse_workspace_shards;
use std::env;
use std::path::PathBuf;

fn main() -> Result<()> {
    let workspace = PathBuf::from(
        env::args()
            .nth(1)
            .expect("usage: issue116_analyze_shards <workspace>"),
    );
    let report = analyse_workspace_shards(&workspace)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
