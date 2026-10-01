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
| Runtime config | `starlight.bright_star_supplement` optional pin |
| Default activation | **off** / experimental |
| `scientifically_validated` | **false** |

Do not embed Hipparcos, Tycho-2, or XHIP bytes in the NSB repository.
