> Current reset state: the regenerated v1 artifacts await authorization. The active
> decision is pending renewed checksum-specific authorization. Original approvals
> and external/CI evidence are preserved in `approval-history/pre-v1-reset/`.
> See [schema-reset-v1.md](schema-reset-v1.md); historical activation results
> below apply to the former hashes only.

# Starlight release-candidate bundle and promotion mechanism (#102)

Status: Current fail-closed bundle for the frozen combined 300–650 nm candidate.

Scientific production readiness is machine-verifiable: the exact candidate,
technical validation, external cross-implementation validation, provenance,
and green CI are checksum-pinned. A separate manual scientific signature is
not required. Redistribution/licensing remains a distinct human/legal gate in
issue #103 and is not approved by this bundle.

## Files

| File | Role |
|---|---|
| `release-candidate-v1.toml` | Frozen candidate identity, deterministic RFC3339 generation timestamp, checksum pin for the canonical merge report, complete bright-star provenance copied from that report, technical status, external-validation pin, and runtime identities. |
| `redistribution-review-decision-v1.json` | The sole authoritative human redistribution decision, now `approved_with_conditions`. |
| `runtime-assets-v1.toml` | Deterministic packed runtime map and schema-v1 provenance sidecar identities. |
| `review-bundle-v1.toml` | Immutable release evidence pinned by the redistribution decision. |

The release-candidate gate table contains `validation_status`,
`redistribution_review_status`, and the report-only `promotion_eligible` field.
Scientific readiness is derived from the checksum-pinned external validation;
there is no `scientific_review_status` or scientific decision template.

## Deterministic runtime staging

Runtime assets can be generated and reviewed without granting redistribution:

```bash
nsb-data dataset starlight stage-runtime \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --repository-root . \
  --output-csv crates/nsb/data/starlight_nside128.release.csv \
  --output-sidecar crates/nsb/data/starlight_nside128.manifest.toml
```

This command verifies the pinned candidate bytes and checksum-pinned merge
report, requires the release-candidate bright-star structure to match the
canonical merge-report provenance exactly, validates the frozen RFC3339
generation timestamp, packs NESTED candidate pixels into the RING runtime
format, and writes the complete 34-input bright-star provenance into the
schema-v1 sidecar. Gaia and Hipparcos/XHIP/CK04 UV routes remain distinct. It
does not inspect or change the redistribution decision and does not mutate the
asset registry. Repeated staging from the same frozen evidence is byte-identical.

## Final promotion

```bash
nsb-data dataset starlight promote \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --redistribution-decision docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json \
  --repository-root . \
  --output target/starlight-promotion/production-manifest-draft.toml
```

Promotion fails closed unless all of the following agree:

1. The pinned candidate bytes, checksum-pinned merge report, deterministic
   generation timestamp, exact canonical bright-star provenance, and repository
   registry entry.
2. The technical validation and a real frozen green GitHub Actions run.
3. The checksum-pinned external validation using the registered validator and
   `nsb2` reference commits.
4. The deterministic runtime map and schema-v1 sidecar checksums.
5. The immutable review bundle and an authorized redistribution decision with
   reviewer identity, timestamp, candidate pin, inventory pin, and structured
   machine-verifiable conditions.

`promotion_eligible` and the TOML redistribution status are report snapshots;
the signed redistribution decision is authoritative. Passing scientific and
technical gates never implies redistribution approval.

## Runtime gate

`StarlightModel::BundledProductionGaiaDr3` and `ComponentMask::ALL` remain
fail-closed. The build only enables bundled production Starlight when a
registered `nsb-healpix-starlight-binary-v1` map (or supported external CSV) and
`nsb-starlight-runtime-manifest-v1` sidecar pass checksum and provenance
validation. Malformed or incomplete v1 provenance is rejected.

## Related issues

- #207 — final combined-band candidate and bright-star supplement.
- #103 — the remaining human/legal redistribution decision.
- #102 — technical packing, validation, and promotion automation.

## Compact runtime transport (binary v1)

`nsb-healpix-starlight-binary-v1` encodes the existing packed runtime CSV,
without regenerating the astrophysical map. Layout, in order:

| Bytes | Meaning |
| --- | --- |
| 8 | ASCII `NSBSTL01` (magic and format version) |
| 4 | Little-endian u32 length of the following UTF-8 header |
| variable | All CSV metadata, sorted by key, as `# key=value\n` lines |
| `12 * nside² * 32` | Dense pixels in the header's RING or NESTED order; four little-endian IEEE-754 f64 values per pixel |
| 32 | SHA-256 of every preceding byte, raw digest |

Fields are integrated radiance, statistical, systematic, and total uncertainty,
all in photons cm⁻² ns⁻¹ sr⁻¹. Coordinates and solid angle come from HEALPix.
No S10 values are synthesized: their absence remains explicit in the header.
This version supports the existing packed CSV's complete uncertainty triplet;
missing uncertainty fields are rejected rather than silently zero-filled.
Other map schemas remain available through the existing external map APIs.

There is no native struct serialization, padding, compression dependency,
quantization, reduced precision, or host endianness assumption. Unknown magic,
truncated/extra pixels, invalid lengths, checksum failures, nonfinite/negative
values and invalid uncertainty triplets fail closed. The external sidecar also
pins the SHA-256 of the complete binary including its internal digest. Its
only changed scientific-manifest field is `map_sha256`. Original header schema
labels identify the source CSV contract; the binary magic and registry schema
identify the transport. Original candidate and runtime CSV/sidecar hashes are
preserved as the frozen scientific evidence.

Reproduce locally (staging does **not** authorize distribution):

```bash
cargo run -p nsb-data-tools --locked -- dataset starlight stage-runtime \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --repository-root . \
  --output-csv /tmp/starlight_nside128.compact.release.bin \
  --output-sidecar /tmp/starlight_nside128.compact.manifest.toml
cargo test -p nsb-data-tools compact_runtime_matches --locked -- --nocapture
```

`runtime-assets-v1.toml` now pins these compact bytes in `[compact]`; it remains
inside the human decision's checksum-pinned review bundle. The compact files
are now conditionally approved, production-registered and committed. The
existing `promote --apply` command reproduces the
frozen CSV and sidecar, verifies their existing pins and every prior scientific
and legal gate, converts them, verifies compact pins, and installs/registers
`starlight_nside128.compact.release.bin` and
`starlight_nside128.compact.manifest.toml`. Do not hand-edit registry status.
The original CSV/report exclusions remain applicable to repository evidence;
the approved binary pair is eligible for packaging. Release CI reruns promotion
without applying it and compares the production registry to its results.

The build embeds map bytes and the sidecar. Admission runs once per process
through `OnceLock`; evaluators share the immutable map through `Arc`. Failure
is cached as an error, never as substitute data. The existing build-dependent
`ComponentMask::DEFAULT`/`ALL` includes all four components upon production
registration. Python uses that same Rust configuration and compiled asset;
wheels carry notices without a second data copy. Prior to approval, defaults
still contain three components and explicitly selecting missing bundled
Starlight returns an error. This PR does not claim four-component default
release readiness before approval.

### Equivalence and performance

On Linux x86_64, Rust 1.97.1, normal test profile (`opt-level=1`), one local
measurement including checksum verification and scientific admission:

| Metric | Frozen runtime CSV | Compact binary |
| --- | ---: | ---: |
| Asset bytes | 18,568,693 | 6,294,180 |
| Owned pixel storage | 15,728,640 bytes | 15,728,640 bytes |
| File admission | 187.8 ms | 72.0 ms |
| 100,000 identical directional lookups | 3.335 ms | 3.332 ms |

These are measurements, not cross-platform performance guarantees. Retained
pixel memory is unchanged (80 bytes per pixel), while embedded transport drops
by 66.1%; CSV parsing temporaries disappear. Peak RSS is not equivalent to
pixel allocation and must not be inferred from this table. The shared cache
eliminates repeated admission and pixel allocation across bundled evaluators.

All 196,608 pixels and 786,432 f64 fields compare bit-for-bit, including all
uncertainties. The Rust test also compares complete diagnostics (flux
conservation, plane/pole and longitude-wrap), deterministic bytes and evaluates
the sum of all four components using validated external admission. An
independent Python `struct.pack('<d', float(csv_field))` comparison agrees for
every field. Frozen candidate SHA-256 remains
`e8d31d0a04a18314e3273ffc9e2ad449014adca8710b11bda2f3b6cf2c811526`.

A separate release-mode process measurement (`/usr/bin/time`, same files;
filesystem read + checksum + admission) used 52,108 KiB peak RSS for CSV and
39,864 KiB for compact input, with 234 ms and 97 ms admission respectively.
RSS includes parsing temporaries and allocator behavior; this is a local
measurement under concurrent build load, not a promised memory ceiling.

A local transport-only checkout, explicitly without recording human approval,
verified Cargo packaging/build with the compact pair and exercised the bundled
four-component default. Its `.crate` was **6,385,561 bytes (6.09 MiB)**, leaving
3.91 MiB below the project's 10 MiB ceiling. This rehearsal artifact is not a
publication candidate. The earlier ~17.1 MiB packaging attempt included CSV
overhead/duplicate data (reported baseline); it is not a controlled comparison
against a historical preapproval package that excluded all Starlight payloads.
The frozen CSVs and upstream catalogues remain excluded after promotion.

Source-distribution testing found that maturin prunes unrelated workspace
members while retaining the workspace Cargo.lock. Cargo must normalize that
lockfile before building the library-only sdist. `locked=true` is therefore
not imposed globally by pyproject.toml; release wheel commands explicitly use
`--locked`, and sdist verification builds the extracted archive with the normal
PEP 517 backend. This preserves retained dependency pins while allowing removal
of unrelated workspace packages. The binary encoding itself is deterministic.

### Historical preregistration recovery

The original preregistration recovered for #226 is preserved byte-for-byte in
`approval-history/pre-v1-reset/validation/results/frozen-preregistration-v1.toml`.
The active `validation/results/frozen-preregistration-v1.toml` now uses the v1
candidate identifier. All numerical tolerances, references and regions are
unchanged; the validation artifact manifest records the regenerated hash and
byte count. Historical approval and run records refer to the archived bytes.

### Explicit compact channel approval

Promotion of compact assets requires decision pins for both
`compact-runtime-map-v1` and `compact-runtime-sidecar-v1`, matching the hashes
in `runtime-assets-v1.toml`, with `git_repository`, `crates_io`, `pypi_wheel`
and `pypi_sdist` in each pin's `approved_channels`. The immutable approval plan
uses staged entries; the current inventory now records actual Git publication. Approval only for existing
Git artifacts cannot authorize future package distribution. A GitHub release
asset additionally needs approval for that publication channel. The actual
canonical decision records the maintainer, acceptance time, restrictions and
all five approved channels. The rights assessment explains the accepted findings.

## Approval and publication lifecycle

The sole authoritative decision remains
`redistribution-review-decision-v1.json`. The separate
`redistribution-review-decision-template-v1.json` is **TEMPLATE ONLY**: it shows
`approved_with_conditions`, all exact required pins and planned channels, but
leaves reviewer name/role/time null and therefore cannot authorize promotion.
It is derived from the evidence and deliberately not included in the bundle
that its own fields reference, avoiding a hash cycle. Never automatically copy
it over the actual authoritative decision. For the previous hashes, the maintainer expressly accepted
the documented purpose, scope, source conditions, XHIP map-only interpretation
and independent embedded-data finding. V2 is the explicit documentary correction
of stale notices; the original v1 evidence and decision are preserved unchanged
in the [amendment history](approval-history/notice-amendment-v2.md).

| Stage | Immutable approval inventory | Current publication inventory |
|---|---|---|
| Candidate/staged | Frozen exact map/sidecar identities, `lifecycle = "staged"`, planned channels, no claimed upload | Same staged state |
| Approved | Same bytes; authorization lives in the human decision | Staged until installed |
| Production registered | Same approved bytes and bundle/decision pins | `production_registered`, actual channels empty; updated registry digest |
| Published | Same approved bytes and decision | `published`, `distributed = true`, only actual channels in `channels` |

`licensing/approval-inventory-v1.toml` is the regenerated review input awaiting authorization, pinned
by the release candidate, review bundle and decision. `artifact-inventory-v1.toml`
is the current state and is **not** an input to that checksum cycle. Initially
both inventories describe the same assets. Promotion derives the current state
from the snapshot; only the two compact lifecycle/current-channel fields and
the deterministic production-registry digest may change. Source licences,
notices, planned channels and scientific identities remain exactly as approved.
Both validators require SHA-256 for **every distributed artifact**, including
the real external-manifest documentation file. The bundle pins that file too.

The baseline registry digest in the approval snapshot is verified after removing
only the two deterministic production entries. Repeat promotion checks all
registry fields, installed bytes and the current inventory before writing;
partial, tampered or unauthorized states fail closed. Approved conditions and
all nested bundle evidence are verified for both approval dispositions.
Registration itself is not represented as publication. Recording actual channels
is an operator/workflow assertion after successful upload, not permission to
upload and not a change to human authorization.

After the actual human decision, from the repository root:

```bash
cargo run -p nsb-data-tools --locked -- dataset starlight promote \
  --repository-root . \
  --release-candidate docs/nsb_components/starlight/release-candidate/release-candidate-v1.toml \
  --redistribution-decision docs/nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json \
  --apply
```

Commit the exact compact pair, updated runtime registry and current inventory,
run the production/scientific/package checks, and push the authorized Git
publication. **After that push**, rerun the same command with
`--published-channel git_repository`, commit the resulting current inventory and
push it. This records the already-published scientific bytes without changing
the approval snapshot or requiring a second authorization. Repeat recording is
idempotent. Do not record crates.io/PyPI/GitHub-release channels before uploads.

The tag workflow requires the real approved production pair and verified current
inventory, checks its package hashes/notices and installed four-component tests,
then publishes only after maintainer authorization. After successful crates.io
and PyPI jobs, it records `crates_io`, `pypi_wheel`, `pypi_sdist` in a downloadable
publication-inventory artifact. Retain/commit that current inventory (or rerun
the same command with those repeated `--published-channel` flags). Record
`github_release` only if map-containing release artifacts were actually uploaded.
Never repin the immutable snapshot, review bundle or human decision merely
because publication metadata changed.
