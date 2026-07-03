# Release checklist

- [ ] `ComponentMask::ALL`, CLI `all`, examples, and docs agree.
- [ ] No removed compatibility API appears under `crates/*/src`.
- [ ] `Cargo.lock` is committed and the local Siderust checkout matches the compatibility matrix.
- [ ] Asset verifier passes and every data file has source, license, checksum, schema, generator, command, validation report, and maturity.
- [ ] Runtime starlight header checks agree with the manifest.
- [ ] Validated external starlight rejects incomplete provenance, checksum/header drift, proxy photometry, incomplete HEALPix maps, failed diagnostics, and missing independent-comparison evidence.
- [ ] External-reference fixtures state source, locator, unit, band, tolerance, assumptions, and deviation class.
- [ ] Model maturity and known limitations match CLI metadata.
- [ ] CTAO profiles remain uncalibrated unless dedicated validation data justify promotion.
- [ ] B/V values remain labelled diagnostic unless passband validation lands.
- [ ] Formatting, clippy, locked tests, doctests, docs, release build, MSRV, and `cargo deny` pass.
- [ ] Binary distribution plan satisfies AGPL dependency obligations and asset licenses.
- [ ] Scheduled/manual benchmarks compile and performance changes are summarized.
- [ ] `CHANGELOG.md` and version constants are updated.
- [ ] PR body lists fully resolved issues and issues left open with evidence-based reasons.
- [ ] Release tag is created only after all blocking boxes are satisfied.
