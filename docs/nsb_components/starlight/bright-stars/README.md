# Very-bright-star supplement

Design: [`design-v1.md`](design-v1.md).

Implementation lives in `crates/nsb-data-tools/src/starlight/bright_stars/`.

## Status

| Item | State |
|---|---|
| Scientific design | **v1 written**; combined-band contract in #207 |
| Catalogue choice | Hipparcos-2 primary; Tycho-2 colours; XHIP optional SpT input |
| Redistribution | **External opt-in only** (ESA CC BY-NC 3.0 IGO / CDS terms); `#103` human gate |
| Measured artifact | `starlight-bright-stars-v1` / `measured-336-650` (schema 1) |
| Combined artifact | `starlight-bright-stars-combined-v1` / `combined-300-650` (schema 2) |
| Spectral coverage | Measured-only still fail-closed in combined runs; combined uses one Hp-scaled CK04 SED for 300–336 and 336–650 |
| Replacement semantics | Gaia `source_id` suppression + supplement admission, checked atomically |
| Runtime config | `starlight.bright_star_supplement` optional pin |
| Default activation | **off** until production candidate regeneration on Ladon/BeegFS |
| `scientifically_validated` | **false** |

Do not embed Hipparcos, Tycho-2, or XHIP bytes in the NSB repository.

## Product compatibility

| Artifact product_band | Starlight `product_band` | Result |
|---|---|---|
| `measured-336-650` | `measured-336-650` | allowed |
| `measured-336-650` | `combined-300-650` | **rejected** |
| `combined-300-650` | `combined-300-650` | allowed |
| `combined-300-650` | `measured-336-650` | **rejected** (no measured projection) |
