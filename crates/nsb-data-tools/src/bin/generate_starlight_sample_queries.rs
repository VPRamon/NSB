//! Thin executable adapter for `generate_starlight_sample_queries`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::generate_starlight_sample_queries", "starting command");
    let result = nsb_data_tools::tool_services::generate_starlight_sample_queries::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::generate_starlight_sample_queries", "command failed: {error:#}");
    }
    result
}
