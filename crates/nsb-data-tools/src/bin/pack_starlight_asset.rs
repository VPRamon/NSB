//! Thin executable adapter for `pack_starlight_asset`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::pack_starlight_asset", "starting command");
    let result = nsb_data_tools::tool_services::pack_starlight_asset::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::pack_starlight_asset", "command failed: {error:#}");
    }
    result
}
