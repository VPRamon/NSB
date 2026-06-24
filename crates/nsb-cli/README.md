# nsb-cli

Operational CLI for `nsb`. It owns named-site aliases, parsing, stable output
schemas, and operational metadata.

`--components all` is exactly `ComponentMask::ALL`: zodiacal, airglow, and
moonlight. The incomplete seed requires the explicit component name
`experimental-starlight`.

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

JSON includes model/component maturity, provenance, validated domain,
uncertainty, diagnostic-band meaning, exact versions, and asset checksums. CSV
v1 schemas are documented in `docs/CLI_SCHEMAS.md`.

Scientific data generation belongs to `nsb-data-tools`, never this operational
binary.
