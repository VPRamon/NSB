//! Thin executable adapter for `train_starlight_photometry_models`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::train_starlight_photometry_models", "starting command");
    let result = nsb_data_tools::tool_services::train_starlight_photometry_models::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::train_starlight_photometry_models", "command failed: {error:#}");
    }
    result
}
