use nsb_public_api_gate::{
    discover_repo, run_check, run_write, CheckOptions, GateStatus, DEFAULT_NIGHTLY,
    DEFAULT_PUBLIC_API_VERSION, DEFAULT_SNAPSHOT_PATH,
};
use std::env;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return ExitCode::SUCCESS;
    }

    let mut options = CheckOptions {
        repo: discover_repo(&env::current_dir().unwrap_or_else(|_| ".".into())).unwrap_or_else(
            |error| {
                eprintln!("{error}");
                std::process::exit(2);
            },
        ),
        write: false,
        base: None,
        base_explicit: false,
    };

    if let Ok(base) = env::var("NSB_PUBLIC_API_BASE") {
        options.base = Some(base);
        options.base_explicit = true;
    }

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--write" => options.write = true,
            "--base" => {
                index += 1;
                let Some(value) = args.get(index) else {
                    eprintln!("--base requires a revision");
                    print_help();
                    return ExitCode::from(2);
                };
                options.base = Some(value.clone());
                options.base_explicit = true;
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_help();
                return ExitCode::from(2);
            }
        }
        index += 1;
    }

    let result = if options.write {
        run_write(&options)
    } else {
        run_check(&options)
    };

    match result {
        Ok(outcome) => {
            println!("{}", outcome.message);
            match outcome.status {
                GateStatus::Pass => ExitCode::SUCCESS,
                GateStatus::Fail => ExitCode::from(1),
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}

fn print_help() {
    println!(
        "\
Usage: nsb-public-api-gate [--write] [--base REV]

  --write     Regenerate {DEFAULT_SNAPSHOT_PATH} from the working tree.
  --base REV  Historical revision for SemVer diff (overrides NSB_PUBLIC_API_BASE).

Environment:
  NSB_PUBLIC_API_TOOL_VERSION       cargo-public-api pin (default: {DEFAULT_PUBLIC_API_VERSION})
  NSB_PUBLIC_API_RUSTDOC_TOOLCHAIN  nightly toolchain for rustdoc JSON (default: {DEFAULT_NIGHTLY})
  NSB_PUBLIC_API_BASE               historical diff base revision (CI should set this)

CI must pass an explicit base:
  - pull_request: PR base SHA
  - push: github.event.before (commit before the push)

Local runs without --base / NSB_PUBLIC_API_BASE fall back to merge-base with
origin/main, then HEAD~1. Direct-push protection must not rely on
origin/main == HEAD inference."
    );
}
