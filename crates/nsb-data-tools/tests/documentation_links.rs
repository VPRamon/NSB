//! Repository documentation link contract.
//!
//! Ensures tracked Markdown files do not point at missing local paths after
//! cleanup. External http(s) links and bare `#anchors` are ignored.

use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn is_skipped_dir(name: &str) -> bool {
    matches!(
        name,
        ".git" | "target" | ".venv" | "__pycache__" | ".pytest_cache" | "coverage_html" | "local"
    )
}

fn collect_markdown_files(root: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(root).unwrap_or_else(|error| {
        panic!("read {}: {error}", root.display());
    });
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().and_then(|v| v.to_str()).unwrap_or("");
            if !is_skipped_dir(name) {
                collect_markdown_files(&path, out);
            }
        } else if path.extension().and_then(|v| v.to_str()) == Some("md") {
            out.push(path);
        }
    }
}

fn strip_code_fences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn extract_markdown_links(text: &str) -> Vec<String> {
    let mut links = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'[' {
            // Find matching ](
            if let Some(close) = text[index..].find("](") {
                let after = index + close + 2;
                if let Some(end) = text[after..].find(')') {
                    let target = text[after..after + end].trim();
                    // Drop optional title after whitespace.
                    let target = target.split_whitespace().next().unwrap_or(target);
                    if !target.is_empty() {
                        links.push(target.trim_matches(|c| c == '<' || c == '>').to_string());
                    }
                    index = after + end + 1;
                    continue;
                }
            }
        }
        index += 1;
    }
    links
}

fn is_external_or_special(target: &str) -> bool {
    target.starts_with('#')
        || target.starts_with("http://")
        || target.starts_with("https://")
        || target.starts_with("mailto:")
        || target.starts_with("doi:")
        || target.starts_with("irc:")
}

fn resolve_local_target(source: &Path, target: &str) -> Option<PathBuf> {
    let without_anchor = target.split('#').next().unwrap_or(target);
    if without_anchor.is_empty() {
        return None;
    }
    if is_external_or_special(without_anchor) {
        return None;
    }
    let candidate = if Path::new(without_anchor).is_absolute() {
        PathBuf::from(without_anchor)
    } else {
        source
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(without_anchor)
    };
    Some(candidate)
}

#[test]
fn markdown_local_links_resolve() {
    let root = repo_root();
    let mut files = Vec::new();
    collect_markdown_files(&root, &mut files);
    assert!(
        !files.is_empty(),
        "expected markdown files under {}",
        root.display()
    );

    let mut broken = Vec::new();
    for file in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        let body = strip_code_fences(&text);
        for target in extract_markdown_links(&body) {
            let Some(path) = resolve_local_target(file, &target) else {
                continue;
            };
            if !path.exists() {
                broken.push(format!(
                    "{} -> {} (resolved {})",
                    file.strip_prefix(&root).unwrap_or(file).display(),
                    target,
                    path.display()
                ));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "broken local markdown links:\n{}",
        broken.join("\n")
    );
}
