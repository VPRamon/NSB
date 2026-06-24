# Siderust compatibility

| NSB | Siderust release | Exact revision | Rust MSRV | Status |
|---|---|---|---|---|
| 0.1.x | 0.10.1 | `8d94b8375ae23c26d00346f74951e52cd1b595cc` | 1.89 | Supported |

All workspace crates use the same exact revision. Moving branches are forbidden.

To update Siderust:

1. review upstream release notes and scientific/data API changes;
2. change the revision in all three crate manifests;
3. run `cargo update -p siderust` and review `Cargo.lock`;
4. update this matrix, `CHANGELOG.md`, and CLI version metadata;
5. run every release gate and scientific validation test;
6. include the revision change and compatibility evidence in one reviewed PR.
