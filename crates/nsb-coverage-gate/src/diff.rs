use std::collections::BTreeMap;
use std::process::Command;
use thiserror::Error;

/// One changed line in the new version of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangedLine {
    pub path: String,
    pub line: u32,
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
        if raw.starts_with('+') {
            changed.push(ChangedLine {
                path: path.to_string(),
                line: *line,
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

/// Group changed lines by path.
pub fn group_by_path(changed: &[ChangedLine]) -> BTreeMap<String, Vec<u32>> {
    let mut grouped = BTreeMap::new();
    for line in changed {
        grouped
            .entry(line.path.clone())
            .or_insert_with(Vec::new)
            .push(line.line);
    }
    for lines in grouped.values_mut() {
        lines.sort_unstable();
        lines.dedup();
    }
    grouped
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
                },
                ChangedLine {
                    path: "crates/nsb/src/lib.rs".into(),
                    line: 12,
                },
            ]
        );
    }
}
