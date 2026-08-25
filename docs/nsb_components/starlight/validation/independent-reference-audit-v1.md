# Independent-reference technical audit (issue #102)

Status: Frozen technical evidence for human review on issue #103.
Audience: Scientific reviewer and maintainers.
Non-goals: This document does not approve the candidate, retune tolerances,
or invent a starlight-only comparison grid.

## Question

Is there a documented, machine-readable, starlight-only top-of-atmosphere
300–650 nm product that can be transformed onto the frozen UV-v2 candidate
without unpublished amplitudes, contamination subtraction, or treating a
Gaia-derived map as independent ground truth?

## Registered and acquired references

| id | Machine product | Admissibility |
| --- | --- | --- |
| `toller-1981-pioneer-background-starlight` | Pioneer 10 pole transcription | Not admissible: ISL+DGL+EBL in a 2.3° FOV; DGL inseparable |
| `leinert-1998-diffuse-night-sky-brightness` | Paper PDF / table transcription | Not admissible: 2D Gaussian ISL model without published amplitudes/widths |
| `masana-2021-gambons-gaia-hipparcos-starlight` | arXiv PDF, not a HEALPix grid | Not admissible: mixed ISL+DGL+EBL+ZL+airglow; Gaia-dependent |

No fourth `[[references]]` row exists. The registry minimum of two candidates
is met. Transformations that would reconstruct unpublished Leinert Gaussians,
subtract unmeasured DGL/EBL/ZL/airglow, or treat GAMBONS as Gaia-free truth
are out of scope.

## Other documented candidates considered and not acquired

These appear in historical notes (`existing-datasets.md`, validation README,
science-requirements) but are not admissible starlight-only TOA 300–650 nm
grids for this candidate:

- Manual Tycho/Hipparcos experimental seed (`starlight_manual_seed_v1.csv`): runtime fixture, not independent validation.
- Retired nside 64/256/512 derived maps: not independent source-level products.
- GAMBONS website all-sky maps: explicitly mixed night-sky products.
- Toller 1981 dissertation / Toller et al. 1987 / Weinberg 1974: no additional machine-readable starlight-only table beyond the acquired pole transcription.
- Mattila DGL programmes, Besançon, TRILEGAL, Hipparcos-only ISL, Tycho-only ISL: no in-repo machine-readable starlight-only 300–650 nm HEALPix/table product with published provenance that isolates direct Galactic starlight at TOA.

## Outcome

`independent_reference_status = no_admissible_independent_reference`

`technical_gates_passed` remains false because no admissible transformed grid
exists. That is not a software failure and must not be labelled as pending
acquisition. Issue #103 decides whether the remaining evidence is sufficient
for production use.
