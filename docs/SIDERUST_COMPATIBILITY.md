# Siderust compatibility

| NSB | Siderust release | Dependency source | Baseline revision | Rust MSRV | Status |
|---|---|---|---|---|---|
| 0.1.x | 0.10.1 | immutable Git revision | `3b079f950b22d5c5bb7bddedf3a3bdd3f842b07c` | 1.89 | Supported |

All workspace crates resolve Siderust through the same immutable Git revision in
their manifests. Moving branches and unresolved local paths are forbidden for
release builds. The validated
external starlight admission path depends on this Siderust checkout's typed
HEALPix completeness/value, flux-conservation, plane/pole, and longitude-wrap
validators; a checkout update therefore requires rerunning its admission and CLI
fixtures, not only compiling the API.

To update Siderust:

1. review upstream release notes and scientific/data API changes;
2. choose a released Siderust version or immutable Git revision and keep all
   three crate manifests on the same dependency;
3. run `cargo update -p siderust` and review `Cargo.lock`;
4. update this matrix, `CHANGELOG.md`, and CLI version metadata;
5. run every release gate and scientific validation test;
6. include the checkout change and compatibility evidence in one reviewed PR.
