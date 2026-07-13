//! Thin executable adapter for `generate_gaia_starlight_release_inputs`.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::tool_logging::init_from_env()
        .map_err(|error| anyhow::anyhow!("failed to initialize logging: {error}"))?;
    log::info!(target: "nsb_data_tools::generate_gaia_starlight_release_inputs", "starting command");
    let result = nsb_data_tools::tool_services::generate_gaia_starlight_release_inputs::run_cli();
    if let Err(error) = &result {
        log::error!(target: "nsb_data_tools::generate_gaia_starlight_release_inputs", "command failed: {error:#}");
    }
    result
}
