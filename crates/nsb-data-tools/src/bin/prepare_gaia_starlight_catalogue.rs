//! Thin executable adapter for `prepare_gaia_starlight_catalogue`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::prepare_gaia_starlight_catalogue", "starting command");
    let result = nsb_data_tools::tool_services::prepare_gaia_starlight_catalogue::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::prepare_gaia_starlight_catalogue", "command failed: {error:#}");
    }
    result
}
