//! Thin executable adapter for `build_integrated_starlight_product`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::build_integrated_starlight_product", "starting command");
    let result = nsb_data_tools::tool_services::build_integrated_starlight_product::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::build_integrated_starlight_product", "command failed: {error:#}");
    }
    result
}
