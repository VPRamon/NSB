// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Build script: validate scientific assets and generate static Rust metadata.

#[path = "build/mod.rs"]
mod nsb_build;

fn main() {
    nsb_build::run();
}
