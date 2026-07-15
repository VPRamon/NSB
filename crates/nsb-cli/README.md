# `nsb-cli`

Operational command-line interface for the `nsb` runtime library. The installed
binary is named `nsb`.

This crate owns:

- `point`, `window`, `sites`, and `config` commands;
- parsing of UTC timestamps, coordinates, targets, components, and radiance;
- named observatory aliases;
- stable table, JSON, and CSV output;
- operational metadata and logging.

Scientific component implementations and threshold-search algorithms remain in
the `nsb` crate. Offline catalogue and data-product generation remain in
`nsb-data-tools`.

## Commands

| Command | Purpose |
| --- | --- |
| `nsb point` | Evaluate NSB at one UTC instant and target direction |
| `nsb window` | Find UTC periods satisfying NSB and visibility constraints |
| `nsb sites list` | List known operational site aliases |
| `nsb sites show <alias>` | Inspect one alias and its coordinates |
| `nsb config init` | Print a starter TOML configuration structure |
| `nsb config validate <path>` | Validate the TOML configuration schema |

## Examples

```bash
nsb --format json point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components all

nsb --format csv window \
  --start 2026-06-18T20:00:00Z \
  --end 2026-06-19T06:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --max-nsb 0.25
```

Global options such as `--format`, `--log-level`, and `-v` precede the
subcommand.

## Production starlight

`--components starlight` uses the bundled production map when one is embedded.
A validated external override requires both files:

```bash
nsb --format json point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components starlight \
  --starlight-map /data/starlight.csv \
  --starlight-manifest /data/starlight.toml
```

Validation failure is fatal. The incomplete experimental seed is selected only
with `experimental-starlight` and is never a production fallback.

## Documentation

- [Getting started](../../docs/user-guide/getting-started.md)
- [Runtime components](../../docs/user-guide/components.md)
- [Observatory configuration](../../docs/user-guide/observatory-customization.md)
- [CLI schemas](../../docs/specifications/cli-schemas.md)
- [Logging](../../docs/specifications/logging.md)
