//! Build script: validate scientific assets and generate static Rust metadata.

#[path = "build_support/mod.rs"]
mod nsb_build;

fn main() {
    nsb_build::run();
}
