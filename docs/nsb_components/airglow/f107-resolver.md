# F10.7 solar-activity resolver

Status: Runtime + data-tools contract for issue #109.
Audience: Users and developers who need date-aware F10.7 for Airglow.
Scope: Quantity convention, store schema, precedence, online update,
offline resolution, provenance, reproducibility, and relationship to the
#108 Paranal planning-proxy Airglow baseline.

## Goal

Given a UTC date, NSB deterministically chooses and reports the best available
F10.7 value from a **pinned local dataset**. Network acquisition is explicit and
lives only in `nsb-data-tools`. Normal Airglow evaluation remains offline and
reproducible.

```text
nsb-data-tools (network acquisition)
    -> versioned F10.7 store (checksum/snapshot)
    -> nsb offline resolver
    -> Airglow
```

## Quantity and units

- **Quantity**: monthly-averaged Penticton/DRAO 10.7 cm solar radio flux, matching
  Noll et al. 2012 / ESO SkyCalc `msolflux` (“Monthly Averaged Solar Flux”).
- **Unit**: solar flux unit (**sfu**), \(1\,\mathrm{sfu}=10^{-22}\,\mathrm{W\,m^{-2}\,Hz^{-1}}\).
- **Convention id**: `penticton-f107-sfu-as-reported-by-noaa-swpc`.

Noll et al. obtained **monthly** S10.7 averages for each spectrum because the
atmosphere responds with a delay of weeks; diurnal F10.7 is a different
scientific variable from the one used to fit the Airglow solar-activity slope.
NSB therefore resolves a **monthly-mean** quantity for Airglow and never feeds a
raw daily observation or daily forecast value into that correction.

NSB consumes F10.7 **as republished by NOAA/NWS SWPC** machine-readable
products. Product identity is retained on every record so users can audit which
SWPC product supplied a value.

### Observed vs 1-AU-adjusted (documented ambiguity)

DRAO publishes both Earth-observed and 1-AU-adjusted F10.7 series. SWPC products
used here do not always label that distinction in the payload. NSB therefore:

1. does **not** invent a conversion between observed and adjusted series;
2. records provider + product + source locator for each value;
3. treats SWPC-reported sfu values as the operational index class expected by the
   Noll/SkyCalc-derived Airglow solar-activity correction.

If a future audited product clearly distinguishes adjusted-to-1-AU values and
Airglow’s historical fit is shown to require one variant, a follow-up can add an
explicit conversion or product filter. Until then, inventing a correction would
be less honest than preserving product provenance.

## Relationship to #108 (Paranal planning proxy)

Issue #108 established that the bundled Airglow continuum is a Paranal /
Noll / SkyCalc / FORS1-derived **generic/planning proxy**, not a globally
calibrated site model. That contract is unchanged:

- arbitrary-location evaluation remains supported as an explicit proxy;
- generic vs site-calibrated remain distinguishable;
- **a measured F10.7 value does not make Airglow site-calibrated**.

F10.7 resolution only replaces the previous single neutralizing default for
automatic evaluations. Continuum geometry, extinction, and calibration maturity
are out of scope here (#110 / #114 / #38 are intentionally not implemented).

## Precedence (tested)

1. Explicit caller override (`with_f10_7` / `--solar-radio-flux-sfu`), validated
   finite and positive
2. **Monthly** measured observation covering the requested UTC date
   (`cadence=monthly`, e.g. `observed-solar-cycle-indices`)
3. Monthly-compatible forecast:
   - calendar-month mean of available SWPC **45-day** daily forecasts for the
     requested month (`product=45-day-forecast-monthly-mean`) when any day in
     that month is present — an operational aggregation approximating
     `msolflux`, **not** a measured monthly mean (incomplete months use the mean
     of available days only);
   - else monthly `predicted-solar-cycle` covering that month
4. Documented climatological fallback (`climatology_sfu` = Noll/SkyCalc
   neutralizing reference ≈ 129.207 sfu / `DEFAULT_SOLAR_RADIO_FLUX`)
5. Legacy neutralizing constant only via `SolarActivitySource::LegacyDefault`

Raw **daily** observations and daily 45-day rows may remain in the store for
diagnostics/CLI inspection but are never selected as the Airglow F10.7 input.

Historical dates never silently prefer forecasts or climatology when a monthly
observation exists. Forecasts are never labelled as observations. Climatology is
never labelled as forecast or measurement.

`retrieved_at_utc` and `forecast_issued_at_utc` are distinct: products without an
issuance field (e.g. `predicted-solar-cycle`) leave issuance unset rather than
fabricating it from download time.

## Runtime API (network-free)

```rust
use nsb::{resolve_f107, NsbModelConfig, SolarActivitySource, SolarFluxUnits};
use tempoch::{Time, UTC};

// Automatic: bundled offline store + climatology
let config = NsbModelConfig::generic_clear_sky();

// Explicit override
let config = config.with_f10_7(SolarFluxUnits::new(130.0));

// Or resolve directly
let resolved = resolve_f107(time, &SolarActivitySource::Automatic)?;
```

`Airglow::compute` and the evaluator perform **no network I/O**. Online updates
must materialize a local dataset first.

## Store schema (`nsb-f107-store-v1`)

JSON asset with `schema_version = 1`, `dataset_id`, `snapshot_id`, `convention`,
`climatology_sfu`, and `records[]` carrying `date`, `value_sfu`, `kind`
(`observed|forecast|climatology|explicit`), `provider`, `product`, temporal
fields, optional uncertainty/range, and source locator.

Validation rejects NaN/inf, non-positive values, invalid dates, contradictory
temporal fields, and conflicting duplicates without deterministic product
precedence. Checksums pin snapshot bytes.

The bundled offline snapshot is registered in `crates/nsb/data/manifest.toml`
as `f107_store.json` (`runtime_embedded = true`). Scientific provenance is owned
by the manifest; Rust only pins the embedded-byte checksum.

### Bundled asset validation

Embedded bytes must match the manifest SHA-256. CI loads and resolves against the
bundled store without network access.

## Online update (`nsb-data-tools`)

```text
nsb-data solar f107 update [--store PATH] [--fixture-dir DIR]
nsb-data solar f107 status [--store PATH]
nsb-data solar f107 resolve --time <UTC> [--store PATH]
nsb-data solar f107 import <file> [--store PATH]
nsb-data solar f107 verify <asset> [--sha256 DIGEST]
```

Primary provider: **NOAA/NWS SWPC**.

| Product | Role |
|---------|------|
| `daily-solar-indices.txt` | Recent daily observed F10.7 |
| `45-day-forecast.json` | Short-range daily forecast |
| `predicted-solar-cycle.json` | Longer-range monthly predictions + ranges |
| `observed-solar-cycle-indices.json` | Monthly observed indices (optional/best-effort online) |

Updates validate before activation, write atomically, retain previous snapshots
under `snapshots/`, and report dataset/snapshot/checksum. Network errors must
not corrupt a valid store. CI uses `--fixture-dir` with pinned fixtures under
`crates/nsb-data-tools/tests/fixtures/swpc/` (no live provider dependency).

## Offline / local / explicit paths

- **Offline Automatic**: bundled `f107_store.json`.
- **Local dataset**: `--f107-store PATH` / `SolarActivitySource::Dataset`.
- **Import**: `nsb-data solar f107 import`.
- **Explicit**: `--solar-radio-flux-sfu` / `with_f10_7`.

## Reproducibility

`requested_time` + resolver policy + selected dataset `snapshot_id`/`checksum` +
selected record ⇒ the same F10.7 later. Updates create new snapshots; they do not
mutate prior snapshot files required for pinned runs.

## Forecast limitations

Short-range and monthly forecasts are planning aids with intrinsic uncertainty.
Metadata exposes issuance time when the upstream product provides it, product
identity, and ranges when available. The 45-day → calendar-month mean is an
operational approximation of `msolflux` for near-future planning; it must not be
confused with a measured monthly mean. Forecast/climatology inputs are
distinguishable from measured observations and must not be presented as
measurements.

CLI / JSON point results report the F10.7 **actually applied** under
`model.solar_radio_flux_sfu` (from resolved Airglow metadata) and under
`components[].metadata.solar_activity`. Window JSON omits
`model.solar_radio_flux_sfu` for Automatic/Dataset sources because samples can
differ across the window.

## Examples

Historical (prefer monthly observation):

```bash
nsb-data solar f107 resolve --time 2026-08-01T12:00:00Z
```

Near future (45-day calendar-month mean when no monthly observation):

```bash
nsb-data solar f107 resolve --time 2026-09-05T12:00:00Z
```

Explicit override:

```bash
nsb point --time 2026-08-01T12:00:00Z ... --solar-radio-flux-sfu 130
```

Update local dataset from fixtures (CI-safe):

```bash
nsb-data solar f107 update --store /tmp/f107.json \
  --fixture-dir crates/nsb-data-tools/tests/fixtures/swpc
```

## Why no runtime network

Hidden fetches would make Airglow non-deterministic, break offline observatory
use, and couple scientific results to transient provider availability. Acquisition
is therefore an explicit data-tools operation; runtime only reads pinned bytes.
