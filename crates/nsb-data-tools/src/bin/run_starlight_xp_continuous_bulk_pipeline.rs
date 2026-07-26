//! Gaia DR3 XP continuous bulk production pipeline (preflight + production loop).

fn main() -> anyhow::Result<()> {
    nsb_data_tools::starlight::xp_continuous::run_bulk_pipeline::run_standalone()
}
