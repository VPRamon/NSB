//! Normative registry and deterministic renderer for `nsb-data` actions.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

const REGISTRY: &str = include_str!("../../tool-registry.toml");
const REFERENCE_PATH: &str = "../../docs/maintainer-guide/tools.md";

#[derive(Debug, Deserialize)]
struct Registry {
    schema_version: u32,
    actions: Vec<Action>,
}

#[derive(Debug, Deserialize)]
struct Action {
    command: String,
    owner: String,
    audience: String,
    status: String,
    purpose: String,
    inputs: String,
    outputs: String,
    resume: String,
    exit_codes: String,
    reference: String,
}

fn registry() -> Result<Registry> {
    let registry: Registry = toml::from_str(REGISTRY).context("parse tool registry")?;
    if registry.schema_version != 2 {
        bail!(
            "unsupported tool registry schema {}",
            registry.schema_version
        );
    }
    if registry.actions.is_empty() {
        bail!("tool registry has no actions");
    }
    for action in &registry.actions {
        for (name, value) in [
            ("command", &action.command),
            ("owner", &action.owner),
            ("audience", &action.audience),
            ("status", &action.status),
            ("purpose", &action.purpose),
            ("inputs", &action.inputs),
            ("outputs", &action.outputs),
            ("resume", &action.resume),
            ("exit_codes", &action.exit_codes),
            ("reference", &action.reference),
        ] {
            if value.trim().is_empty() {
                bail!("action {:?} has empty {name}", action.command);
            }
        }
        if action.status != "supported" {
            bail!(
                "action {:?} has unsupported status {:?}",
                action.command,
                action.status
            );
        }
        if action.command.contains("phase") || action.command.contains("pilot") {
            bail!("action {:?} is not a durable capability", action.command);
        }
    }
    Ok(registry)
}

/// Render the checked-in maintainer reference from the normative registry.
pub fn render_reference(write: bool) -> Result<()> {
    let rendered = render()?;
    let path = reference_path();
    let existing = fs::read_to_string(&path).unwrap_or_default();
    if existing == rendered {
        println!("tool reference is current: {}", path.display());
        return Ok(());
    }
    if !write {
        bail!("tool reference is stale; run `nsb-data maintenance render-tool-reference --write`");
    }
    fs::write(&path, rendered).with_context(|| format!("write {}", path.display()))?;
    println!("rendered tool reference: {}", path.display());
    Ok(())
}

/// Return the deterministic Markdown reference used by documentation checks.
pub fn render() -> Result<String> {
    let registry = registry()?;
    let mut output = String::from(
        "# NSB data tools\n\n> Generated from `crates/nsb-data-tools/tool-registry.toml`; do not edit manually.\n\n",
    );
    output.push_str(
        "`nsb-data` is the only supported data-product executable. Use `nsb-data --help` and a group-level `--help` to discover actions. Production actions fail closed on missing provenance, incomplete validation, checksum mismatch, schema errors, and unreconciled counts.\n\n",
    );
    output.push_str("## Action reference\n\n");
    output.push_str("| Command | Owner | Purpose | Inputs → outputs | Resume and exit condition | Reference |\n");
    output.push_str("| --- | --- | --- | --- | --- | --- |\n");
    for action in registry.actions {
        writeln!(
            output,
            "| `nsb-data {}` | `{}` ({}, {}) | {} | {} → {} | {}; {} | `{}` |",
            action.command,
            action.owner,
            action.audience,
            action.status,
            action.purpose,
            action.inputs,
            action.outputs,
            action.resume,
            action.exit_codes,
            action.reference,
        )
        .unwrap();
    }
    output.push_str(
        "\n## Maintenance policy\n\nNew actions must be durable reusable capabilities, own no duplicate scientific or persistence logic, and be registered before implementation is exposed. Remove an obsolete action and its code, tests, registry entry, and documentation in the same change; do not retain wrappers, aliases, pilot commands, phase-named modules, or ad-hoc scripts.\n",
    );
    Ok(output)
}

fn reference_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REFERENCE_PATH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_reference_matches_registry() {
        assert_eq!(
            fs::read_to_string(reference_path()).unwrap(),
            render().unwrap()
        );
    }
}
