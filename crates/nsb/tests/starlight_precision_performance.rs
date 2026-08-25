use nsb::{StarlightMap, StarlightProvenance};
use siderust::coordinates::cartesian::Direction as CartesianDirection;
use siderust::coordinates::frames::Galactic;
use siderust::coordinates::spherical;
use siderust::qtty::Degrees;
use std::hint::black_box;
use std::time::{Duration, Instant};

fn fixture_map() -> StarlightMap {
    StarlightMap::from_csv_str(
        include_str!("data/starlight_fixture_map.csv"),
        StarlightProvenance::test_fixture(),
    )
    .expect("starlight fixture")
}

fn galactic_direction(lon_deg: f64, lat_deg: f64) -> CartesianDirection<Galactic> {
    spherical::Direction::<Galactic>::new(Degrees::new(lon_deg), Degrees::new(lat_deg))
        .to_cartesian()
}

#[test]
fn fixture_lookup_preserves_precision_and_throughput_budget() {
    let map = fixture_map();
    let equatorial = map.lookup(galactic_direction(45.0, 0.0));
    assert!((equatorial.integrated.value() - 4.0).abs() < 1.0e-12);
    assert!(!equatorial.s10_diagnostics_provided);

    let start = Instant::now();
    let mut accumulated = 0.0;
    for index in 0..20_000 {
        let lon = (index as f64 * 7.5) % 360.0;
        let lat = ((index as f64 * 1.25) % 180.0) - 90.0;
        accumulated += black_box(map.lookup(galactic_direction(lon, lat)))
            .integrated
            .value();
    }

    assert!(accumulated.is_finite() && accumulated > 0.0);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "20k starlight fixture lookups exceeded the CI regression budget"
    );
}
