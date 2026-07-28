# Starlight dataset validation

`nsb-data dataset starlight validate --config <run.toml>` validates exactly one
configured canonical map and `merge_report.json`. Publication recomputes
checksums and rejects missing, extra, changed, malformed, or mismatched
artifacts.

Candidate schema `nsb-healpix-starlight-candidate-v3` requires:

```text
map_type=healpix
coordinate_frame=galactic
ordering=nested
representation=sparse
omitted_pixel_semantics=zero_flux_and_source_counts
flux_quantity=integrated_per_pixel
flux_unit=ph_m-2_s-1
derivation=canonical_gaia_source_accumulation
source_count_semantics=exact_source_membership
```

The nside header and filename come from `canonical_nside`. The canonical CSV is
sparse: rows must be strictly increasing by pixel ID, and an omitted HEALPix
pixel means zero integrated flux, zero admitted sources, and zero excluded
sources. Validation rejects missing, unknown, duplicate, or incompatible
headers; malformed, duplicate, or out-of-order rows; out-of-range pixels;
negative or non-finite flux; empty maps; and row counts larger than
`12 * nside^2`.

The current candidate remains `runtime_embedded = false`. Any future runtime
admission of this schema must materialize an omitted pixel as the same explicit
zero-flux, zero-source-count value rather than treating omission as missing or
unknown data.

Report schema v4 declares one `canonical_map`. Validation independently reads
the CSV using compensated summation and requires its checksum, nside, schema,
representation, omitted-pixel semantics, pixel-domain size, occupied-pixel
count, integrated flux, admitted sources, and excluded sources to match the
report. Global report totals must match the canonical map, and
`observed_sources` must equal admitted plus excluded with checked arithmetic.

Normal candidate gates are:

- `canonical-map-integrity`;
- `canonical-map-cardinality`;
- `canonical-map-flux`;
- `population-accounting`;
- `pixel-coverage-galactic-plane`;
- `declared-science-policy`;
- `deterministic-independent-partial-merge`.

No derived resolution is expected or loaded. Resolution comparison belongs to
a separate pre-publication study using independently generated source-level
runs, not synthetic resampling.

Scientific production admission remains stricter than structural validity. It
still requires catalogue provenance, calibrated non-proxy photometry,
population correction, complete passband treatment, independent comparison,
redistribution approval, longitude and plane/pole behaviour, and uncertainty
evidence defined in [science requirements](science-requirements.md).
