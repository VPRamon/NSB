# Siderust compatibility

| NSB | Siderust release | Dependency source | Baseline revision | Rust MSRV | Status |
|---|---|---|---|---|---|
| 0.1.x | 0.10.1 | local `../../siderust` checkout | `8d94b8375ae23c26d00346f74951e52cd1b595cc` | 1.89 | Supported |

All workspace crates resolve Siderust through the same local path dependency in
their manifests. Moving branches are forbidden for release builds. The validated
external starlight admission path depends on this Siderust checkout's typed
HEALPix completeness/value, flux-conservation, plane/pole, and longitude-wrap
validators; a checkout update therefore requires rerunning its admission and CLI
fixtures, not only compiling the API.

To update Siderust:

1. review upstream release notes and scientific/data API changes;
2. update the local `../../siderust` checkout and keep all three crate
   manifests on the same local path dependency;
3. run `cargo update -p siderust` and review `Cargo.lock`;
4. update this matrix, `CHANGELOG.md`, and CLI version metadata;
5. run every release gate and scientific validation test;
6. include the checkout change and compatibility evidence in one reviewed PR.
