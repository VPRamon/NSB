use crate::cli::TargetArgs;
use nsb::{Target, DEG};

pub fn resolve_target(args: &TargetArgs) -> Target {
    Target::new(args.ra * DEG, args.dec * DEG)
}
