# Adaptive search scientific validation

The regression suite tests production discovery and strict classification, with
analytic functions, authoritative physical evaluations and independent local
references. It contains no historical certificate-feasibility engine. A compact
test-only discrete reference retains the original grid and Illinois crossing
rules for numerical comparisons and scan600 timing. Its separate uncached
radiance path also checks exact astronomical cache equivalence. Historical full
implementations remain available in Git.

## Coverage and interpretation

The 204-scenario production matrix covers CTAO-N/S, both Moonlight models,
1/7/30/365 days, 1/10/100 targets, individual components, all 16 masks, actual
`ALL`, explicit four-component composition, near/far Moon geometry and Moon-down
certificates. Configured Starlight is a synthetic benchmark fixture, not a newly
admitted production asset. Composed thresholds add the cached Starlight scalar
so dynamic components still cross; pure Starlight retains the nominal threshold.

Every complete adaptive schedule is checked against independent authoritative
30-second scans for single targets and 120-second scans for batches. Global
matched-boundary tolerance remains 0.533 s: the reference's 0.432 s half bracket,
adaptive's 0.1 s accepted-side bracket and 0.001 s representation allowance.
Unexplained topology or boundary differences fail before a timing row is emitted.
Every timed output must exactly equal its validated warmup output.

A discrete reference can itself miss an event. Additional rejected excursions or
accepted islands require an independent local scan of at most 10 ms and both
boundaries within 0.106 s. Accepted islands also require authoritative interior
acceptance and eligibility clipping. A reference singleton jump artifact can be
excluded only after checking the physical edge and five rejected interior points;
coarse endpoint classification is checked just inside the edge. These explicit
conditions do not permit a general duration filter or relaxed boundary tolerance.

## Essential regressions

- Narrow double crossings, grazing extrema, tangencies, flat equality, jumps,
  and the smooth opposite-aliasing pulse visible to scan600 but missed by the
  earlier insufficiently validated fit.
- The approximately 53 ms Zodiacal rejection and 2.4006 s PALACE/Jones rejection,
  independently resolved at 1 ms; the 87.6283 s twilight rejection independently
  resolved at 10 ms.
- Accepted islands missed by scan120: approximately 7.1992 s (South, both lunar
  models), 9.8427 s (North K&S), and 13.7884 s (North Jones), with 1 ms references.
  Compact verified values are in [accepted-islands.json](adaptive-acceptance/accepted-islands.json).
- UTC midnight, adjacent split-time instants, Moon visibility inclusivity,
  accepted-side min/max bands, invalid model inputs, resource exhaustion and
  deterministic schedules/errors across Rayon worker counts.
- Exact cached/uncached component equality across both sites, both Moon models,
  all masks and repeated reads; distinct sub-microsecond keys and bounded capacity.

The 64-case seven-day boundary corpus validates all masks, sites and Moon models.
Its verified baseline has 716 accepted native UTC endpoints, 712 paired fine
boundaries, maximum matched error 0.41529231 s, two independently confirmed
reference misses, zero unpaired boundaries and zero rejected native endpoints.
[Boundary records](adaptive-acceptance/boundary-accuracy.jsonl) and their
[summary](adaptive-acceptance/boundary-accuracy-summary.json) retain the evidence.
This is observed corpus accuracy, not a global error theorem.

The Siderust 0.13-to-0.14 migration produced 67 identical dependency-reference
records (64 physical cases plus three temporal round trips). The immutable
[0.13 export](https://github.com/VPRamon/NSB/blob/954817f71f404f21fc9ff9fdd37c870506532c89/docs/specifications/adaptive-acceptance/dependency-A.jsonl)
and [0.14 export](https://github.com/VPRamon/NSB/blob/954817f71f404f21fc9ff9fdd37c870506532c89/docs/specifications/adaptive-acceptance/dependency-B.jsonl)
remain accessible in Git instead of duplicating 1.3 MB in the final tree.

## Reproduction

```bash
cargo test --release --locked -p nsb --lib planning::discovery_tests
cargo test --release --locked -p nsb --lib planning::strict_tests
cargo test --release --locked -p nsb --lib physical_boundary_accuracy_reference -- --ignored --nocapture
scripts/benchmark-adaptive-threshold.sh target/adaptive-matrix.log
```

Run native Rust, installed-wheel Python and CLI suites as well; their inward
projection contracts differ in representable precision. Full release evidence
and benchmark reproduction are recorded in [performance](adaptive-performance.md).
