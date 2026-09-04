use std::collections::BTreeMap;
use std::process::Command;
use thiserror::Error;

/// One changed line in the new version of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedLine {
    pub path: String,
    pub line: u32,
    /// Text of the added line without the leading `+` from the unified diff.
    pub text: String,
}

/// Diff/git failures.
#[derive(Debug, Error)]
pub enum DiffError {
    #[error("failed to invoke git: {0}")]
    Git(String),
    #[error("invalid unified diff: {0}")]
    Parse(String),
    #[error("failed to read diff file {path}: {source}")]
    Io {
        path: std::path::PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Collect added/changed lines from `git diff -U0 <merge-base>...HEAD`.
pub fn changed_lines_from_git(base: &str) -> Result<Vec<ChangedLine>, DiffError> {
    let merge_base = git_stdout(&["merge-base", base, "HEAD"])?;
    let merge_base = merge_base.trim();
    if merge_base.is_empty() {
        return Err(DiffError::Git(format!(
            "git merge-base {base} HEAD produced an empty SHA"
        )));
    }
    let range = format!("{merge_base}...HEAD");
    let diff = git_stdout(&[
        "diff",
        "-U0",
        "--no-color",
        "--find-renames",
        &range,
        "--",
        "*.rs",
    ])?;
    parse_unified_diff(&diff)
}

fn git_stdout(args: &[&str]) -> Result<String, DiffError> {
    let output = Command::new("git")
        .args(args)
        .output()
        .map_err(|error| DiffError::Git(error.to_string()))?;
    if !output.status.success() {
        return Err(DiffError::Git(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    String::from_utf8(output.stdout).map_err(|error| DiffError::Git(error.to_string()))
}

/// Parse `git diff -U0` output into new-file line numbers.
pub fn parse_unified_diff(diff: &str) -> Result<Vec<ChangedLine>, DiffError> {
    let mut current_path: Option<String> = None;
    let mut new_line: Option<u32> = None;
    let mut changed = Vec::new();
    for raw in diff.lines() {
        if let Some(path) = raw.strip_prefix("+++ b/") {
            current_path = Some(path.trim().to_string());
            continue;
        }
        if raw.starts_with("+++ /dev/null") {
            current_path = None;
            continue;
        }
        if raw.starts_with("@@ ") {
            new_line = Some(parse_hunk_new_start(raw)?);
            continue;
        }
        if raw.starts_with("diff ") || raw.starts_with("index ") || raw.starts_with("--- ") {
            continue;
        }
        let Some(path) = current_path.as_deref() else {
            continue;
        };
        let Some(line) = new_line.as_mut() else {
            continue;
        };
        if let Some(added) = raw.strip_prefix('+') {
            changed.push(ChangedLine {
                path: path.to_string(),
                line: *line,
                text: added.to_string(),
            });
            *line += 1;
        } else if raw.starts_with('-') {
            // Deleted lines exist only in the old file.
        } else if raw.starts_with('\\') {
            // "\ No newline at end of file"
        } else {
            *line += 1;
        }
    }
    Ok(changed)
}

fn parse_hunk_new_start(header: &str) -> Result<u32, DiffError> {
    // @@ -old[,len] +new[,len] @@
    let plus = header
        .split_whitespace()
        .find(|part| part.starts_with('+'))
        .ok_or_else(|| DiffError::Parse(format!("missing new-file range in {header}")))?;
    let range = plus.trim_start_matches('+');
    let start = range
        .split(',')
        .next()
        .ok_or_else(|| DiffError::Parse(format!("invalid new-file range {plus}")))?;
    start
        .parse()
        .map_err(|_| DiffError::Parse(format!("invalid new-file line {start}")))
}

/// Group changed lines by path (stable order; duplicate line numbers kept once).
pub fn group_by_path(changed: &[ChangedLine]) -> BTreeMap<String, Vec<&ChangedLine>> {
    let mut grouped: BTreeMap<String, Vec<&ChangedLine>> = BTreeMap::new();
    for line in changed {
        grouped.entry(line.path.clone()).or_default().push(line);
    }
    for lines in grouped.values_mut() {
        lines.sort_by_key(|line| line.line);
        lines.dedup_by_key(|line| line.line);
    }
    grouped
}

/// Heuristic: lines LLVM coverage typically does not instrument as executable.
///
/// Used only when a production file is absent from LCOV so declaration-only
/// edits (mods, re-exports, docs, attributes) do not fail the diff gate, while
/// genuinely executable production edits remain fail-closed.
pub fn is_likely_non_instrumentable_rust_line(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    if t.starts_with("//") {
        return true;
    }
    if t.starts_with("#[") || t.starts_with("#![") {
        return true;
    }
    if t.starts_with("/*") || t == "*/" {
        return true;
    }
    // Block-comment / doc continuations (` * ...`), not raw pointers.
    if t.starts_with('*') && !t.starts_with("*mut") && !t.starts_with("*const") {
        return true;
    }
    if matches!(t, "{" | "}" | "};" | ");" | "," | ";") {
        return true;
    }

    let without_vis = strip_visibility(t);
    if without_vis.starts_with("mod ")
        || without_vis.starts_with("use ")
        || without_vis.starts_with("extern crate ")
    {
        return true;
    }
    if without_vis.starts_with("type ")
        || without_vis.starts_with("struct ")
        || without_vis.starts_with("enum ")
        || without_vis.starts_with("union ")
        || without_vis.starts_with("trait ")
        || without_vis.starts_with("impl ")
        || without_vis.starts_with("const ")
        || without_vis.starts_with("static ")
    {
        return true;
    }
    // Trait / extern method signatures without a body.
    if without_vis.starts_with("fn ") && t.ends_with(';') && !t.contains('{') {
        return true;
    }
    // Enum unit/tuple variants and struct fields (no executable statements).
    if looks_like_item_member(t) {
        return true;
    }
    // Multi-line `use` / `pub use` path lists (`Foo,`, `path::Bar,`, …).
    if looks_like_use_list_continuation(t) {
        return true;
    }
    false
}

/// Continuation lines of multi-line `use` / re-export lists.
fn looks_like_use_list_continuation(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    if !(t.ends_with(',')
        || t.ends_with('{')
        || t.ends_with('}')
        || t.ends_with("};")
        || t.ends_with(';'))
    {
        return false;
    }
    let stripped = t
        .trim_end_matches(';')
        .trim_end_matches('}')
        .trim_end_matches('{')
        .trim_end_matches(',')
        .trim();
    if stripped.is_empty() {
        return true;
    }
    stripped.split(',').all(|part| {
        let part = part.trim();
        if part.is_empty() {
            return true;
        }
        let name = part
            .split_once(" as ")
            .map(|(path, _)| path.trim())
            .unwrap_or(part);
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '{' | '}' | '*'))
            && !name.starts_with("fn")
            && !name.starts_with("let")
            && !name.starts_with("return")
    })
}

fn strip_visibility(text: &str) -> &str {
    let text = text
        .strip_prefix("pub(crate) ")
        .or_else(|| text.strip_prefix("pub(super) "))
        .or_else(|| text.strip_prefix("pub(self) "))
        .unwrap_or(text);
    let text = if let Some(rest) = text.strip_prefix("pub(") {
        // pub(in path) …
        if let Some(idx) = rest.find(") ") {
            &rest[idx + 2..]
        } else {
            text
        }
    } else {
        text
    };
    text.strip_prefix("pub ").unwrap_or(text)
}

fn looks_like_item_member(text: &str) -> bool {
    let t = text.trim().trim_end_matches(',').trim();
    if t.is_empty() {
        return true;
    }
    // `Ident` or `Ident(…)` enum variants.
    if t.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && t.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '(' || c == ')' || c == ',')
    {
        return true;
    }
    // `name: Type` / `pub name: Type` field lines without executable bodies.
    let body = strip_visibility(t);
    body.contains(':')
        && !body.contains("::")
        && !body.contains('=')
        && !body.contains('{')
        && !body.starts_with("fn ")
        && !body.starts_with("if ")
        && !body.starts_with("match ")
        && !body.starts_with("for ")
        && !body.starts_with("while ")
        && !body.starts_with("return ")
        && !body.starts_with("let ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_added_and_context_lines() {
        let diff = "\
diff --git a/crates/nsb/src/lib.rs b/crates/nsb/src/lib.rs
--- a/crates/nsb/src/lib.rs
+++ b/crates/nsb/src/lib.rs
@@ -10,0 +11,2 @@
+pub fn added() {}
+pub fn also() {}
";
        let changed = parse_unified_diff(diff).unwrap();
        assert_eq!(
            changed,
            vec![
                ChangedLine {
                    path: "crates/nsb/src/lib.rs".into(),
                    line: 11,
                    text: "pub fn added() {}".into(),
                },
                ChangedLine {
                    path: "crates/nsb/src/lib.rs".into(),
                    line: 12,
                    text: "pub fn also() {}".into(),
                },
            ]
        );
    }

    #[test]
    fn declaration_lines_are_non_instrumentable() {
        assert!(is_likely_non_instrumentable_rust_line("pub mod continuum;"));
        assert!(is_likely_non_instrumentable_rust_line(
            "pub use continuum::Airglow;"
        ));
        assert!(is_likely_non_instrumentable_rust_line("//! docs"));
        assert!(is_likely_non_instrumentable_rust_line("#[non_exhaustive]"));
        assert!(is_likely_non_instrumentable_rust_line(
            "pub enum NsbError {"
        ));
        assert!(is_likely_non_instrumentable_rust_line("GenericClearSky,"));
        assert!(is_likely_non_instrumentable_rust_line("    file: String,"));
        assert!(is_likely_non_instrumentable_rust_line("{"));
        assert!(is_likely_non_instrumentable_rust_line("}"));
        assert!(is_likely_non_instrumentable_rust_line(
            "    DEFAULT_VAN_RHIJN_EMISSION_HEIGHT_KM, VERTICAL_EMISSION_PROFILE_SCHEMA_VERSION,"
        ));
        assert!(is_likely_non_instrumentable_rust_line(
            "    AtmosphericConditions, Jones2013Spectral, KrisciunasSchaefer1991, DEFAULT_K_EXT,"
        ));
        assert!(is_likely_non_instrumentable_rust_line(
            "pub const SIDERUST_VERSION: &str = \"0.11.1\";"
        ));
        assert!(is_likely_non_instrumentable_rust_line(
            "pub const SIDERUST_SOURCE: &str = \"crates.io:siderust:0.11.1\";"
        ));
        assert!(is_likely_non_instrumentable_rust_line(
            "pub use components::moonlight::{"
        ));
        assert!(!is_likely_non_instrumentable_rust_line("return value;"));
        assert!(!is_likely_non_instrumentable_rust_line("fn missing() {}"));
        assert!(!is_likely_non_instrumentable_rust_line("let x = 1;"));
    }
}
