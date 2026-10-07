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
| `redistribution-review-decision-v1.json` | The sole authoritative human redistribution decision. It remains `pending`. |
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
registered `nsb-healpix-starlight-v1` map and
`nsb-starlight-runtime-manifest-v1` sidecar pass checksum and provenance
validation. Malformed or incomplete v2 provenance is rejected.

## Related issues

- #207 — final combined-band candidate and bright-star supplement.
- #103 — the remaining human/legal redistribution decision.
- #102 — technical packing, validation, and promotion automation.
