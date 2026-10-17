// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Bind dataset manifests to the compiled workspace, independent of launch directory.
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn main() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    // The binary links other workspace crates and embeds data through their build
    // scripts. Watching only this crate's src can retain a clean identity after
    // Cargo recompiles those dependencies. Do not watch target/ or the entire
    // checkout: generated reports are not build inputs.
    let workspace = root.parent().and_then(Path::parent).unwrap();
    for input in ["Cargo.toml", "Cargo.lock", ".cargo", "crates"] {
        let path = workspace.join(input);
        if path.exists() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    let mut commit = git(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    match git(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
    ) {
        Some(status) if status.is_empty() => {}
        // Failure must not be interpreted as evidence of a clean source tree.
        _ => commit.push_str("-dirty"),
    }
    println!("cargo:rustc-env=NSB_DATA_TOOLS_COMMIT={commit}");
    let symbolic_ref = git(&root, &["symbolic-ref", "-q", "HEAD"]);
    for name in [
        "HEAD",
        "index",
        "packed-refs",
        symbolic_ref.as_deref().unwrap_or("HEAD"),
    ] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", name]) {
            let path = PathBuf::from(path);
            let absolute = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            if absolute.exists() {
                println!("cargo:rerun-if-changed={}", absolute.display());
            }
        }
    }
}
