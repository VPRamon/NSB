// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use nsb::components::starlight::StarlightProduct;
use nsb::{ComponentMask, NsbEvaluator, PointQuery, Target, DEG};
use siderust::coordinates::transform::TransformFrame;
#[test]
fn approved_production_has_all_four_components_offline() {
    assert!(StarlightProduct::bundled_production_available());
    assert_eq!(ComponentMask::ALL.bits(), 15);
    assert_eq!(ComponentMask::DEFAULT.bits(), 15);
    let time = tempoch::Time::<tempoch::UTC>::from_chrono(
        chrono::DateTime::parse_from_rfc3339("2023-09-04T01:48:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc),
    );
    let target: Target =
        siderust::coordinates::spherical::direction::ICRS::new(266.41683 * DEG, -29.00781 * DEG)
            .to_frame();
    let result = NsbEvaluator::new()
        .unwrap()
        .evaluate(&PointQuery::new(
            siderust::coordinates::centers::Geodetic::new_raw(
                siderust::qtty::Degrees::new(-70.31634444444444),
                siderust::qtty::Degrees::new(-24.683427777777776),
                siderust::qtty::Meters::new(2184.6),
            ),
            time,
            target,
        ))
        .unwrap();
    assert_eq!(result.components.len(), 4);
    assert!(
        result
            .components
            .iter()
            .find(|c| c.name == "starlight")
            .unwrap()
            .integrated
            .value()
            > 0.0
    );
    assert_eq!(
        result.integrated.value(),
        result
            .components
            .iter()
            .map(|c| c.integrated.value())
            .sum::<f64>()
    );
    let values = serde_json::json!({"total": result.integrated.value(),
        "components": result.components.iter().map(|c| (c.name.to_string(), c.integrated.value())).collect::<std::collections::BTreeMap<_, _>>()});
    if let Ok(path) = std::env::var("NSB_PRODUCTION_RESULT") {
        std::fs::write(path, serde_json::to_vec(&values).unwrap()).unwrap();
    }
}
