//! Stream one XP continuous bulk partition through Rust calibrate + HEALPix.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::starlight::xp_continuous::run_mini_pilot::run_standalone()
}
