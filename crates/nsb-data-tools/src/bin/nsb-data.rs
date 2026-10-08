// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! NSB data-product command-line interface.

fn main() -> anyhow::Result<()> {
    nsb_data_tools::cli::run()
}
