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
- [ ] For the four-component release, #103 contains the actual authorized decision; the exact compact binary and v1 sidecar are production-registered and `ALL`/`DEFAULT` evaluate all four components in Rust and installed Python. Pending development packages exclude Starlight and are not this release configuration.
- [ ] The immutable approval inventory and review bundle still match the decision. The current inventory is the verified deterministic production/publication projection, with approved planned channels distinguished from actual published channels; no approved hashes were silently repinned.
- [ ] Record actual Git publication after pushing the approved product, then commit that metadata before tagging. After registry publication, retain the workflow's publication-inventory artifact and commit the updated current inventory. Use the [lifecycle procedure](../nsb_components/starlight/release-candidate/README.md#approval-and-publication-lifecycle).
- [ ] Verify all 34 Starlight source inputs have a defensible, cited provenance/rights classification; XHIP/SVO unverified grants and later Hipparcos-2/Tycho-2 scope cannot be silently represented as CC BY 4.0 or cleared by a CI string.
- [ ] Before a release that includes the Starlight map, issue #103 records an authorized human decision and channel-specific restrictions. Do not approve automatically solely because source metadata is populated.
- [ ] Compact Starlight admission agrees with the approved binary/v1-sidecar hashes and self-loads through `ValidatedStarlightMap`; exact equivalence with the frozen runtime CSV is preserved.
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
- [ ] Manual benchmarks compile and performance changes are summarized.
- [ ] `CHANGELOG.md` and version constants are updated; `python3 scripts/check-release-metadata.py --tag v0.1.0` passes on the exact tagged commit.
- [ ] Review the first-release v1 schemas and regenerated hashes using existing checks; confirm the recorded checksum-specific Starlight v1 authorization and current hosted CI evidence (see [reset record](../nsb_components/starlight/release-candidate/schema-reset-v1.md)).
- [ ] The first-release API freeze is active: `crates/nsb/api/API_FROZEN` exists, `crates/nsb/api/public-api.txt` is current, and `scripts/check-public-api.sh` passes. Do not tag a public release while the API gate reports pre-freeze mode.
- [ ] The release tag is exactly `v<crates/nsb/Cargo.toml version>`; tagged releases build all Python wheels and the sdist before publishing.
- [ ] `CARGO_REGISTRY_TOKEN` is configured for crates.io and the `pypi` GitHub environment is configured for PyPI Trusted Publishing/OIDC.
- [ ] The tag-triggered release workflow publishes `nsb` to crates.io and `nsb-rust` to PyPI from the same commit.
- [ ] Release tag is created only after all blocking boxes are satisfied.

## Tagging the first public release

Run these steps **only after this preparation PR is merged** and the exact
release commit has green CI and any required scientific review. A tag on the
unmerged PR branch or an older commit is not the approved release.

1. Confirm a crates.io token with publish permission is stored as the GitHub
   Actions secret `CARGO_REGISTRY_TOKEN`. For PyPI, configure the repository's
   `pypi` environment and the `nsb-rust` Trusted Publisher for the
   `.github/workflows/publish.yml` workflow. First-time PyPI publication may
   require a pending publisher configured before the project exists.
2. From a clean local checkout of the intended release commit, run:

   ```bash
   git fetch origin main --tags
   git switch main
   git pull --ff-only origin main
   git status --short
   python3 scripts/check-release-metadata.py --tag v0.1.0
   git tag -a v0.1.0 -m 'NSB v0.1.0'
   git push origin v0.1.0
   ```

3. Follow the [Release workflow](../../.github/workflows/publish.yml) in
   GitHub Actions. Its tag validation, scientific tests, crate package/dry-run,
   five platform wheels and sdist must all succeed **before** publishing.
   The crate publishes first; PyPI receives the verified artifacts afterwards.
4. Verify `nsb` version `0.1.0` on crates.io/docs.rs and `nsb-rust` version
   `0.1.0` on PyPI. Retain the workflow's `publication-inventory` artifact,
   reconcile the published scientific inventory, and optionally create the
   GitHub Release from the existing tag (the workflow does not create one).

Never replace an existing `v0.1.0` tag to retry a failed upload. Use the
partial-publication recovery procedure below and inspect actual registry
artifacts before any retry. Automation cannot verify the presence or validity
of repository secrets, environment protection rules, Trusted Publisher
registration, or the maintainer's final release authorization.
## Partial publication recovery

Do not retag or change scientific approval identities after a partial upload.
Inspect the failed job and the actual registry versions and checksums first.
`publish-changed.sh` skips a crates.io version already present; this is only an
idempotency check and does not attest that its bytes match the tagged artifact.
For a crates.io success followed by a PyPI failure, rerun failed jobs using the
original verified wheel/sdist artifacts. If PyPI accepted only some files,
reconcile each accepted filename and digest before uploading the missing files;
the workflow deliberately does not silently skip conflicting existing files.
The final inventory job runs only after both registries succeed. Retain its
`publication-inventory` artifact and commit the verified current inventory in a
follow-up change without modifying the immutable approval inventory or bundle.
Registry credentials, OIDC configuration, final authorization and renewed
checksum-bound external scientific evidence require maintainer action before
release. No checklist completion alone authorizes publication.
