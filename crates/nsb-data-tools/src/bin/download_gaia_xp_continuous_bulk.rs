//! Standalone XP continuous bulk downloader (USB rotating-cache compatible).

fn main() -> anyhow::Result<()> {
    nsb_data_tools::starlight::acquisition::download_gaia_xp_continuous_bulk::run_standalone()
}
