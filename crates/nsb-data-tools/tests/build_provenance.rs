// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Exercise Cargo's real incremental build tracking in an isolated Git workspace.
use std::path::Path;
use std::process::Command;

fn run(root: &Path, program: &str, args: &[&str]) -> String {
    let output = Command::new(program)
        .current_dir(root)
        .args(args)
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("CARGO_TARGET_DIR")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{program} {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn build(root: &Path) -> String {
    run(
        root,
        env!("CARGO"),
        &["run", "--quiet", "--offline", "-p", "provenance-fixture"],
    )
}

#[test]
fn incremental_provenance_tracks_workspace_inputs_and_worktree_git_state() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for dir in ["crates/nsb/src", "crates/nsb-data-tools/src", ".cargo"] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    std::fs::write(root.join(".gitignore"), "/target/\n").unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/nsb\", \"crates/nsb-data-tools\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/nsb/Cargo.toml"),
        "[package]\nname = \"nsb-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("crates/nsb/src/lib.rs"),
        "pub fn value() -> u8 { 1 }\n",
    )
    .unwrap();
    std::fs::write(root.join("crates/nsb-data-tools/Cargo.toml"), "[package]\nname = \"provenance-fixture\"\nversion = \"0.0.0\"\nedition = \"2021\"\n[dependencies]\nnsb-fixture = { path = \"../nsb\" }\n").unwrap();
    std::fs::write(root.join("crates/nsb-data-tools/src/main.rs"), "fn main() { println!(\"{}:{}\", env!(\"NSB_DATA_TOOLS_COMMIT\"), nsb_fixture::value()); }\n").unwrap();
    std::fs::write(
        root.join("crates/nsb-data-tools/build.rs"),
        include_str!("../build.rs"),
    )
    .unwrap();
    run(root, "git", &["init", "-q"]);
    run(
        root,
        "git",
        &["config", "user.email", "fixture@example.invalid"],
    );
    run(root, "git", &["config", "user.name", "Provenance fixture"]);
    run(root, env!("CARGO"), &["generate-lockfile", "--offline"]);
    run(root, "git", &["add", "."]);
    run(root, "git", &["commit", "-qm", "fixture"]);
    let head = run(root, "git", &["rev-parse", "HEAD"]);
    assert_eq!(build(root), format!("{head}:1"));
    // A dependency rebuild must refresh provenance without cleaning target/.
    std::fs::write(
        root.join("crates/nsb/src/lib.rs"),
        "pub fn value() -> u8 { 2 }\n",
    )
    .unwrap();
    assert_eq!(build(root), format!("{head}-dirty:2"));
    run(root, "git", &["restore", "crates/nsb/src/lib.rs"]);
    assert_eq!(build(root), format!("{head}:1"));
    let source = root.join("crates/nsb-data-tools/src/main.rs");
    let original = std::fs::read_to_string(&source).unwrap();
    std::fs::write(&source, format!("{original}\n// dirty tool source\n")).unwrap();
    assert_eq!(build(root), format!("{head}-dirty:1"));
    run(
        root,
        "git",
        &["restore", "crates/nsb-data-tools/src/main.rs"],
    );
    std::fs::write(root.join(".cargo/config.toml"), "[build]\njobs = 1\n").unwrap();
    assert_eq!(build(root), format!("{head}-dirty:1"));
    std::fs::remove_file(root.join(".cargo/config.toml")).unwrap();
    run(root, "git", &["checkout", "--detach", "-q"]);
    assert_eq!(build(root), format!("{head}:1"));
    // A linked worktree has a .git file and its refs live in the common dir.
    let linked = root.join("linked-worktree");
    run(
        root,
        "git",
        &[
            "worktree",
            "add",
            "--detach",
            "-q",
            linked.to_str().unwrap(),
            "HEAD",
        ],
    );
    assert_eq!(build(&linked), format!("{head}:1"));
    std::fs::write(
        linked.join("crates/nsb/src/lib.rs"),
        "pub fn value() -> u8 { 3 }\n",
    )
    .unwrap();
    assert_eq!(build(&linked), format!("{head}-dirty:3"));
}
