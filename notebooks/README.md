# NSB observational validation notebooks

Two complementary executable notebooks ship with NSB v0.2.0:

| Notebook | Role |
|---|---|
| [`NSB_Validation.ipynb`](NSB_Validation.ipynb) | Standalone Patat/FORS1 four-component campaign (Automatic/Oxygen, explicit continuum, nsb2, baseline) |
| [`NSB_CTAO_Validation.ipynb`](NSB_CTAO_Validation.ipynb) | CTAO-oriented scientific article: physical checks, full four-component benchmark, OI gate, CTAO scenarios |

**Automatic Airglow (v0.2.0):** PALACE Oxygen (`ParanalPalaceOxygenV1`). Explicit continuum-only (`ParanalPalaceV1`) remains available. This default is a release-policy reference-model choice supported by physical completeness and retrospective Patat evidence; it is **not** an independent absolute observational admission. Protected LVM spectra remain sealed.

## CTAO package

```bash
# From the NSB repository root (pre-publication release candidate):
./ctao/reproduce.sh lightweight
./ctao/reproduce.sh full
```

`reproduce.sh` builds this checkout into a verified wheel + receipt, installs pinned nsb2 for full mode, and executes the CTAO notebook with `nbclient`.

Imported from [nsb-validation PR #23](https://github.com/VPRamon/nsb-validation/pull/23) at `9132acd34b7e2e9359a5664cc91b0943efd48b2a`, adapted so Automatic resolves to Oxygen. See [`../ctao/SOURCE.md`](../ctao/SOURCE.md).

## Standalone NSB_Validation.ipynb

Designed to run as a single file after installing an Oxygen-capable NSB wheel built from this branch (v0.2.0 is not yet on PyPI). See the notebook configuration cell. Use a fresh kernel when switching NSB builds.

## Scientific honesty

Patat informed Oxygen development → development evidence, not a new holdout. Historical continuum and Oxygen scores remain reproducible under explicit selection. Do not treat favorable Patat MAE as certification to change scientific admission gates.
