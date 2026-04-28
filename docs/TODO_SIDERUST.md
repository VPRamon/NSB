# NSB upstream TODOs

The old `nsb::units` layer no longer exists. The simplified crate now exposes
shared quantity types directly from `qtty` and `siderust`, for example:

- `qtty::radiometry::PhotonsPerSquareCentimeterNanosecondSteradian`
- `qtty::radiometry::S10s`
- `qtty::photometry::SurfaceBrightness`
- `siderust::coordinates::spherical::Direction`

That means there is currently **no** NSB-local unit wrapper that needs to be
migrated upstream just to keep the public API coherent.

If future upstreaming work happens, it is more likely to be in these areas:

| Candidate | Why it may belong upstream |
|---|---|
| Provenance helpers for bundled scientific datasets | Several NSB data files carry partial lineage and checksum metadata that could be standardized. |
| Reusable sky-brightness component traits or result metadata | If other crates need componentized diffuse-sky models, a shared abstraction may become worthwhile. |
| Additional photometric/radiometric aliases | Only if a new quantity appears repeatedly across sibling crates rather than staying NSB-specific. |

Until that happens, this file is mainly a reminder that the simplification work
already moved the public surface onto ecosystem types instead of local wrappers.
