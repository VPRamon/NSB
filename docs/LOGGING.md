# Logging

NSB uses the Rust `log` facade for diagnostic events. Reusable library code emits events only; it does not initialize global logging. The command-line binary initializes a stderr logger at the application boundary.

## Levels

Supported levels are:

- `error`: command-aborting failures and unrecoverable errors.
- `warn`: recoverable problems, suspicious inputs, degraded behavior, skipped paths, or non-production choices.
- `info`: high-level lifecycle and progress messages.
- `debug`: developer diagnostics such as resolved inputs, model configuration, search phases, and timing.
- `trace`: fine-grained internals, especially threshold-search refinement decisions.
- `off`: disable logging.

Default CLI behavior is `warn`, so normal successful table, JSON, and CSV output remains readable and stable.

## CLI controls

Use an explicit level:

```sh
nsb --log-level info point --time 2026-06-18T23:00:00Z --site CTAO-S --ra 83.0 --dec 22.0
```

Use verbosity shorthands:

```sh
nsb -v window ...      # info
nsb -vv window ...     # debug
nsb -vvv window ...    # trace
```

Use environment variables when the flag is not provided:

```sh
NSB_LOG=debug nsb point ...
RUST_LOG=nsb=trace nsb window ...
```

`NSB_LOG` takes precedence over `RUST_LOG`. `RUST_LOG` may be a plain level such as `debug` or a directive such as `nsb=debug`.

## Output contract

Logs are emitted to stderr. Machine-readable command output remains on stdout, so JSON/CSV consumers can opt into logging without corrupting stdout streams.

## Maintainer guidance

- Initialize logging only in binaries or applications.
- Library code may emit `log::{error,warn,info,debug,trace}` events but must not call `set_logger` or otherwise configure global logging.
- Use `info` for stable phase/progress milestones, not per-record spam.
- Use `debug` for resolved configuration, selected model parameters, paths, and timing.
- Use `trace` for high-volume internals such as per-interval search refinement.
- Never log credentials, tokens, or large raw external-service payloads.
