//! Identify lines that belong only to `#[cfg(test)]` items.
//!
//! The diff-coverage gate must not treat executable code inside conventional
//! inline test modules as changed *production* lines. This scanner walks a
//! Rust source file and returns 1-based line numbers that are compiled only
//! under `cfg(test)`.

use std::collections::BTreeSet;

/// 1-based line numbers that belong to file-level `#[cfg(test)]` items.
///
/// Recognizes attribute groups that include an exact `#[cfg(test)]`, followed by
/// a module (`mod name { ... }` / `mod name;`) or another braced/semicolon item
/// at the same brace depth. Nested braces are tracked so production code after
/// the item is not excluded. Files that merely *mention* `#[cfg(test)]` inside
/// strings or comments are unaffected.
pub fn cfg_test_line_numbers(source: &str) -> BTreeSet<u32> {
    let lines: Vec<&str> = source.lines().collect();
    let mut excluded = BTreeSet::new();
    let mut depth = 0i32;
    let mut i = 0usize;

    while i < lines.len() {
        let line_no = (i as u32) + 1;
        let trimmed = strip_line_comment(lines[i]).trim();

        if depth == 0 && is_attribute_line(trimmed) {
            let attr_start = i;
            let mut j = i;
            let mut saw_cfg_test = false;
            while j < lines.len() {
                let t = strip_line_comment(lines[j]).trim();
                if t.is_empty() {
                    j += 1;
                    continue;
                }
                if !is_attribute_line(t) {
                    break;
                }
                if is_cfg_test_attribute(t) {
                    saw_cfg_test = true;
                }
                j += 1;
            }
            while j < lines.len() && strip_line_comment(lines[j]).trim().is_empty() {
                j += 1;
            }
            if saw_cfg_test && j < lines.len() {
                let item = strip_line_comment(lines[j]).trim();
                if let Some(end) = item_end_line(&lines, j, depth) {
                    for line in (attr_start as u32 + 1)..=end {
                        excluded.insert(line);
                    }
                    // Advance past the item; brace depth is unchanged at the
                    // file level because the item's braces open and close
                    // within [j, end].
                    i = end as usize;
                    continue;
                }
                // Unclosed item: fail closed by not excluding further lines.
                let _ = (item, line_no);
            }
            i += 1;
            continue;
        }

        depth = update_depth(depth, lines[i]);
        i += 1;
    }

    excluded
}

/// True when `line` (1-based) is inside a `#[cfg(test)]` item in `source`.
pub fn is_cfg_test_line(source: &str, line: u32) -> bool {
    cfg_test_line_numbers(source).contains(&line)
}

fn item_end_line(lines: &[&str], item_start: usize, outer_depth: i32) -> Option<u32> {
    let first = strip_line_comment(lines[item_start]).trim();
    if first.is_empty() {
        return None;
    }
    // `mod name;` / `fn name();` style — single line, no body.
    if !first.contains('{') && first.ends_with(';') {
        return Some((item_start as u32) + 1);
    }
    if !first.contains('{') && !looks_like_item_header(first) {
        return None;
    }

    let mut depth = outer_depth;
    let mut seen_open = false;
    for (idx, raw) in lines.iter().enumerate().skip(item_start) {
        let before = depth;
        depth = update_depth(depth, raw);
        if depth > before {
            seen_open = true;
        }
        if seen_open && depth == outer_depth {
            return Some((idx as u32) + 1);
        }
        // Header-only line without `{` yet (signature wrap): keep scanning.
        if !seen_open && idx > item_start + 32 {
            return None;
        }
    }
    None
}

fn looks_like_item_header(trimmed: &str) -> bool {
    let body = strip_visibility(trimmed);
    body.starts_with("mod ")
        || body.starts_with("fn ")
        || body.starts_with("struct ")
        || body.starts_with("enum ")
        || body.starts_with("union ")
        || body.starts_with("trait ")
        || body.starts_with("impl ")
        || body.starts_with("type ")
        || body.starts_with("const ")
        || body.starts_with("static ")
        || body.starts_with("async fn ")
        || body.starts_with("unsafe fn ")
        || body.starts_with("unsafe impl ")
}

fn is_attribute_line(trimmed: &str) -> bool {
    trimmed.starts_with("#[") && trimmed.contains(']')
}

/// Exact `#[cfg(test)]`, allowing internal whitespace.
fn is_cfg_test_attribute(trimmed: &str) -> bool {
    let t = trimmed.trim();
    let Some(inner) = t.strip_prefix("#[").and_then(|rest| rest.strip_suffix(']')) else {
        return false;
    };
    let inner = inner.trim();
    let Some(cfg) = inner.strip_prefix("cfg") else {
        return false;
    };
    let cfg = cfg.trim_start();
    let Some(pred) = cfg
        .strip_prefix('(')
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    pred.trim() == "test"
}

fn strip_visibility(text: &str) -> &str {
    let text = text
        .strip_prefix("pub(crate) ")
        .or_else(|| text.strip_prefix("pub(super) "))
        .or_else(|| text.strip_prefix("pub(self) "))
        .unwrap_or(text);
    let text = if let Some(rest) = text.strip_prefix("pub(") {
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

fn strip_line_comment(line: &str) -> &str {
    // Prefer not to treat `http://` as a comment. Only strip `//` outside quotes.
    let mut in_string = false;
    let mut chars = line.char_indices().peekable();
    while let Some((idx, ch)) = chars.next() {
        if ch == '"' && !in_string {
            in_string = true;
            continue;
        }
        if ch == '"' && in_string {
            in_string = false;
            continue;
        }
        if in_string {
            if ch == '\\' {
                chars.next();
            }
            continue;
        }
        if ch == '/' && matches!(chars.peek(), Some((_, '/'))) {
            return &line[..idx];
        }
    }
    line
}

fn update_depth(mut depth: i32, line: &str) -> i32 {
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    let mut in_char = false;
    let mut in_block_comment = false;
    while let Some(ch) = chars.next() {
        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some('/')) {
                chars.next();
                in_block_comment = false;
            }
            continue;
        }
        if in_string {
            if ch == '\\' {
                chars.next();
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if in_char {
            if ch == '\\' {
                chars.next();
            } else if ch == '\'' {
                in_char = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '\'' => in_char = true,
            '/' if matches!(chars.peek(), Some('*')) => {
                chars.next();
                in_block_comment = true;
            }
            '/' if matches!(chars.peek(), Some('/')) => break,
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth.max(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_before_and_after_inline_tests_remain() {
        let source = "\
fn production_before() {
    let x = 1;
}

#[cfg(test)]
mod tests {
    #[test]
    fn foo() {
        assert_eq!(1, 1);
    }
}

fn production_after() {
    let y = 2;
}
";
        let excluded = cfg_test_line_numbers(source);
        assert!(!excluded.contains(&1), "fn production_before header");
        assert!(!excluded.contains(&2), "body before tests");
        assert!(excluded.contains(&5), "#[cfg(test)]");
        assert!(excluded.contains(&6), "mod tests");
        assert!(excluded.contains(&9), "assert inside tests");
        assert!(!excluded.contains(&13), "fn production_after");
        assert!(!excluded.contains(&14), "body after tests");
    }

    #[test]
    fn differently_named_cfg_test_module_is_excluded() {
        let source = "\
pub fn keep() { 1 }

#[cfg(test)]
mod regression {
    #[test]
    fn case() {
        assert!(true);
    }
}
";
        let excluded = cfg_test_line_numbers(source);
        assert!(!excluded.contains(&1));
        assert!(excluded.contains(&3));
        assert!(excluded.contains(&4));
        assert!(excluded.contains(&7));
    }

    #[test]
    fn nested_braces_do_not_swallow_following_production() {
        let source = "\
fn outer() {
    if true {
        let z = 0;
    }
}

#[cfg(test)]
mod tests {
    fn nested() {
        if true {
            let _ = 1;
        }
    }
}

fn later() {
    let ok = true;
}
";
        let excluded = cfg_test_line_numbers(source);
        assert!(!excluded.contains(&3));
        assert!(excluded.contains(&11));
        assert!(!excluded.contains(&17));
        assert!(!excluded.contains(&18));
    }

    #[test]
    fn cfg_not_test_is_not_excluded() {
        let source = "\
#[cfg(not(test))]
fn production_only() {
    let x = 1;
}
";
        assert!(cfg_test_line_numbers(source).is_empty());
    }

    #[test]
    fn string_mention_of_cfg_test_is_ignored() {
        let source = "\
fn demo() {
    let s = \"#[cfg(test)]\";
}
";
        assert!(cfg_test_line_numbers(source).is_empty());
    }
}
