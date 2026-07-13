//! Stderr logger for data-product executable adapters.
//!
//! Reusable services emit through the `log` facade. Executable adapters call
//! [`init_from_env`] once, preserving stdout for stable machine-readable data.

use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::env;

static LOGGER: StderrLogger = StderrLogger;

/// Initialize the process logger from `NSB_LOG` or `RUST_LOG`.
///
/// The default level is `warn`, matching the public NSB logging contract.
pub fn init_from_env() -> Result<LevelFilter, SetLoggerError> {
    let level = env_level().unwrap_or(LevelFilter::Warn);
    log::set_logger(&LOGGER)?;
    log::set_max_level(level);
    Ok(level)
}

fn env_level() -> Option<LevelFilter> {
    env::var("NSB_LOG")
        .ok()
        .and_then(|value| parse_level_filter(&value))
        .or_else(|| {
            env::var("RUST_LOG")
                .ok()
                .and_then(|value| parse_level_filter(&value))
        })
}

fn parse_level_filter(value: &str) -> Option<LevelFilter> {
    let mut global = None;
    let mut data_tools = None;
    for directive in value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if let Some((target, level)) = directive.rsplit_once('=') {
            let target = target.trim().replace('-', "_");
            if matches!(target.as_str(), "nsb" | "nsb_data_tools")
                || target.starts_with("nsb::")
                || target.starts_with("nsb_data_tools::")
            {
                data_tools = parse_level(level);
            }
        } else {
            global = parse_level(directive);
        }
    }
    data_tools.or(global)
}

fn parse_level(value: &str) -> Option<LevelFilter> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" => Some(LevelFilter::Off),
        "error" => Some(LevelFilter::Error),
        "warn" | "warning" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        "trace" => Some(LevelFilter::Trace),
        _ => None,
    }
}

struct StderrLogger;

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!(
                "[{level} {target}] {message}",
                level = record.level(),
                target = record.target(),
                message = record.args()
            );
        }
    }

    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_and_targeted_levels() {
        assert_eq!(parse_level_filter("debug"), Some(LevelFilter::Debug));
        assert_eq!(
            parse_level_filter("warn,nsb-data-tools=trace"),
            Some(LevelFilter::Trace)
        );
        assert_eq!(
            parse_level_filter("info,other=trace,nsb_data_tools::pipeline=debug"),
            Some(LevelFilter::Debug)
        );
    }
}
