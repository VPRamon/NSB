# Production-readiness roadmap

Status: Maintainer roadmap for the current release branch.
Audience: Maintainers and release reviewers.
Scope: Issue-level workstreams, blocking release evidence, and remaining
scientific gaps.
Non-goals: This roadmap is not a user-facing guarantee or a calibrated-science
claim.

```text
#30 coherent defaults
  ├─> #31 production starlight ─> #35 external validation ─> #36 photometry
  └─> #32 API cleanup
#34 dependency pin ─> #33 CI gates
#40 asset registry ─┬─> #31 starlight
                    ├─> #35 validation
                    └─> #38 CTAO calibration
#37 performance ─> #39 operational metadata ─> #41 release pass
all workstreams ─> #42 release decision
```

| Order | Issue | Release role | Current disposition |
|---:|---|---|---|
| 1 | #30 | Blocking | Coherent production-safe default; starlight explicitly outside it |
| 2 | #31, #26, #28, #45 | Blocking for bundled starlight claims | Validated external production path complete; real licensed bundled product still required |
| 3 | #32 | Blocking | Removed compatibility public API |
| 4 | #34, #33 | Blocking | Exact dependency and release gates |
| 5 | #40 | Blocking | Registry and verifier implemented |
| 6 | #35 | Blocking for calibrated claims | Published KS91 case present; broader external campaign required |
| 7 | #36 | Blocking for starlight claims | Siderust Gaia DR3/passband primitives wired into NSB release tools; production Gaia asset and independent validation remain release-blocking |
| 8 | #37 | Blocking | Cached data and realistic benchmarks implemented |
| 9 | #38 | Post-release planning enhancement | Remains planning until cleared CTAO data exist |
| 10 | #39 | Blocking | Audit-complete JSON and stable CSV v1 |
| 11 | #41 | Blocking | Documentation and release checklist |

Minimum software release: all CI gates pass, default claims remain generic or
planning-grade, experimental starlight is opt-in, and no calibrated CTAO or
passband claim is made.

Outcome B for #45 is complete: callers can use a validated external map through
a fail-closed manifest contract. Starlight stays outside defaults because NSB
does not bundle the referenced catalogue, license, calibration report, or
independent comparison.

Minimum calibrated-science release: add independently licensed catalogue/site
assets, recover inherited-asset licenses, implement validated passband
integration, pass external comparison tolerances, then update maturity metadata.
