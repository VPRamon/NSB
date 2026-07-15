# Gaia DR3 XP continuous bulk processing

> Phase 5B pilot executables were removed by issue #58 after their evidence was
> frozen. This document preserves the scientific and operational conclusions of
> those runs, but removed command names are not current usage instructions. See
> [data-tool migration](DATA_TOOL_MIGRATION.md) and the normative
> [`nsb-data-tools` registry](../crates/nsb-data-tools/tool-registry.toml).

## Official product

Gaia DR3 exposes XP continuous spectra as **basis-function coefficients**, not
pre-sampled flux arrays. The authoritative bulk delivery is:

| Field | Value |
| --- | --- |
| Table | `xp_continuous_mean_spectrum` |
| Bulk URL | https://cdn.gea.esac.esa.int/Gaia/gdr3/Spectroscopy/xp_continuous_mean_spectrum/ |
| Checksum manifest | `_MD5SUM.txt` (MD5, official ESA) |
| Partition files | 3,386 `XpContinuousMeanSpectrum_*.csv.gz` |
| Estimated compressed size | ~3.3 TiB |
| TAP access | **Not available** — Massive Data / CDN only |
| License / credit | Gaia DR3 — https://gea.esac.esa.int/archive/documentation/credits.html |

Primary documentation:

- [xp_continuous_mean_spectrum datamodel](https://gea.esac.esa.int/archive/documentation/GDR3/Gaia_archive/chap_datamodel/sec_dm_spectroscopic_tables/ssec_dm_xp_continuous_mean_spectrum.html)
- [Gaia archive programmatic access](https://www.cosmos.esa.int/web/gaia-users/archive/programmatic-access)

Each bulk row contains `source_id`, BP/RP coefficient arrays, coefficient errors
and quality metadata. The migration reference reconstructs spectra offline with
pinned GaiaXPy 2.1.4 and integrates the inclusive **336–650 nm** photon-flux
band. GaiaXPy is temporary reference infrastructure tracked for replacement by
pure Rust in #61.

The 336–650 nm result is **not** labelled as the integrated 300–650 nm product
until the independently validated 300–336 nm correction is applied.

## Mandatory production strategy

Individual DataLink requests for 184,729,270 continuous-only sources are
forbidden. The required capability pipeline is:

```text
official bulk coefficient files
  → resumable MD5-validated download
  → canonical versioned coefficient records
  → validated spectral reconstruction
  → integrate 336–650 nm
  → apply the frozen quality policy
  → accumulate deterministic HEALPix contributions
  → compact partition checkpoints and exact reconciliation
  → discard transient coefficient batches only after durable verification
```

Persistent outputs are manifests, checksums, partition accounting, rejected-row
evidence, HEALPix accumulators and reproducible commands—not a 184M-row CSV.

## Current supported tooling

| Capability | Current owner |
| --- | --- |
| Official bulk download and checksum verification | `download_gaia_xp_continuous_bulk` |
| Bulk partition/source index | `index_gaia_xp_continuous_bulk` |
| Canonical coefficient normalization | `normalize_xp_continuous_coefficients` and `gaia_xp_continuous_canonical.rs` |
| Reconstruction parity validation | `validate_xp_continuous_reconstruction` |
| HEALPix accumulator implementation | `gaia_xp_continuous_healpix.rs` |
| Integrated candidate construction | `build_integrated_starlight_product` |
| Tool purpose and maturity | `crates/nsb-data-tools/tool-registry.toml` |

There is deliberately no supported command named after Phase 5 or Phase 5B.
A production bulk orchestrator must be introduced only after #60 defines the
state machine, compact checkpoints, transactional cleanup and crash/resume test
contract. It must use a durable capability name and reusable library services.

## Historical Phase 5B multifile pilot (2026-07-11)

**PHASE 5B MULTIFILE PILOT PASSED — READY FOR SCIENTIFIC POLICY**

Two downloaded prefixes (~2.75 GiB compressed) were used:

```text
XpContinuousMeanSpectrum_000000-003111.csv.gz
XpContinuousMeanSpectrum_003112-005263.csv.gz
```

| Gate | Recorded result |
| --- | --- |
| Same adapter/schema on both prefixes | PASS — identical headers, `bp_n_parameters=55`, 1485 correlations |
| Multifile streaming (20,000 rows / 19,995 valid) | PASS — peak RSS ~50 MiB |
| Per-file reconciliation | PASS — all exclusions logged (`non_positive_flux`) |
| Resume (10k uninterrupted vs 5k+5k) | PASS |
| Merge order independence (1→2 vs 2→1) | PASS — checksum `a875a1b0d0c302c…` |

These results remain historical evidence about the parser, reconstruction and
accumulator. They are not evidence that the removed pilot architecture is safe
for the full 184.7M-source production run. Production scalability and recovery
must be demonstrated under #60.
