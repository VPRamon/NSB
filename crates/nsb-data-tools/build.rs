// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Bind dataset manifests to the compiled source, independent of launch directory.
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
    let mut commit = git(&root, &["rev-parse", "HEAD"]).unwrap_or_else(|| "unknown".into());
    if git(
        &root,
        &["status", "--porcelain", "--untracked-files=normal"],
    )
    .is_some_and(|s| !s.is_empty())
    {
        commit.push_str("-dirty");
    }
    println!("cargo:rustc-env=NSB_DATA_TOOLS_COMMIT={commit}");
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");
    for name in [
        "HEAD",
        git(&root, &["symbolic-ref", "-q", "HEAD"])
            .as_deref()
            .unwrap_or("HEAD"),
    ] {
        if let Some(path) = git(&root, &["rev-parse", "--git-path", name]) {
            let path = PathBuf::from(path);
            let absolute = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            println!("cargo:rerun-if-changed={}", absolute.display());
        }
    }
}
