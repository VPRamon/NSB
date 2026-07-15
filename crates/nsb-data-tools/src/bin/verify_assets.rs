//! Thin executable adapter for `verify_assets`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::verify_assets", "starting command");
    let result = nsb_data_tools::tool_services::verify_assets::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::verify_assets", "command failed: {error:#}");
    }
    result
}
