use nsb::{StarlightMap, StarlightProvenance};
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

#[test]
fn fixture_lookup_preserves_precision_and_throughput_budget() {
    let map = fixture_map();
    let mid = map.lookup(Degrees::new(45.0), Degrees::new(45.0));
    assert!((mid.integrated.value() - 3.0).abs() < 1.0e-12);
    assert!((mid.b_flux_s10.value() - 30.0).abs() < 1.0e-12);
    assert!((mid.v_flux_s10.value() - 15.0).abs() < 1.0e-12);

    let start = Instant::now();
    let mut accumulated = 0.0;
    for index in 0..20_000 {
        let lon = Degrees::new((index as f64 * 7.5) % 360.0);
        let lat = Degrees::new(((index as f64 * 1.25) % 180.0) - 90.0);
        accumulated += black_box(map.lookup(lon, lat)).integrated.value();
    }

    assert!(accumulated.is_finite() && accumulated > 0.0);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "20k starlight fixture lookups exceeded the CI regression budget"
    );
}
