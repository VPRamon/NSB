# Reacquisition recipes for restricted Starlight inputs

These inputs must **not** be committed under `crates/nsb/data/`. The production
pipeline reacquires them from the pinned URLs and checksums in
`crates/nsb-data-tools/config/starlight-production-300-650.ladon.toml`.

## Gaia DR3 GaiaSource

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  dataset starlight update \
  --config crates/nsb-data-tools/config/starlight-production-300-650.ladon.toml
```

Inventory: `https://cdn.gea.esac.esa.int/Gaia/gdr3/gaia_source/_MD5SUM.txt`
(SHA-256 `9ec782f9c83b29885924c7d47bba18d70c86b8cbefbc408b19090b6a76e8e369`).
Licence: CC BY-NC 3.0 IGO. Distribution class: `download_only`.

## Gaia DR3 XP continuous mean spectra

Same `dataset starlight update` command. Inventory SHA-256
`f23df1ffb45b19fc3f34d6f37791179cef1ebec6c5b9fd613a488b3be580fccd`.
Distribution class: `download_only`.

## CALSPEC

Training lives outside the repository (`/home/valles/nsb-calibration` /
BeeGFS `starlight-calibration`). The UV artifact pin is the only production
input; raw STIS FITS files are `download_only`.

## Cantat-Gaudin selection function

Zenodo 8063930 file `allsky_M10_hpx7.hdf5` (see selection artifact). Not
redistributed; completeness tables are derived into the selection artifact
consumed by workers.

## Independent validation references

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- \
  dataset starlight validation acquire \
  --references docs/nsb_components/starlight/validation/references-v1.toml \
  --workspace <path>
```

References without `acquisition_url` remain `pending-acquisition` until a
human supplies `--source id=/path`.
