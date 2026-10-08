// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Reproducible Rust pipelines for NSB runtime datasets.
#![deny(unsafe_code)]

extern crate self as nsb_data_tools;

pub mod cli;
pub mod dataset;
pub mod platform;
pub mod solar;
pub mod starlight;
