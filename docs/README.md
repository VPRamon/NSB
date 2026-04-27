# NSB documentation

This directory centralizes persistent reports and project documentation for the
NSB Rust port. The root `README.md` remains the package entry point for GitHub
and crates.io metadata.

| Document | Purpose |
|---|---|
| `DARKNSB_REPORT.md` | Inspection report for the original Python `darknsb` model and data. |
| `SIDERUST_REIMPLEMENTATION_REPORT.md` | Assessment of how `darknsb` maps onto the SideRust ecosystem. |
| `NSB_STAGED_IMPLEMENTATION_PLAN.md` | Staged roadmap for implementing NSB functionality. |
| `NSB_CONCEPT_PROVENANCE_AND_SIDERUST_REUSE_REPORT.md` | Source-of-knowledge and generic SideRust reuse assessment for each NSB concept. |
| `TODO_SIDERUST.md` | Local NSB units/helpers that should migrate upstream once stable. |

Generated discrepancy reports are still written under `target/` by the test
suite because they are build artifacts, not source documentation.
