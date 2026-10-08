// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Production Starlight dataset pipeline.

pub mod bright_stars;
pub mod conditions;
pub mod config;
pub mod diagnostics;
pub mod healpix;
pub mod healpix_topology;
pub mod licensing;
pub mod map;
pub mod pack;
pub mod photometric;
mod pipeline;
pub mod promotion;
pub mod selection;
pub mod sources;
pub mod uncertainty;
pub mod uv;
pub mod validation;
mod worker;
pub mod xp;

pub(crate) use pipeline::PIPELINE;
