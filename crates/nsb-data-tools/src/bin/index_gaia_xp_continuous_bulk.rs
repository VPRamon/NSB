//! Thin executable adapter for `index_gaia_xp_continuous_bulk`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::index_gaia_xp_continuous_bulk", "starting command");
    let result = nsb_data_tools::tool_services::index_gaia_xp_continuous_bulk::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::index_gaia_xp_continuous_bulk", "command failed: {error:#}");
    }
    result
}
