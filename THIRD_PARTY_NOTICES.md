# Third-party notices

NSB source is licensed under AGPL-3.0-only (see [`LICENSE`](LICENSE) and
[`README.md`](README.md#licensing)). Third-party dependencies, scientific data,
and reference material retain their own licence and attribution obligations.
Distributors must review and comply with the complete terms that apply to the
artifacts and dependency versions they ship. This file consolidates attribution
for third-party data and reference material used by NSB components.
Component-specific detail, licence classification, distribution status, and
checksums live next to each component; see the links below.

This file records attribution and licence facts. It does not itself
authorize redistribution of any listed third-party or derived artifact.
Where a component's licensing folder records a pending human decision, that
decision remains the sole authorization gate.

## Starlight (integrated starlight component)

Full artifact inventory, licence classification, and attribution wording:

- [`docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml`](docs/nsb_components/starlight/licensing/artifact-inventory-v1.toml)
- [`docs/nsb_components/starlight/licensing/ATTRIBUTION.md`](docs/nsb_components/starlight/licensing/ATTRIBUTION.md)

Summary of third-party sources:

- **Gaia DR3** (ESA/DPAC) — GaiaSource and XP continuous mean spectrum bulk
  products. Licence: Gaia data licence (CC BY-NC 3.0 IGO),
  <https://www.cosmos.esa.int/web/gaia-users/license>.
- **Cantat-Gaudin et al. (2023)** empirical Gaia DR3 selection function.
  Licence: CC-BY-4.0, DOI `10.1051/0004-6361/202244784`.
- **STScI CALSPEC** spectrophotometric standard-star atlas, used as an
  offline UV-correction training reference only. Public HST calibration
  data; attribution requested.
- **GaiaXPy** — cited only as historical independent reference evidence for
  continuous-XP reconstruction; not redistributed.

## Other bundled runtime assets

The moonlight and airglow/solar bundled snapshots (`airglow_cont.dat`,
`solar_spectrum.dat`, `mie_m15s1.dat`, `sscatcor_m15s1.dat`) are historical
imports associated with the ESO Sky Model lineage and have incomplete upstream
licence records; see `crates/nsb/data/manifest.toml` for the current, explicitly
flagged state of each. This blocks their calibrated-production promotion until a
reviewed source and licence are supplied; it is out of scope for this Starlight
redistribution package (#88).

## Reporting a missing or incorrect notice

Open an issue referencing the specific artifact id from the relevant
`artifact-inventory-v1.toml` entry.
