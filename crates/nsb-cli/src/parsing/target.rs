// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use crate::cli::TargetArgs;
use anyhow::{bail, Result};
use nsb::{HorizontalTarget, TargetDirection, DEG};
use siderust::coordinates::{spherical::direction, transform::TransformFrame};

/// Resolve CLI target flags to a frame-preserving [`TargetDirection`].
///
/// Equatorial (`--ra`/`--dec`) and horizontal (`--alt`/`--az`) forms are
/// mutually exclusive. Both members of a coordinate pair are required.
pub fn resolve_target(args: &TargetArgs) -> Result<TargetDirection> {
    let equatorial = args.ra.is_some() || args.dec.is_some();
    let horizontal = args.alt.is_some() || args.az.is_some();
    match (equatorial, horizontal) {
        (true, true) => bail!(
            "target coordinates are mutually exclusive: use either --ra/--dec (ICRS) or --alt/--az (geometric horizontal)"
        ),
        (false, false) => bail!(
            "missing target coordinates: provide --ra and --dec (ICRS) or --alt and --az (geometric horizontal, North-clockwise azimuth)"
        ),
        (true, false) => {
            let (Some(ra), Some(dec)) = (args.ra, args.dec) else {
                bail!("ICRS targets require both --ra and --dec in degrees");
            };
            if !ra.is_finite() || !dec.is_finite() {
                bail!("--ra and --dec must be finite");
            }
            Ok(TargetDirection::Equatorial(
                direction::ICRS::new(ra * DEG, dec * DEG).to_frame(),
            ))
        }
        (false, true) => {
            let (Some(alt), Some(az)) = (args.alt, args.az) else {
                bail!("horizontal targets require both --alt and --az in degrees");
            };
            if !alt.is_finite() || !az.is_finite() {
                bail!("--alt and --az must be finite");
            }
            if !(-90.0..=90.0).contains(&alt) {
                bail!("--alt must be within [-90, 90] degrees (geometric altitude)");
            }
            Ok(TargetDirection::Horizontal(HorizontalTarget::new(
                alt * DEG,
                az * DEG,
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::{spherical::direction, transform::TransformFrame};

    #[test]
    fn cli_icrs_coordinates_preserve_the_direction_through_the_rust_target_frame() {
        let args = TargetArgs {
            ra: Some(83.6331),
            dec: Some(22.0145),
            alt: None,
            az: None,
        };
        let TargetDirection::Equatorial(target) = resolve_target(&args).unwrap() else {
            panic!("expected equatorial");
        };
        let back: direction::ICRS = target.to_frame();
        let original = direction::ICRS::new(args.ra.unwrap() * DEG, args.dec.unwrap() * DEG);
        assert!(back.angular_separation(&original).value() < 1e-10);
        assert!(
            (target.ra().value() - args.ra.unwrap()).abs() > 1e-8,
            "frame bias must be applied"
        );
    }

    #[test]
    fn cli_horizontal_coordinates_are_preserved() {
        let args = TargetArgs {
            ra: None,
            dec: None,
            alt: Some(60.0),
            az: Some(130.0),
        };
        let TargetDirection::Horizontal(target) = resolve_target(&args).unwrap() else {
            panic!("expected horizontal");
        };
        assert!((target.alt().value() - 60.0).abs() < 1e-12);
        assert!((target.az().value() - 130.0).abs() < 1e-12);
    }

    #[test]
    fn mixed_or_partial_coordinate_pairs_fail() {
        assert!(resolve_target(&TargetArgs {
            ra: Some(1.0),
            dec: None,
            alt: None,
            az: None,
        })
        .is_err());
        assert!(resolve_target(&TargetArgs {
            ra: Some(1.0),
            dec: Some(2.0),
            alt: Some(3.0),
            az: Some(4.0),
        })
        .is_err());
        assert!(resolve_target(&TargetArgs {
            ra: None,
            dec: None,
            alt: None,
            az: None,
        })
        .is_err());
        assert!(resolve_target(&TargetArgs {
            ra: None,
            dec: None,
            alt: Some(60.0),
            az: None,
        })
        .is_err());
        assert!(resolve_target(&TargetArgs {
            ra: Some(f64::NAN),
            dec: Some(1.0),
            alt: None,
            az: None,
        })
        .is_err());
        assert!(resolve_target(&TargetArgs {
            ra: None,
            dec: None,
            alt: Some(f64::INFINITY),
            az: Some(10.0),
        })
        .is_err());
        assert!(resolve_target(&TargetArgs {
            ra: None,
            dec: None,
            alt: Some(95.0),
            az: Some(10.0),
        })
        .is_err());
    }
}
