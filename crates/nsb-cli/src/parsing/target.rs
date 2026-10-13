// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
use crate::cli::TargetArgs;
use nsb::{Target, DEG};
use siderust::coordinates::{spherical::direction, transform::TransformFrame};

pub fn resolve_target(args: &TargetArgs) -> Target {
    direction::ICRS::new(args.ra * DEG, args.dec * DEG).to_frame()
}

#[cfg(test)]
mod tests {
    use super::*;
    use siderust::coordinates::{spherical::direction, transform::TransformFrame};
    #[test]
    fn cli_icrs_coordinates_preserve_the_direction_through_the_rust_target_frame() {
        let args = TargetArgs {
            ra: 83.6331,
            dec: 22.0145,
        };
        let target = resolve_target(&args);
        let back: direction::ICRS = target.to_frame();
        let original = direction::ICRS::new(args.ra * DEG, args.dec * DEG);
        assert!(back.angular_separation(&original).value() < 1e-10);
        assert!(
            (target.ra().value() - args.ra).abs() > 1e-8,
            "frame bias must be applied"
        );
    }
}
