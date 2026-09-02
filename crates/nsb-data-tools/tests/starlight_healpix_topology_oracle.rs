//! healpy cross-checks for reference HEALPix topology (ring2nest, neighbours).

use nsb_data_tools::starlight::healpix::{nested_neighbours, ring_to_nested};
use nsb_data_tools::starlight::healpix_topology::reference_nested2ring;

const ORACLE_CASES: &str = include_str!("fixtures/healpix_topology_oracle.json");

#[derive(serde::Deserialize)]
struct OracleCase {
    nside: u32,
    nest: u64,
    ring: u64,
    neighbours: Vec<u32>,
}

#[test]
fn reference_topology_matches_healpy_oracle_vectors() -> anyhow::Result<()> {
    let cases: Vec<OracleCase> = serde_json::from_str(ORACLE_CASES)?;
    for case in cases {
        assert_eq!(
            reference_nested2ring(case.nside, case.nest)?,
            case.ring,
            "nest2ring mismatch nside={} nest={}",
            case.nside,
            case.nest
        );
        assert_eq!(
            ring_to_nested(case.nside, case.ring)?,
            case.nest,
            "ring2nest mismatch nside={} ring={}",
            case.nside,
            case.ring
        );
        let neighbours = nested_neighbours(case.nside, case.nest as u32)?;
        assert_eq!(
            neighbours, case.neighbours,
            "neighbour mismatch nside={} nest={}",
            case.nside, case.nest
        );
    }
    Ok(())
}
