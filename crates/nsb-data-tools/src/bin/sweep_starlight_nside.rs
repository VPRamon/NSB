//! Thin executable adapter for `sweep_starlight_nside`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::sweep_starlight_nside", "starting command");
    let result = nsb_data_tools::tool_services::sweep_starlight_nside::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::sweep_starlight_nside", "command failed: {error:#}");
    }
    result
}
