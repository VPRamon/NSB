# Very-bright-star supplement

Design: [`design-v1.md`](design-v1.md).

Implementation lives in `crates/nsb-data-tools/src/starlight/bright_stars/`.

## Status

| Item | State |
|---|---|
| Scientific design | **v1 written** |
| Catalogue choice | Hipparcos-2 primary; Tycho-2 colours; XHIP optional SpT input |
| Redistribution | **External opt-in only** (ESA CC BY-NC 3.0 IGO / CDS terms); `#103` human gate |
| Artifact contract | `starlight-bright-stars-v1` schema + SHA pin |
| Spectral coverage | **Measured 336–650 nm only**; combined 300–650 loading fails |
| Replacement semantics | Gaia `source_id` suppression + supplement admission, checked atomically |
| Runtime config | `starlight.bright_star_supplement` optional pin |
| Default activation | **off** / experimental |
| `scientifically_validated` | **false** |

Do not embed Hipparcos, Tycho-2, or XHIP bytes in the NSB repository.
