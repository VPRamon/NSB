# Starlight map validation

Status: Current contract for `nsb-data starlight map validate`.
Audience: Maintainers generating starlight map candidates and reviewers reading
validation reports.
Scope: Validator inputs, output report fields, gates, independent-reference
schema, failure modes, and packing handoff.
Non-goals: This document does not validate a specific Gaia release product and
does not admit caller-supplied external maps; see
[Validated external starlight manifest](external-manifest.md).

`nsb-data starlight map validate` is the release harness for generated starlight maps. It
turns a map candidate, generation diagnostics, and independent regional
reference ranges into a machine-readable validation report. That report is then
consumed by `nsb-data starlight release pack-asset`.

## Starlight Document Path

```text
science-requirements.md
  -> map-generation.md
  -> map-validation.md
  -> external-manifest.md or bundled asset review
  -> validation.md and model-maturity.md
```

Use [Starlight science requirements](science-requirements.md) for
the release criteria. Use
[Starlight data-product pipeline](map-generation.md) for generation
commands. This file defines how the generated map is checked.

## Purpose

The validator answers four release questions:

| Question | Evidence source |
| --- | --- |
| Can the map be parsed as an NSB starlight map? | `--input` |
| Are pixel values physically admissible for this contract? | map pixels |
| Is the Galactic longitude seam free of obvious wrap artifacts? | deterministic seam diagnostic |
| Does the map match reviewed regional reference ranges? | structured `--reference` JSON |

The tool derives pass/fail values itself. It does not trust a raw
`production_ready` or `independent_comparison_pass` boolean supplied by an
external file.

## Inputs

| Input | Required | Role |
| --- | --- | --- |
| `--input <CSV>` | Yes | Generated Galactic starlight map. |
| `--diagnostics <JSON>` | No | Build diagnostics from `nsb-data starlight map build`; the path is recorded in the report. |
| `--reference <JSON>` | Required for production | Structured independent regional reference ranges. |
| `--output <JSON>` | Yes | Validation report path. |
| `--require-independent-comparison` | Required for release validation | Fails if the structured reference is absent or any region fails. |

Maintainer release run:

```bash
cargo run --locked -p nsb-data-tools --bin nsb-data -- starlight map validate \
  --input target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.csv \
  --diagnostics target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.diagnostics.json \
  --reference "$STARLIGHT_INDEPENDENT_VALIDATION_REFERENCE" \
  --output target/starlight-release/starlight_gaia_dr3_xp_330_650nm_nside128_v1.validation.json \
  --require-independent-comparison
```

This command is maintainer-only because it depends on local release artifacts
and reviewed reference data.

## Output Report

The report is JSON with schema version 1. The important release fields are:

| Field | Meaning |
| --- | --- |
| `finite_nonnegative_pass` | Every radiance and S10 field is finite and nonnegative. |
| `plane_pole_pass` | Integrated Galactic plane average is at least the high-latitude pole average. |
| `longitude_wrap_pass` | The seam diagnostic passes. |
| `longitude_wrap_metric` | Maximum of the seam/control median jump and seam spike ratio. |
| `longitude_wrap_threshold` | Current threshold used for `longitude_wrap_metric`. |
| `seam_pixel_count` | Number of finite nonnegative pixels in the seam band. |
| `control_pixel_count` | Number of pixels in the adjacent control bands. |
| `independent_comparison_pass` | Every structured independent region passed. |
| `independent_comparison` | Per-region observed mean, median, range, sample count, and pass flag. |
| `production_ready` | True only when every release gate passes. |
| `limitations` | Human-readable blockers for failed or missing gates. |

Example shape:

```json
{
  "schema_version": 1,
  "map_path": "target/starlight-release/starlight.csv",
  "diagnostics_path": "target/starlight-release/starlight.diagnostics.json",
  "dataset": "Gaia",
  "pixel_count": 196608,
  "finite_nonnegative_pass": true,
  "plane_pole_pass": true,
  "longitude_wrap_pass": true,
  "longitude_wrap_metric": 1.2,
  "longitude_wrap_threshold": 10.0,
  "seam_pixel_count": 16384,
  "control_pixel_count": 32768,
  "independent_comparison_pass": true,
  "production_ready": true,
  "limitations": []
}
```

## Validation Gates

`production_ready=true` requires all gates to pass:

| Gate | Failure meaning |
| --- | --- |
| Finite and nonnegative values | The map contains invalid radiance or diagnostic values. |
| Plane/pole contrast | The map lacks the expected broad Galactic contrast. |
| Longitude wrap | The `l = 0/360 deg` seam has an invalid value, spike, or discontinuity. |
| Independent regional comparison | At least one reviewed reference region is missing, malformed, empty, or out of range. |

Tiny CI fixtures can pass deterministic construction checks, but they cannot
establish production readiness without structured independent comparison
evidence.

## Longitude-Wrap Semantics

The seam diagnostic compares integrated radiance near the Galactic longitude
wrap with adjacent control bands:

```text
seam:   l <= 15 deg or l >= 345 deg
control: 15 deg <= l <= 45 deg or 315 deg <= l <= 345 deg
```

The reported metric is:

```text
max(abs(seam_median - control_median) / control_scale,
    seam_max / control_scale)
```

where `control_scale` is derived from the control median, control maximum, and a
small finite floor. The current threshold is `10.0`. The gate fails for
non-finite seam values, negative seam values, extreme spikes, or large
seam/control discontinuities. If a fixture is too small to contain seam and
control samples, the diagnostic is deterministic but not sufficient for
production readiness.

## Independent Reference Schema

The reference file declares reviewed regional expectations. It does not declare
the result.

```json
{
  "schema_version": 1,
  "production_use": true,
  "units": "ph cm-2 ns-1 sr-1",
  "regions": [
    {
      "name": "north_galactic_pole",
      "frame": "galactic",
      "l_deg": 0.0,
      "b_deg": 90.0,
      "aperture_deg": 10.0,
      "expected_min": 0.0,
      "expected_max": 100.0,
      "source": "reviewed independent reference"
    }
  ]
}
```

For each region, the validator selects map pixels inside the Galactic aperture,
computes observed mean and median integrated radiance, and checks:

```text
expected_min <= observed_mean <= expected_max
```

Schema rules:

| Field | Rule |
| --- | --- |
| `schema_version` | Must be `1`. |
| `production_use` | Must be `true` for release validation. |
| `units` | Must be `ph cm-2 ns-1 sr-1`. |
| `regions` | Must contain at least one region. |
| `frame` | Must be `galactic`. |
| `l_deg` | Must be finite and in `[0, 360)`. |
| `b_deg` | Must be finite and in `[-90, 90]`. |
| `aperture_deg` | Must be finite and in `(0, 90]`. |
| `expected_min`, `expected_max` | Must be finite, nonnegative, and ordered. |
| `name`, `source` | Must be nonempty and must not contain placeholder text. |

Placeholder terms such as `todo`, `placeholder`, `unknown`, `pending`, and
`unreviewed` fail the schema.

## Failure Modes

| Failure | Typical cause |
| --- | --- |
| Map parse error | Unsupported header, bad CSV, or invalid numeric field. |
| `finite_nonnegative_pass=false` | Negative or non-finite map values. |
| `plane_pole_pass=false` | Plane/pole contrast absent or insufficient. |
| `longitude_wrap_pass=false` | Seam discontinuity, seam spike, or invalid seam value. |
| Reference parse error | Blind boolean file, malformed JSON, unsupported schema, or missing fields. |
| Region comparison failure | No samples in aperture or observed mean outside the reviewed range. |
| `production_ready=false` | One or more gates failed or independent comparison was not supplied. |

## Relationship To Packing

`nsb-data starlight release pack-asset --production` consumes the validation report and fails if
`production_ready` is not true. `--candidate` may produce a review artifact, but
candidate output must remain clearly labelled and outside `ComponentMask::ALL`.

Production packing also depends on the generation diagnostics, map checksum,
manifest metadata, license review, and maturity review described in
[Starlight data-product pipeline](map-generation.md) and
[Model maturity](../../specifications/model-maturity.md).
