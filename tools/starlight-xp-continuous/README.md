# Gaia DR3 XP continuous offline reconstruction (Phase 5)

Deterministic, checksum-pinned reconstruction of Gaia DR3 **XP continuous** spectra
with [GaiaXPy](https://gaia-dpci.github.io/GaiaXPy-website/) **2.1.4**. Outputs
are consumed by `nsb-data-tools` validation binaries; GaiaXPy is **not** linked
into the Rust runtime.

## Layout

```text
$HOME/nsb-data/starlight-gaia-release/missing-flux/xp-continuous/
  coefficients/raw/          # XP_CONTINUOUS DataLink CSV (via query_gaia / datalink)
  reconstruction/normalized/ # calibrated 336–650 nm grids (NSB normalized CSV)
  validation/                # overlap-sample bias reports
```

## Setup

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
```

## Reconstruct calibrated spectra

```bash
.venv/bin/python reconstruct_and_integrate.py \
  --coefficients-dir "$HOME/nsb-data/starlight-gaia-release/missing-flux/xp-continuous/coefficients/raw" \
  --output-dir "$HOME/nsb-data/starlight-gaia-release/missing-flux/xp-continuous/reconstruction/normalized" \
  --manifest "$HOME/nsb-data/starlight-gaia-release/missing-flux/xp-continuous/reconstruction/manifest.json"
```

The manifest records GaiaXPy version, sampling grid, input/output SHA-256, and row counts.

## Validate against XP sampled (overlap population)

Use `validate_xp_continuous_reconstruction` in `nsb-data-tools` after both
continuous reconstruction and sampled DataLink products exist for overlap sources.
