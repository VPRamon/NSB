use clap::ValueEnum;
use log::{LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::env;

static LOGGER: StderrLogger = StderrLogger;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl From<LogLevel> for LevelFilter {
    fn from(value: LogLevel) -> Self {
        match value {
            LogLevel::Off => Self::Off,
            LogLevel::Error => Self::Error,
            LogLevel::Warn => Self::Warn,
            LogLevel::Info => Self::Info,
            LogLevel::Debug => Self::Debug,
            LogLevel::Trace => Self::Trace,
        }
    }
}

pub fn init(cli_level: Option<LogLevel>, verbosity: u8) -> Result<LevelFilter, SetLoggerError> {
    let level = resolve_level(cli_level, verbosity);
    log::set_logger(&LOGGER)?;
    log::set_max_level(level);
    Ok(level)
}

fn resolve_level(cli_level: Option<LogLevel>, verbosity: u8) -> LevelFilter {
    if let Some(level) = cli_level {
        return level.into();
    }

    match verbosity {
        0 => env_level().unwrap_or(LevelFilter::Warn),
        1 => LevelFilter::Info,
        2 => LevelFilter::Debug,
        _ => LevelFilter::Trace,
    }
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
    let mut nsb_specific = None;

    for directive in value.split(',') {
        let directive = directive.trim();
        if directive.is_empty() {
            continue;
        }
        if let Some((target, level)) = directive.rsplit_once('=') {
            let parsed = parse_level(level.trim())?;
            let target = target.trim().replace('-', "_");
            if target == "nsb" || target == "nsb_cli" || target.starts_with("nsb::") {
                nsb_specific = Some(parsed);
            }
        } else {
            global = parse_level(directive);
        }
    }

    nsb_specific.or(global)
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
    fn parses_plain_levels() {
        assert_eq!(parse_level_filter("debug"), Some(LevelFilter::Debug));
        assert_eq!(parse_level_filter("warn"), Some(LevelFilter::Warn));
        assert_eq!(parse_level_filter("off"), Some(LevelFilter::Off));
    }

    #[test]
    fn prefers_nsb_directive_over_global_rust_log() {
        assert_eq!(
            parse_level_filter("info,other=trace,nsb=debug"),
            Some(LevelFilter::Debug)
        );
        assert_eq!(
            parse_level_filter("warn,nsb-cli=trace"),
            Some(LevelFilter::Trace)
        );
    }
}
