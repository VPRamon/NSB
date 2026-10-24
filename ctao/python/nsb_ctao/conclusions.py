"""Build the final scientific-evidence conclusions table from executed results."""

from __future__ import annotations

from typing import Any


def build_conclusions(
    *,
    physical: dict[str, Any] | None,
    oi_gate: dict[str, Any] | None,
    patat: dict[str, Any] | None,
    ctao: dict[str, Any] | None,
    nsb_version: str | None,
    four_component: dict[str, Any] | None = None,
) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []

    rows.append(
        {
            "question": "Is NSB numerically/physically consistent?",
            "dataset": "installed NSB package (CTAO site profiles)",
            "model_versions": nsb_version,
            "observable": "restricted-band airglow radiance / component summation",
            "quantitative_result": (
                "all_passed=" + str(physical.get("all_passed")) if physical else "not executed"
            ),
            "statistical_uncertainty": "N/A (numerical checks)",
            "independence_status": "N/A — implementation verification",
            "admission_status": "PASS" if physical and physical.get("all_passed") else "INCOMPLETE",
            "scientific_conclusion": (
                "Installed NSB exposes continuum and Oxygen models; Oxygen increases "
                "airglow in CTAO-band integrals; component sums match integrate_band."
                if physical and physical.get("all_passed")
                else "Physical checks incomplete or failed."
            ),
        }
    )

    if four_component:
        table = {r["configuration"]: r for r in four_component.get("primary_table", [])}
        rows.append(
            {
                "question": "How accurate is complete NSB vs Patat total radiance?",
                "dataset": "Patat 2008 FORS1 — 94 evaluation spectra / 17 nights",
                "model_versions": nsb_version,
                "observable": "four-component photon radiance 365–650 nm",
                "quantitative_result": {
                    k: {
                        "rel_mae": table[k]["mean_absolute_relative_error"],
                        "rmse": table[k]["rmse"],
                        "rel_mae_ci95": table[k]["relative_mae_ci95"],
                    }
                    for k in (
                        "NSB_continuum_f27",
                        "NSB_oxygen_f27",
                        "nsb2_f27",
                        "baseline",
                    )
                    if k in table
                },
                "statistical_uncertainty": "night-cluster bootstrap on evaluation nights",
                "independence_status": four_component.get("admission", {}).get(
                    "independence_status", "DEVELOPMENT_ONLY"
                ),
                "admission_status": "development_retrospective",
                "scientific_conclusion": (
                    "Full four-component predictions were executed on the registered "
                    "evaluation partition. Patat informed Oxygen development — not an "
                    "independent holdout. Compare Oxygen, continuum, nsb2 and baseline "
                    "under matched F10.7 using the primary table."
                ),
            }
        )
        rows.append(
            {
                "question": "Is NSB Oxygen comparable to nsb2 on this domain?",
                "dataset": "Patat evaluation set (matched F10.7)",
                "model_versions": nsb_version,
                "observable": "paired relative MAE difference",
                "quantitative_result": four_component.get("metrics", {})
                .get("paired_focus", {})
                .get("NSB_oxygen_f27_minus_nsb2_f27"),
                "statistical_uncertainty": "night-cluster bootstrap CI for paired difference",
                "independence_status": "DEVELOPMENT_ONLY",
                "admission_status": "diagnostic",
                "scientific_conclusion": (
                    "Report the paired CI; a CI that includes zero does not prove "
                    "equivalence or noninferiority without a preregistered margin."
                ),
            }
        )
    else:
        rows.append(
            {
                "question": "How accurate is complete NSB vs Patat total radiance?",
                "dataset": "Patat 2008 FORS1",
                "model_versions": nsb_version,
                "observable": "four-component 365–650 nm",
                "quantitative_result": "not executed in this run (lightweight or failed)",
                "statistical_uncertainty": None,
                "independence_status": "DEVELOPMENT_ONLY",
                "admission_status": "SKIPPED",
                "scientific_conclusion": (
                    "Full mode required for the primary accuracy claim."
                ),
            }
        )

    rows.append(
        {
            "question": "Do independent OI photometer data admit absolute Oxygen validation?",
            "dataset": oi_gate.get("dataset") if oi_gate else None,
            "model_versions": nsb_version,
            "observable": oi_gate.get("observable") if oi_gate else None,
            "quantitative_result": oi_gate.get("admission") if oi_gate else "not executed",
            "statistical_uncertainty": "absolute calibration sigma unavailable",
            "independence_status": "independent lineage (Abastumani), not Paranal training",
            "admission_status": oi_gate.get("admission") if oi_gate else "INCOMPLETE",
            "scientific_conclusion": (
                "Abastumani IQSY products FAIL absolute-equivalence gates required "
                "for default Airglow admission."
                if oi_gate
                else "OI gate not executed."
            ),
        }
    )

    if patat:
        rows.append(
            {
                "question": "Airglow-only supplement (not primary accuracy)",
                "dataset": "Patat development cohort",
                "model_versions": nsb_version,
                "observable": "airglow-only vs total observed (invalid as primary)",
                "quantitative_result": patat.get("scores"),
                "statistical_uncertainty": "see supplement",
                "independence_status": "DEVELOPMENT_ONLY",
                "admission_status": "supplement_only",
                "scientific_conclusion": (
                    "Demoted diagnostic; do not quote as full-model accuracy."
                ),
            }
        )

    rows.append(
        {
            "question": "CTAO-relevant rate impact of Oxygen?",
            "dataset": "Illustrative CTAO-N/S scenarios",
            "model_versions": nsb_version,
            "observable": "airglow band + toy R_pe",
            "quantitative_result": ctao.get("oxygen_effect_f107_130") if ctao else None,
            "statistical_uncertainty": "scenario spread vs F10.7; no camera calibration",
            "independence_status": "N/A — modeled scenarios",
            "admission_status": "ILLUSTRATIVE",
            "scientific_conclusion": (
                "Oxygen changes predicted airglow and illustrative photoelectron rates; "
                "not validated CTAO camera predictions. A rate change is not proof of "
                "improved observational accuracy."
                if ctao
                else "CTAO scenarios not executed."
            ),
        }
    )

    rows.append(
        {
            "question": "Does scientific admission authorize Oxygen as NSB default Airglow?",
            "dataset": "LVM gate + independent OI + Patat development",
            "model_versions": nsb_version,
            "observable": "absolute continuum-subtracted OI / sealed LVM",
            "quantitative_result": "LVM_ABSOLUTE_EQUIVALENCE=PENDING_EXTERNAL_EVIDENCE",
            "statistical_uncertainty": "N/A",
            "independence_status": "no admitted independent absolute holdout",
            "admission_status": "NO-GO",
            "scientific_conclusion": (
                "Scientific absolute admission remains NO-GO: no admitted independent "
                "absolute-equivalence holdout. Favorable Patat development MAE is not "
                "sufficient under the frozen protocol. Separately, NSB v0.2.0 adopts "
                "PALACE Oxygen as the Automatic/default reference Airglow by "
                "maintainer release policy (physical completeness + retrospective "
                "evidence); maturity stays Experimental and is not a validation PASS."
            ),
        }
    )
    return rows
