# Very-bright-star supplement

Design: [`design-v1.md`](design-v1.md).

Implementation lives in `crates/nsb-data-tools/src/starlight/bright_stars/`.

## Status

| Item | State |
|---|---|
| Scientific design | **v1 implemented** |
| Catalogue choice | Hipparcos-2 primary; Tycho-2 colours; XHIP optional SpT input |
| Redistribution | **External inputs only**; bundled production redistribution remains gated by `#103` |
| Artifact contracts | `starlight-bright-stars-v1` (measured 336–650 nm) and `starlight-bright-stars-combined-v1` (combined 300–650 nm) |
| Spectral coverage | The combined artifact includes the CK04-based 300–336 nm completion plus 336–650 nm reconstruction; the measured-only artifact remains diagnostic and fails closed in a combined build |
| Replacement semantics | Gaia `source_id` suppression + supplement admission, checked atomically |
| Runtime config | `starlight.bright_star_supplement` optional pin during candidate generation |
| Frozen candidate | **Included** in the #211 combined 300–650 nm candidate with checksum-pinned provenance |
| Bundled runtime activation | **off** while #103 redistribution approval is pending |

The current release-candidate bundle pins candidate-level technical and external
cross-implementation validation. This page does not independently promote raw
catalogue inputs or authorize redistribution.

Do not embed Hipparcos, Tycho-2, or XHIP catalogue bytes in the NSB repository
or published packages.
