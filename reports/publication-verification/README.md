# Publication engineering evidence

This work starts from NSB main `57584ee298f25f8af7a6f169d71e87f8a59eea9a`.
Publication audit #273 was inspected at `8e0bb493bc8d6c8615032f5434f6362e9ba92f35`
and remains an independently reviewed draft. No manuscript files are overwritten.

## Investigation before implementation

| Blocker | Root cause or missing evidence | Required implementation | Verification |
| --- | --- | --- | --- |
| Release (#265) | Registry Siderust 0.14.1 lacks LayeredAtmosphere and NormalizedPhaseTable; workspace patch disappears from Cargo archive | Retain exact upstream pin and release guard; document external dependency, do not invent a registry version | Ordinary archive verification, publish dry-run, patched repository wheel and isolated sdist |
| Numerical verification | Many analytic, domain, integration, independent scattering tests already exist; manuscript lacks executed error receipts | Small analytic receipt probe plus focused photon-energy regression | Machine-readable errors and existing workspace tests |
| Interface equivalence | Rust Target is Mean J2000; Python/CLI take ICRS; separately copied constants are insufficient | One explicit ICRS/UTC/geodetic fixture consumed by all interfaces | Installed wheel, native probe, CLI pairwise differences and repeated runs |
| Scientific example | No version-pinned end-to-end manuscript example | Existing PALACE continuum F10.7 sweep at fixed geometry, no new physics | Registered numerical outputs, figure regeneration, provenance |

## Audit of existing coverage

- `crates/nsb/src/band.rs`: independent clipped linear integral, partition/full-band identities and unsupported domains. The kernel delegates to Optica `trapz_range`.
- `crates/nsb/tests/atmospheric_transport.rs`: identity/direct, optical depth, units and source-origin gates.
- `crates/nsb/tests/end_to_end_validation.rs`, `query_api.rs`: composition, known geometries and API contracts.
- `crates/nsb/tests/` plus `transport/{single,multiple}`: synthetic, independently generated and convergence references. Running these verifies their registered contracts, not observational accuracy.
- `python/tests/test_band.py`, `test_evaluation.py`, `test_errors.py`, `test_interop.py`: bands, composition, reference regressions, domain and frame/time parsing.
- `crates/nsb-cli/tests`: JSON contracts, invalid inputs, configuration, maturity and provenance.

New receipts quantify analytic integration/convergence, dimensional conversion and direct-path algebra; they do not duplicate the full existing suite. No independent absolute ephemeris accuracy certification is added: existing frame tests are consistency checks.

The validation scope is taken from #273's pinned audit: nsb-validation
`3191bfc7597b5a2add95456aeb2ea1b906979d22`, nsb2
`bc9320db03fbff69997ce366c6e76a37339c5d00`. These related repositories' historical scores
are not rerun or reinterpreted. No protected observational files are accessed.

## Navigate the evidence

- [Release scope and remaining external dependency](release-reproducibility.md).
- [Numerical contracts and SI-constant correction](numerical-verification.md).
- [Interface capability matrix and tolerances](interface-equivalence.md).
- [Executable example and reproduction commands](../../examples/publication/README.md).
- [Manuscript integration text and figure caption](manuscript-evidence.md).
- `verification-results.json`: executed commands and outcomes; `software-provenance.json`: candidate identity.
- `numerical-verification.json`, `photon-si-reference.json`, `interface-equivalence.json`: quantitative receipts.

Source candidate and evidence commits are intentionally distinct: the candidate
contains all executable changes and registered data; the evidence commit adds
run receipts that identify that candidate without requiring a self-referential
Git hash. No release, merge or protected observation access is performed.
