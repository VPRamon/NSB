# Release checklist

Status: Maintainer checklist for release review.
Audience: Release maintainers and reviewers.
Scope: Repository, validation, documentation, dependency, and distribution gates
that must be checked before tagging.
Non-goals: This checklist does not replace the validation matrix or create
scientific calibration evidence.

- [ ] `ComponentMask::ALL`, CLI `all`, examples, and docs agree.
- [ ] No removed compatibility API appears under `crates/*/src`.
- [ ] `Cargo.lock` is committed and the Siderust crates.io source identity matches the compatibility matrix.
- [ ] `cargo package -p nsb --locked` succeeds using publishable registry dependencies.
- [ ] `cargo package -p nsb --list` contains only intended distribution files and the resulting `.crate` is below the crates.io 10 MiB upload limit.
- [ ] `cargo publish --manifest-path crates/nsb/Cargo.toml --dry-run --locked` succeeds on the exact release commit.
- [ ] The registry verifier passes from a normal checkout and validates every tracked scientific payload checksum.
- [ ] The release and default manual scientific-validation paths pass without external asset fetching; the optional `verify_upstream` manual input is reserved for live PALACE provenance regeneration.
- [ ] The Moonlight reproducibility workflow completes `update → build → validate → publish` and leaves the committed Mie LUT, multiple-scattering LUT, and manifest byte-for-byte unchanged.
- [ ] Every data file has source, license, checksum, schema, generator, command, validation report, maturity, and storage metadata where applicable.
- [ ] Every `runtime_embedded = true` asset has explicit redistribution terms; release automation fails closed for missing, unresolved, pending, or unrecorded terms.
- [ ] The packaged Airglow runtime contains `airglow_palace_v1.dat` with the manifest checksum; PALACE attribution and CC BY 4.0 data terms remain in `THIRD_PARTY_NOTICES.md`.
- [ ] Bundled Gaia DR3 starlight, if shipped, has only the derived release CSV/TOML committed under `crates/nsb/data`, both registered as runtime-embedded production assets, plus validation evidence under `docs/nsb_components/starlight/validation/` and the release-candidate review bundle.
- [ ] Runtime starlight header checks agree with the release CSV manifest, and `pack_starlight_asset --production` self-loads the emitted CSV/TOML pair through `ValidatedStarlightMap`.
- [ ] Gaia production extraction diagnostics show zero rejected selected sources, zero XP chunk failures, and at least one accepted XP source.
- [ ] Gaia map validation reports `radiance_field = integrated_ph_cm2_ns_sr` and passing integrated flux conservation.
- [ ] Validated external starlight rejects incomplete provenance, checksum/header drift, proxy photometry, incomplete HEALPix maps, failed diagnostics, and missing independent-comparison evidence.
- [ ] External-reference fixtures state source, locator, unit, band, tolerance, assumptions, and deviation class.
- [ ] Model maturity and known limitations match CLI metadata.
- [ ] CTAO profiles remain uncalibrated unless dedicated validation data justify promotion.
- [ ] B/V values remain labelled diagnostic unless passband validation lands.
- [ ] Format, check, Clippy, unit tests, integration tests, doctests, docs, release build, MSRV, `cargo deny`, all canonical examples, and the aggregate `CI success` gate pass.
- [ ] Coverage gates in [`coverage-policy.toml`](../../coverage-policy.toml) pass on `main`: workspace and `nsb` line floors are blocking (LCOV line data; fail-closed if `nsb` is missing), and the HTML/JSON/LCOV artifacts remain available. Do not lower a floor merely to make a PR pass. See [Coverage policy](../developer-guide/coverage.md).
- [ ] Binary distribution plan satisfies AGPL dependency obligations and asset licenses.
- [ ] Scheduled/manual benchmarks compile and performance changes are summarized.
- [ ] `CHANGELOG.md` and version constants are updated.
- [ ] The first-release API freeze is active: `crates/nsb/api/API_FROZEN` exists, `crates/nsb/api/public-api.txt` is current, and `scripts/check-public-api.sh` passes. Do not tag a public release while the API gate reports pre-freeze mode.
- [ ] The release tag is exactly `v<crates/nsb/Cargo.toml version>`; tagged releases build all Python wheels and the sdist before publishing.
- [ ] `CARGO_REGISTRY_TOKEN` is configured for crates.io and the `pypi` GitHub environment is configured for PyPI Trusted Publishing/OIDC.
- [ ] The tag-triggered release workflow publishes `nsb` to crates.io and `nsb-rust` to PyPI from the same commit.
- [ ] PR body lists fully resolved issues and issues left open with evidence-based reasons.
- [ ] Release tag is created only after all blocking boxes are satisfied.
