# Ecosystem reuse and integration audit

Inspected 2026-10-17 against Siderust 0.14.1, QTTY 0.8.6,
AFFN 0.11.0 and Optica 0.4.0 as resolved by Cargo.lock. Inspection included
registry source, not only API names. No dependency version change is needed.

| Requirement | Existing ecosystem API | NSB implementation / duplication | Ownership and decision | Compatibility |
|---|---|---|---|---|
| Wavelengths and photon radiance | `siderust::qtty::{length, radiometry}` | `units` already reexports the same types | Keep facade; no competing unit system | Public quantities unchanged |
| Typed celestial directions and transforms | Siderust coordinates wrap AFFN directions/frames/centers | Existing `Target`, Galactic transformations | Keep Siderust layer; no direct AFFN dependency | Same coordinate types |
| HEALPix indexing and ordering | `siderust::healpix::{HealpixGrid, HealpixIndex, HealpixOrdering, HealpixMap}` | Starlight uses grid lookup | Keep upstream geometry; map schema remains NSB domain logic | Reject unsupported semantics explicitly |
| Clipped piecewise-linear integration | `siderust::optica::spectrum::integrate::trapz_range` and typed `SampledSpectrum::integrate_range` | Zodiacal uses typed spectrum; Airglow has a duplicate raw kernel in `band` | Reuse upstream allocation-free kernel for Airglow | Same sampled-spectrum contract; numerical regression tests |
| Photometry and Gaia | Siderust passbands, Gaia photometry and integrated starlight primitives | NSB covariance-aware XP generation and calibrated-product schema | Keep NSB scientific product/provenance contract; do not infer B/V from top-hat bins | Missing capability stays unavailable |
| Spectral bins and covariance | No upstream type providing this versioned validated NSB product | NSB generic v2 and frozen v1 adapters | Keep one StarlightMap; private adapters preserve frozen bytes | No new per-band public class |
| Source/build provenance and immutable reports | Cargo input tracking and filesystem primitives | Data-tool build script and XP example | Small private helpers; no artifact framework | Output schema and dataset identities preserved |

No upstream change is required to unblock these PRs. A future reusable
covariance-aware spectral-product contract could be discussed upstream after
more than one consumer needs it; this mission does not create that API.

## Integration decision

Keep existing contributor history and synchronize with merge commits, without
force pushes. Merge latest main into #254, then #254 into #257, and #257 into
#258. Stack #257 on #254 and retain #258 on #257 so the reviewed diffs exclude
prerequisites. The original #254 commits were independently copied into #257;
Git's content merge reconciles them without replaying their changes. Historical
commits remain visible rather than rewriting the large research history.

Human merge order: **#254 → #257 → #258**. Retarget each remaining PR to main
after its prerequisite is merged if the hosting service does not do so. Verify
its resulting diff and CI before merging. No scientific map is regenerated or
promoted; TOA diagnostics remain separate from ground-level predictions.
