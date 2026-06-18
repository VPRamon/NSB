# nsb-cli

Operational command-line interface for the `nsb` library crate.

This crate owns user-facing concerns that intentionally do not belong in the
runtime library:

- named-site aliases such as `CTAO-S` and `CTAO-N`;
- command-line parsing;
- output formatting (`table`, `json`, `csv`);
- operational defaults for point and planning queries.

The dependency direction is:

```text
nsb-cli -> nsb
nsb     -> never depends on nsb-cli
```

## Commands

```bash
nsb point \
  --time 2026-06-18T23:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --components zodiacal,airglow,moon \
  --format json
```

```bash
nsb window \
  --start 2026-06-18T20:00:00Z \
  --end 2026-06-19T06:00:00Z \
  --site CTAO-S \
  --ra 83.6331 \
  --dec 22.0145 \
  --min-nsb 0.02 \
  --max-nsb 0.25 \
  --target-altitude-min 30 \
  --format csv
```

```bash
nsb sites list
nsb sites show CTAO-S --format json
```

The starlight map generator is intentionally not part of this CLI. It should be
implemented later as an offline data-tool crate.
