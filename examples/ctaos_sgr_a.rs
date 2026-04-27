//! Mirrors `darknsb/get_NSB.py`.

use chrono::{DateTime, NaiveDateTime, Utc};
use nsb::{calculate, ComponentMask, ObservationRequest, Site, Source};
use tempoch::{Time, UTC};

fn main() -> anyhow::Result<()> {
    let ndt = NaiveDateTime::parse_from_str("2023-09-04 01:48:00", "%Y-%m-%d %H:%M:%S")?;
    let dt = DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc);
    let req = ObservationRequest {
        site: Site::Paranal,
        time: Time::<UTC>::from_chrono(dt),
        source: Source::Named("SgrA*".into()),
        components: ComponentMask::ZODIACAL | ComponentMask::STARLIGHT | ComponentMask::AIRGLOW,
    };
    let r = calculate(&req).map_err(|e| anyhow::anyhow!("{e}"))?;
    for c in &r.components {
        println!("{:>10}: integrated = {:.6e}  ph/(cm² ns sr)  | B = {:.3} S10 | V = {:.3} S10",
                 c.name, c.integrated.value(), c.b_flux_s10.value(), c.v_flux_s10.value());
    }
    println!("--------------------");
    println!("    total: {:.6e} ph/(cm² ns sr)", r.integrated.value());
    println!("       B = {:.3} mag/arcsec²", r.b_mag.value());
    println!("       V = {:.3} mag/arcsec²", r.v_mag.value());
    Ok(())
}
