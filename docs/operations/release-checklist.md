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
- [ ] `cargo package -p nsb --list` contains only intended distribution files and the resulting `.crate` is below the crates.io 10 MiB upload limit; specifically, the four frozen Starlight candidate/report files remain in Git but are excluded from the `.crate`, while their SHA-256 is checked against the release review bundle.
- [ ] `cargo publish --manifest-path crates/nsb/Cargo.toml --dry-run --locked` succeeds on the exact release commit.
- [ ] The registry verifier passes from a normal checkout and validates every tracked scientific payload checksum.
- [ ] The release and default manual scientific-validation paths pass without external asset fetching; the optional `verify_upstream` manual input is reserved for live PALACE provenance regeneration.
- [ ] The Moonlight reproducibility workflow completes `update → build → validate → publish` and leaves the committed Mie LUT, multiple-scattering LUT, and manifest byte-for-byte unchanged.
- [ ] Every data file has source, license, checksum, schema, generator, command, validation report, maturity, and storage metadata where applicable.
- [ ] Every `runtime_embedded = true` asset has explicit redistribution terms; release automation fails closed for missing, unresolved, pending, or unrecorded terms.
- [ ] The packaged Airglow runtime contains `airglow_palace_v1.dat` with the manifest checksum; PALACE attribution and CC BY 4.0 data terms remain in `THIRD_PARTY_NOTICES.md`.
- [ ] For the four-component release, #103 contains the actual authorized decision; the exact compact binary and v2 sidecar are production-registered and `ALL`/`DEFAULT` evaluate all four components in Rust and installed Python. Pending development packages exclude Starlight and are not this release configuration.
- [ ] The immutable approval inventory and review bundle still match the decision. The current inventory is the verified deterministic production/publication projection, with approved planned channels distinguished from actual published channels; no approved hashes were silently repinned.
- [ ] Record actual Git publication after pushing the approved product, then commit that metadata before tagging. After registry publication, retain the workflow's publication-inventory artifact and commit the updated current inventory. Use the [lifecycle procedure](../nsb_components/starlight/release-candidate/README.md#approval-and-publication-lifecycle).
- [ ] Verify all 34 Starlight source inputs have a defensible, cited provenance/rights classification; XHIP/SVO unverified grants and later Hipparcos-2/Tycho-2 scope cannot be silently represented as CC BY 4.0 or cleared by a CI string.
- [ ] Before a release that includes the Starlight map, issue #103 records an authorized human decision and channel-specific restrictions. Do not approve automatically solely because source metadata is populated.
- [ ] Compact Starlight admission agrees with the approved binary/v2-sidecar hashes and self-loads through `ValidatedStarlightMap`; exact equivalence with the frozen runtime CSV is preserved.
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

## Starlight production preparation — 2026-10-14

- [x] Actual maintainer decision is `approved_with_conditions`, with authenticated
  `VPRamon` identity and explicit acceptance recorded at `2026-10-14T19:50:19Z`.
  See the [decision](../nsb_components/starlight/release-candidate/redistribution-review-decision-v1.json)
  and [accepted findings](../nsb_components/starlight/licensing/derived-map-rights-assessment-v1.md#13-explicit-project-authorization-and-accepted-findings).
- [x] Promote the exact approved compact binary and v2 sidecar; register both as
  production and embedded. Preserve the frozen candidate, radiances, uncertainties
  and source terms. The immutable approval snapshot remains unchanged.
- [x] Push the approved pair to PR #238, then record actual Git publication in
  the current inventory without repinning the decision or review bundle.
- [x] Actual Rust production evaluation requires all four default components,
  positive Starlight and an exact component sum.
- [x] Test the installed production wheel and sdist-built wheel outside the source
  checkout, including Rust/Python parity and unconditional four-component defaults.
  Each passes 78 tests; both contain the exact approved embedded map and notices.
- [x] Cargo package and publish dry-run succeed below the 10 MiB crate limit.
- [ ] Review the final PR #238 hosted acceptance gates before merge/tag. Validation
  results, package sizes and exact revision are recorded in the PR description.
- [ ] Obtain the maintainer's separate final merge/tag/registry-publication
  authorization. This preparation does not create a tag or upload to registries.

The frozen candidate registry entry preserves its historical preapproval text;
production authority is the canonical decision and the two admitted production
entries. Do not edit that baseline entry and silently invalidate its approved
snapshot hash. Existing source and model maturity limitations still apply.
