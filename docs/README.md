# NSB documentation

This directory holds supporting notes and historical reports for `nsb`.

For the **current** crate API and CLI, start here first:

| Entry point | Purpose |
|---|---|
| `../README.md` | Current package overview, library API, CLI usage, and layout. |
| `../examples/point_query.rs` | Runnable point-in-time library example. |
| `../examples/threshold_window.rs` | Runnable threshold-window library example. |

Several documents below were written during the earlier `darknsb`-porting phase.
They are still useful as provenance and design background, but they may refer to
surfaces that no longer exist in the simplified crate (Python bindings, vendored
`darknsb`, named-target catalog, compatibility wrappers, and Python golden
tests).

| Document | Purpose |
|---|---|
| `DARKNSB_REPORT.md` | Historical inspection report for the original Python `darknsb` model and data. |
| `SIDERUST_REIMPLEMENTATION_REPORT.md` | Historical assessment of how `darknsb` mapped onto the SideRust ecosystem. |
| `NSB_STAGED_IMPLEMENTATION_PLAN.md` | Historical staged plan from the original port and API-shaping work. |
| `NSB_CONCEPT_PROVENANCE_AND_SIDERUST_REUSE_REPORT.md` | Provenance notes for NSB concepts, data sources, and possible upstream reuse. |
| `TODO_SIDERUST.md` | Current notes on what, if anything, still makes sense to upstream from NSB. |
