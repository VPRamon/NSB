#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Fit a research-only XP shape mixture on the approved 336–650 nm photon scale."""

import argparse
import itertools
import math
import os
import statistics
import subprocess
from collections import Counter, defaultdict
from pathlib import Path

from calibrate_starlight_bv import SPLITS, bin_for, json_bytes, metrics, sha, split_for
from prepare_starlight_bv import (
    DESIGN,
    DESIGN_SHA,
    GAIA,
    LABELS,
    REPO,
    XP,
    cache_input,
    csv_rows,
    csv_write,
    digest,
    freeze,
    pin,
)

from starlight_bv_normalization import (
    LIMIT_G,
    ROUTES,
    fit_envelopes,
    normalization_metrics,
)

SCALE = 1e-21  # J nm^-1 per integrated photon
SEED = 317
G_EDGES = [3.0, 14.0, 16.0, 22.0]
BP = [-0.5, 0.0, 0.35, 0.7, 1.05, 1.4, 1.75, 2.1, 2.5, 3.0, 4.0]
RP = [-0.5, 0.0, 0.35, 0.7, 1.05, 1.4, 1.75, 2.1, 2.5, 3.0]
PRIOR_SIZE = 40
XP_SCALE_SCENARIO = 0.02


def stratum(edges, x):
    return bin_for(edges, min(max(x, edges[0]), math.nextafter(edges[-1], -math.inf)))


def anchors(rows):
    grouped = [[] for _ in G_EDGES[:-1]]
    for row in rows:
        if not all(row[name] for name in ("g", "bp", "rp", "xp_photon_flux")):
            continue
        g = float(row["g"])
        photon = float(row["xp_photon_flux"])
        truth = tuple(float(row[key]) for key in LABELS[-2:])
        cov = tuple(float(row[key]) for key in ("stat_b_var", "stat_v_var"))
        if (
            g >= LIMIT_G
            or photon <= 3 * float(row["xp_photon_sigma"])
            or any(value <= 3 * math.sqrt(var) for value, var in zip(truth, cov))
        ):
            continue
        bp, rp = float(row["bp"]) - g, g - float(row["rp"])
        x, y = (value / (photon * SCALE) for value in truth)
        vector = (x, y, x * x, y * y, x * y)
        grouped[stratum(G_EDGES, g)].append(
            (int(row["galactic_nest16"]) // 256, bp, rp, vector, g)
        )
    return grouped


def pool(population, bp_node, rp_node):
    """Gaussian conditioning on observed colours; absent dimensions enter with unit weight."""
    sums = [[0.0] * 7 for _ in range(13)]
    for sky, bp, rp, values, _ in population:
        distance = 0.0
        if bp_node is not None:
            distance += ((bp - bp_node) / 0.26) ** 2
        if rp_node is not None:
            distance += ((rp - rp_node) / 0.23) ** 2
        weight = math.exp(-0.5 * min(distance, 60.0))
        for cell in (sums[sky], sums[-1]):
            cell[0] += weight
            cell[1] += weight * weight
            for i, val in enumerate(values, 2):
                cell[i] += weight * val
    return sums


def average(sums):
    return [val / sums[0] for val in sums[2:]]


def fit(grouped):
    # The pooled all-magnitude donor set supplies a prior for sparse colour tails.
    population = list(itertools.chain.from_iterable(grouped))
    all_sky = [[{} for _ in grouped] for _ in range(12)]
    blind = [statistics.mean(anchor[3][i] for anchor in population) for i in range(2)]
    for route, (uses_bp, uses_rp) in ROUTES.items():
        nodes = itertools.product(BP if uses_bp else [None], RP if uses_rp else [None])
        for bp, rp in nodes:
            control = average(pool(population, bp, rp)[-1])
            regional = [pool(donors, bp, rp) for donors in grouped]
            for mag, sums in enumerate(regional):
                global_for_mag = sums[-1]
                # Pool the all-G prior at weakly sampled colours or magnitude ends.
                allowance = min(1.0, global_for_mag[0] / PRIOR_SIZE)
                global_mean = average(global_for_mag) if global_for_mag[0] else control
                prior = [
                    allowance * a + (1 - allowance) * b
                    for a, b in zip(global_mean, control)
                ]
                alternatives = [
                    average(other[-1]) for other in regional if other[-1][0] >= 20.0
                ]
                for sky in range(12):
                    w, w2, *moments = sums[sky]
                    mean = [
                        (s + PRIOR_SIZE * p) / (w + PRIOR_SIZE)
                        for s, p in zip(moments, prior)
                    ]
                    x, y, xx, yy, xy = mean
                    effective = min(w, w * w / w2) if w2 else 0.0
                    # Difference between regional, all-sky and alternative magnitude priors.
                    transfer = [
                        max(
                            abs(mean[i] - a[i]) for a in [prior, control, *alternatives]
                        )
                        for i in range(2)
                    ]
                    m = dict(
                        mean=[x, y],
                        covariance=[xx - x * x, yy - y * y, xy - x * y],
                        transfer_sigma=transfer,
                        effective_local_sources=effective,
                    )
                    context = all_sky[sky][mag]
                    if route == "g_only":
                        context[route] = m
                    else:
                        context.setdefault(route, []).append(m)
    return list(itertools.chain.from_iterable(all_sky)), blind


def bracket(nodes, value):
    if value is None:
        return [(0, 1.0)]
    value = min(max(value, nodes[0]), nodes[-1])
    i = next((i for i, x in enumerate(nodes[1:]) if x > value), len(nodes) - 2)
    t = (value - nodes[i]) / (nodes[i + 1] - nodes[i])
    return [(i, 1 - t), (i + 1, t)]


def interpolate(contexts, route, g, bp, rp, pixel):
    mag = stratum(G_EDGES, g)
    slots = contexts[(len(G_EDGES) - 1) * (pixel // 256) + mag][route]
    if route == "g_only":
        return (
            slots["mean"],
            slots["covariance"][:2],
            slots["transfer_sigma"],
            slots["effective_local_sources"],
        )
    mix = []
    for (i, a), (j, b) in itertools.product(bracket(BP, bp), bracket(RP, rp)):
        index = i * len(RP) + j if route == "full" else i if route == "g_and_bp" else j
        mix.append((a * b, slots[index]))
    mean = [sum(w * m["mean"][i] for w, m in mix) for i in range(2)]
    var = [
        sum(w * (m["covariance"][i] + (m["mean"][i] - mean[i]) ** 2) for w, m in mix)
        for i in range(2)
    ]
    transfer = [sum(w * m["transfer_sigma"][i] for w, m in mix) for i in range(2)]
    return mean, var, transfer, sum(w * m["effective_local_sources"] for w, m in mix)


def shape_for(contexts, row, route):
    has_bp, has_rp = ROUTES[route]
    g = float(row["g"])
    bp = float(row["bp"]) - g if has_bp else None
    rp = g - float(row["rp"]) if has_rp else None
    pixel = int(row["galactic_nest16"])
    return interpolate(contexts, route, g, bp, rp, pixel)


def evaluate(rows, contexts, blind, g_support, normalization=None):
    groups = defaultdict(list)
    previous = defaultdict(list)
    photon_diagnostics = defaultdict(list)
    baseline = defaultdict(list)
    shape_only = defaultdict(list)
    normalization_only = defaultdict(list)
    nside16_sums = defaultdict(lambda: [[0.0, 0.0] for _ in range(3)])
    bright_cut = sorted(float(r["xp_photon_flux"]) for r in rows)
    p90 = bright_cut[math.ceil(0.9 * len(rows)) - 1]
    p99 = bright_cut[math.ceil(0.99 * len(rows)) - 1]
    support_counts = Counter()
    for row in rows:
        g = float(row["g"])
        pixel = int(row["galactic_nest16"])
        for route, (has_bp, has_rp) in ROUTES.items():
            if not row[route + "_photon"]:
                continue
            bp = float(row["bp"]) - g if has_bp else None
            rp = g - float(row["rp"]) if has_rp else None
            mean, var, transfer, local = shape_for(contexts, row, route)
            p, stat, sys = [
                float(row[route + "_" + name]) * SCALE
                for name in ("photon", "stat", "sys")
            ]
            if (bp is not None and not BP[0] <= bp <= BP[-1]) or (
                rp is not None and not RP[0] <= rp <= RP[-1]
            ):
                support = "colour_extrapolation"
            elif not g_support[0] <= g <= g_support[1]:
                support = "magnitude_extrapolation"
            elif local < 20:
                support = "sparse_xp_prior"
            else:
                support = (
                    "xp_colour_support" if route == "full" else "missing_colour_prior"
                )
            support_counts[route, support] += 1
            factor = (
                1
                if support == "xp_colour_support"
                else 1.5
                if support == "missing_colour_prior"
                else 2
            )
            point = [m * p for m in mean]
            truth = [float(row[name]) for name in LABELS[-2:]]
            if factor > 1:
                transfer = [max(t, math.sqrt(v)) for t, v in zip(transfer, var)]
            photon_sys = sys
            if normalization:
                outside_xp = support in (
                    "colour_extrapolation",
                    "magnitude_extrapolation",
                )
                fraction = (
                    normalization["extrapolation"]
                    if outside_xp
                    else normalization["magnitude_cells"][stratum(G_EDGES, g)]
                )[route]
                photon_sys = max(sys, p * fraction)
            stat_sigma = [
                math.sqrt((p * p + stat * stat) * var[i] + stat * stat * mean[i] ** 2)
                for i in range(2)
            ]

            def sigmas(scale):
                return [
                    math.hypot(
                        stat_sigma[i], scale * mean[i] + p * transfer[i] * factor
                    )
                    for i in range(2)
                ]

            entries = tuple(zip(truth, point, sigmas(photon_sys)))
            legacy_entries = tuple(zip(truth, point, sigmas(sys)))
            measured = float(row["xp_photon_flux"])
            measured_prediction = [m * measured * SCALE for m in mean]
            latitude = row.get("galactic_latitude")
            for name in (
                "all",
                f"sky_{pixel // 256}",
                f"g_{stratum(G_EDGES, g)}",
                f"support_{support}",
                "g_at_or_above_xp_limit" if g >= LIMIT_G else "g_below_xp_limit",
                f"colour_{stratum([-1.0, 0.7, 1.4, 8.0], float(row['bp']) - float(row['rp']))}",
                "reported_blend" if row.get("reported_blend") else "no_reported_blend",
                "galactic_latitude_unknown"
                if latitude is None
                else f"absolute_latitude_band_{stratum([0.0, 10.0, 30.0, 60.0, 90.0], abs(latitude))}",
                "photon_top_one_percent"
                if measured > p99
                else (
                    "photon_next_nine_percent"
                    if measured > p90
                    else "photon_lower_ninety_percent"
                ),
                "xp_photon_grid_snr_below_20"
                if measured < 20 * float(row["xp_photon_sigma"])
                else "xp_photon_grid_snr_at_least_20",
            ):
                groups[route, name].append(entries)
                previous[route, name].append(legacy_entries)
                photon_diagnostics[route, name].append(
                    (measured, p / SCALE, truth, measured_prediction, point)
                )
            baseline[route].append(tuple((t, p * m, 0.0) for t, m in zip(truth, blind)))
            shape_only[route].append(
                tuple((t, m, 0.0) for t, m in zip(truth, measured_prediction))
            )
            normalization_only[route].append(
                ((measured, p / SCALE, math.hypot(stat, sys) / SCALE),)
            )
            sums = nside16_sums[route, pixel]
            for i in range(2):
                sums[0][i] += truth[i]
                sums[1][i] += point[i]
            sums[2][i] += measured_prediction[i]
    return {
        "stratified": [
            {"route": r, "stratum": k, **metrics(v)}
            for (r, k), v in sorted(groups.items())
        ],
        "prior_uncertainty_stratified": [
            {"route": r, "stratum": k, **metrics(v)}
            for (r, k), v in sorted(previous.items())
        ],
        "photon_normalization_and_bias_decomposition": [
            {"route": r, "stratum": k, **normalization_metrics(v)}
            for (r, k), v in sorted(photon_diagnostics.items())
        ],
        "support": [
            {"route": r, "status": k, "sources": n}
            for (r, k), n in sorted(support_counts.items())
        ],
        "global_shape_control": {r: point_metrics(v) for r, v in baseline.items()},
        "shape_with_measured_photon_control": {
            r: point_metrics(v) for r, v in shape_only.items()
        },
        "integrated_photon_normalization_control": {
            r: {
                k: x
                for k, x in metrics(v)["bands_b_v"][0].items()
                if not k.endswith("_w_m2_nm")
            }
            for r, v in normalization_only.items()
        },
        "nside16_held_out_flux_sums": [
            {
                "route": r,
                "pixel": p,
                "reference": x,
                "predicted": y,
                "shape_with_measured_photons": z,
            }
            for (r, p), (x, y, z) in sorted(nside16_sums.items())
        ],
    }


def point_metrics(values):
    result = metrics(values)
    for band in result.get("bands_b_v", []):
        del band["coverage_68"], band["coverage_95"]
    return result


def reference_rows(references, crowding, latitudes=None):
    latitudes = latitudes or {}
    rows = csv_rows(references, (*LABELS, "xp_photon_flux", "xp_photon_sigma"))
    for row in rows:
        source = int(row["source_id"])
        row["reported_blend"] = source in crowding
        row["galactic_latitude"] = latitudes.get(source)
        yield row


def train(references, out, photometric, origin, crowding, latitudes=None):
    rows = reference_rows(references, crowding, latitudes)
    splits = defaultdict(list)
    ids = set()
    for row in rows:
        sid = int(row["source_id"])
        pixel = int(row["galactic_nest16"])
        if sid in ids or not 0 <= pixel < 3072:
            raise ValueError("duplicated Gaia source or invalid Galactic NESTED index")
        ids.add(sid)
        if not all(row[name] for name in ("g", "bp", "rp", "xp_photon_flux")):
            continue
        splits[split_for(SEED, pixel)].append(row)
    grouped = anchors(splits["training"])
    population = list(itertools.chain.from_iterable(grouped))
    if (
        len(population) < 128
        or sum(bool(group) for group in grouped) < 2
        or any(
            len(splits[s]) < 32 or len({r["galactic_nest16"] for r in splits[s]}) < 5
            for s in SPLITS
        )
    ):
        raise ValueError(
            "insufficient XP anchors or disjoint sky support to fit a research candidate"
        )
    contexts, blind = fit(grouped)
    g_support = [
        min(a[-1] for a in population),
        min(LIMIT_G, max(a[-1] for a in population)),
    ]
    # Estimate a regional ratio bias envelope on validation, before reading test scores.
    residuals = defaultdict(list)
    for row in splits["validation"]:
        scale = float(row["xp_photon_flux"]) * SCALE
        for route in ROUTES:
            mu, *_ = shape_for(contexts, row, route)
            residuals[route, int(row["galactic_nest16"]) // 256].append(
                [float(row[key]) / scale - x for key, x in zip(LABELS[-2:], mu)]
            )
    envelopes = {
        route: [
            max(
                abs(statistics.mean(x[i] for x in residuals[route, sky]))
                for sky in range(12)
                if residuals.get((route, sky))
            )
            for i in range(2)
        ]
        for route in ROUTES
    }
    for context in contexts:
        for route, table in context.items():
            for moment in table if isinstance(table, list) else [table]:
                moment["transfer_sigma"] = [
                    max(x, y)
                    for x, y in zip(moment["transfer_sigma"], envelopes[route])
                ]
    normalization, normalization_evidence = fit_envelopes(splits["validation"], G_EDGES)
    holdout = evaluate(splits["test"], contexts, blind, g_support, normalization)
    if {
        r["route"]
        for r in holdout["stratified"]
        if r["stratum"] == "all" and r["evaluated"] >= 32
    } != set(ROUTES):
        raise ValueError(
            "insufficient photometric normalization controls for at least one XP holdout route"
        )
    split_evidence = {}
    for role in SPLITS:
        split_evidence[role] = {
            "partition_id": role,
            "source_count": len(splits[role]),
            "source_ids_sha256": sha(
                json_bytes(sorted(int(r["source_id"]) for r in splits[role]))
            ),
            "sky_regions": [
                str(p)
                for p in sorted({int(r["galactic_nest16"]) for r in splits[role]})
            ],
        }
    report = dict(
        origin=origin,
        reference_sha256=digest(references),
        xp_design_sha256=DESIGN_SHA,
        photometric_sha256=digest(photometric),
        splits=split_evidence,
        fit_anchors=[len(g) for g in grouped],
        xp_g_support=g_support,
        g_edges=G_EDGES,
        bp_nodes=BP,
        rp_nodes=RP,
        validation_ratio_bias_envelopes=envelopes,
        validation_photon_normalization_envelopes=normalization_evidence,
        global_shape=blind,
        coverage_definition="Per-source fractions within 1 or 1.96 times total marginal prediction scale against measured XP. Shared terms are coherent scenarios; coverage is not a map-level significance or independent validation.",
        **holdout,
        classification="XP holdout; partial routes use artificial masks",
        crowding_proxy_available=bool(crowding),
        xp_scale_scenario={
            "fraction": XP_SCALE_SCENARIO,
            "basis": "Montegriffo et al. 2023 doi:10.1051/0004-6361/202243880, visible-wavelength external residual scale; correlated sensitivity scenario",
        },
        scientific_limitations=[
            "Missing-colour priors marginalize the sampled XP population; missingness can be informative in Gaia.",
            "Magnitude and colour extrapolation use the nearest supported anchor context and broaden transfer uncertainty.",
            "Faint, blended and naturally incomplete sources lack independent validation.",
            "Unresolved differential extinction and XP completeness can bias both colours.",
        ],
    )
    report_path = out / "shape-held-out.json"
    freeze(report_path, json_bytes(report))
    artifact = dict(
        schema_version=3,
        calibration_status="research_candidate",
        normalization_envelope=normalization,
        method="xp-photon-normalized-population-mixture-v1",
        reference_nm=[445, 551],
        shape_unit="1e-21 J nm-1 photon-1",
        xp_design_sha256=DESIGN_SHA,
        xp_shared_calibration_fraction=[XP_SCALE_SCENARIO] * 2,
        photometric_sha256=digest(photometric),
        references_sha256=digest(references),
        held_out_report_sha256=digest(report_path),
        software_commit=subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=REPO, text=True
        ).strip(),
        partitions={
            **split_evidence,
            "assignment_algorithm": "sha256-galactic-nested-nside16-mod10-v1",
            "seed": SEED,
            "manifest_sha256": digest(references),
            "source_disjoint": True,
            "sky_disjoint": True,
            "disjointness_evidence_sha256": sha(json_bytes(split_evidence)),
        },
        g_edges=G_EDGES,
        xp_g_support=g_support,
        bp_nodes=BP,
        rp_nodes=RP,
        contexts=contexts,
    )
    path = out / "bv-shape.json"
    freeze(path, json_bytes(artifact))
    return path


def prepare(
    out,
    binary,
    photometric,
    *,
    cache=None,
    partitions=(),
    xp=None,
    gaia=None,
    parent=None,
    random_limit=6_000_000,
):
    out.mkdir(parents=True, exist_ok=True)
    pin(DESIGN, DESIGN_SHA)
    crowding = set()
    latitudes = {}
    if cache:
        gaia_rows, xp_rows, census, pins = cache_input(
            out, cache, partitions, random_limit, stellar_only=True
        )
        gaia, xp = out / "gaia.csv", out / "continuous.csv"
        crowding = {
            sid
            for sid, r in gaia_rows.items()
            if any(
                int(r[f"phot_{b}_n_blended_transits"] or 0) > 0 for b in ("bp", "rp")
            )
        }
        latitudes = {sid: float(r["b"]) for sid, r in gaia_rows.items() if r["b"]}
        csv_write(gaia, GAIA, (gaia_rows[i] for i in sorted(gaia_rows)))
        csv_write(xp, XP, (xp_rows[i] for i in sorted(xp_rows)))
        origin = dict(
            mode="verified ESA bulk cache",
            partitions=partitions,
            random_index_limit=random_limit,
            input_pins=pins,
            catalogue_comparison=census,
        )
    else:
        origin = dict(
            mode="matched XP/Gaia extracts",
            inputs={str(p): digest(p) for p in (xp, gaia)},
        )
        if parent:
            for r in csv_rows(
                parent,
                (
                    "source_id",
                    "phot_bp_n_blended_transits",
                    "phot_rp_n_blended_transits",
                ),
            ):
                if r.get("b"):
                    latitudes[int(r["source_id"])] = float(r["b"])
                if any(
                    int(r[f"phot_{b}_n_blended_transits"] or 0) > 0
                    for b in ("bp", "rp")
                ):
                    crowding.add(int(r["source_id"]))
            origin["parent_sha256"] = digest(parent)
    request = dict(
        origin=origin,
        xp=digest(xp),
        gaia=digest(gaia),
        binary=digest(binary),
        fit_source=digest(Path(__file__)),
        normalization_source=digest(
            Path(__file__).with_name("starlight_bv_normalization.py")
        ),
        photometric=digest(photometric),
    )
    freeze(out / "shape-inputs.json", json_bytes(request))
    refs = out / "shape-references.csv"
    if not refs.exists():
        subprocess.run(
            [
                binary,
                "dataset",
                "starlight",
                "diagnose",
                "bv-xp",
                "--xp-continuous",
                xp,
                "--gaia-source",
                gaia,
                "--output",
                refs,
                "--shape-photometric",
                photometric,
                "--shape-photometric-sha256",
                digest(photometric),
            ],
            check=True,
            cwd=REPO,
            env={**os.environ, "STARLIGHT_GAIAXPY_DESIGN_FIXTURE": str(DESIGN)},
        )
    freeze(out / "shape-reference-sha256.json", json_bytes({"sha256": digest(refs)}))
    return train(refs, out, photometric, origin, crowding, latitudes)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ("out", "nsb-data", "photometric", "xp", "gaia", "parent"):
        parser.add_argument(
            "--" + arg,
            type=lambda p: Path(p).resolve(),
            required=arg in ("out", "nsb-data", "photometric", "xp", "gaia"),
        )
    args = parser.parse_args()
    if args.out.is_relative_to(REPO):
        parser.error("write scientific data outside the repository")
    print(
        prepare(
            args.out,
            args.nsb_data,
            args.photometric,
            xp=args.xp,
            gaia=args.gaia,
            parent=args.parent,
        )
    )


if __name__ == "__main__":
    main()
