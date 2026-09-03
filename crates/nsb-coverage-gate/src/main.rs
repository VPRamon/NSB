use nsb_coverage_gate::{run, CheckKind, CheckStatus, GateOptions};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let kind = match args[0].as_str() {
        "overall" => CheckKind::Overall,
        "diff" => CheckKind::Diff,
        other => {
            eprintln!("unknown command {other}; expected overall or diff");
            print_help();
            return ExitCode::from(2);
        }
    };

    match parse_options(&args[1..]) {
        Ok(options) => match run(kind, &options, &mut std::io::stdout()) {
            Ok(outcome) => match outcome.status {
                CheckStatus::Pass => ExitCode::SUCCESS,
                CheckStatus::Fail => ExitCode::from(1),
            },
            Err(error) => {
                eprintln!("{error}");
                ExitCode::from(2)
            }
        },
        Err(error) => {
            eprintln!("{error}");
            print_help();
            ExitCode::from(2)
        }
    }
}

fn parse_options(args: &[String]) -> Result<GateOptions, String> {
    let mut options = GateOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--policy" => {
                options.policy_path = Some(require_value(args, &mut index, "--policy")?.into());
            }
            "--report" => {
                options.report_path = require_value(args, &mut index, "--report")?.into();
            }
            "--workspace-lines-floor" => {
                options.workspace_lines_floor = Some(parse_f64(require_value(
                    args,
                    &mut index,
                    "--workspace-lines-floor",
                )?)?);
            }
            "--nsb-lines-floor" => {
                options.nsb_lines_floor = Some(parse_f64(require_value(
                    args,
                    &mut index,
                    "--nsb-lines-floor",
                )?)?);
            }
            "--diff-lines-floor" => {
                options.diff_lines_floor = Some(parse_f64(require_value(
                    args,
                    &mut index,
                    "--diff-lines-floor",
                )?)?);
            }
            "--base" => {
                options.base = Some(require_value(args, &mut index, "--base")?.to_string());
            }
            "--diff-file" => {
                options.diff_file = Some(require_value(args, &mut index, "--diff-file")?.into());
            }
            "--artifact-hint" => {
                options.artifact_hint =
                    Some(require_value(args, &mut index, "--artifact-hint")?.to_string());
            }
            other => return Err(format!("unknown argument {other}")),
        }
        index += 1;
    }
    Ok(options)
}

fn require_value<'a>(args: &'a [String], index: &mut usize, flag: &str) -> Result<&'a str, String> {
    *index += 1;
    args.get(*index)
        .map(String::as_str)
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_f64(value: &str) -> Result<f64, String> {
    value.parse().map_err(|_| format!("invalid number {value}"))
}

fn print_help() {
    eprintln!(
        "\
nsb-coverage-gate overall|diff [options]

Options:
  --policy PATH
  --report PATH
  --workspace-lines-floor PCT
  --nsb-lines-floor PCT
  --diff-lines-floor PCT
  --base GIT_REF
  --diff-file PATH
  --artifact-hint TEXT
"
    );
}
