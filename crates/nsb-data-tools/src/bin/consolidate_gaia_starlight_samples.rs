//! Thin executable adapter for `consolidate_gaia_starlight_samples`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::consolidate_gaia_starlight_samples", "starting command");
    let result = nsb_data_tools::tool_services::consolidate_gaia_starlight_samples::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::consolidate_gaia_starlight_samples", "command failed: {error:#}");
    }
    result
}
