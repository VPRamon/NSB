//! Thin executable adapter for `validate_xp_continuous_reconstruction`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::validate_xp_continuous_reconstruction", "starting command");
    let result = nsb_data_tools::tool_services::validate_xp_continuous_reconstruction::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::validate_xp_continuous_reconstruction", "command failed: {error:#}");
    }
    result
}
