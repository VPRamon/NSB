#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Assemble, audit and fit a Starlight B/V handoff outside the checkout."""

import argparse
from collections import Counter
import concurrent.futures
import csv
import fcntl
import gzip
import hashlib
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import tomllib
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import urlopen

from calibrate_starlight_bv import EDGES, ROUTES, SUPPORT, cells_for, json_bytes, load_policy, write_json


TOOLS = Path(__file__).resolve().parent
REPO = TOOLS.parents[2]
CONFIG = TOOLS.parent / "config/starlight-production-300-650.ladon.toml"
DESIGN = TOOLS.parent / "tests/fixtures/gaiaxpy_continuous_design_v375wi_v142r.json"
DESIGN_SHA = "e8dc899916ed29d4a92ee6ff7ec3d88a85f9c26d9c7e375ef6a6b3dd76666539"
TAP = "https://gea.esac.esa.int/tap-server/tap/sync"
XP_TAP = "https://gaia.ari.uni-heidelberg.de/tap/sync"
GAIA = ("source_id", "ra", "dec", "phot_g_mean_mag", "phot_bp_mean_mag", "phot_rp_mean_mag")
PARENT = (*GAIA[:3], "l", "b", *GAIA[3:], "has_xp_continuous", "duplicated_source",
          "phot_bp_n_blended_transits", "phot_rp_n_blended_transits", "random_index")
NULLABLE_PHOTOMETRY = (*GAIA[3:], "phot_bp_n_blended_transits", "phot_rp_n_blended_transits")
XP = ("source_id", *(f"{band}_{field}" for band in ("bp", "rp") for field in (
    "n_parameters", "standard_deviation", "coefficients", "coefficient_errors",
    "coefficient_correlations",
)))
LABELS = ("source_id", "galactic_nest16", "g", "bp", "rp", "b_w_m2_nm", "v_w_m2_nm")
DIAGNOSTIC = {
    "seed": 317, "sky_nside": 1,
    "g_edges": [3, 15, 17.65, 21.5],
    "bp_g_edges": [-1, 0.60, 6], "g_rp_edges": [-1, 0.85, 5],
    "min_training": 25, "min_validation": 10, "min_validation_sky_cells": 5,
}


def digest(path, algorithm="sha256"):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, algorithm).hexdigest()


def pin(path, expected, algorithm="sha256"):
    actual = digest(path, algorithm)
    if actual != expected:
        raise ValueError(f"{algorithm} mismatch for {path}: expected {expected}, found {actual}")


def freeze(path, data):
    if path.exists():
        if path.read_bytes() != data:
            raise ValueError(f"{path} differs from frozen input; use a new output directory")
    else:
        write_json(path, data)


def csv_rows(path, columns, *, compressed=False):
    opener = gzip.open if compressed else open
    with opener(path, "rt", encoding="utf-8", newline="") as stream:
        reader = csv.DictReader(line for line in stream if not line.startswith("#"))
        if not reader.fieldnames or not set(columns) <= set(reader.fieldnames):
            raise ValueError(f"missing CSV columns in {path}: {columns}")
        if len(reader.fieldnames) != len(set(reader.fieldnames)):
            raise ValueError(f"duplicate CSV columns in {path}")
        for row in reader:
            if None in row or any(row[key] is None for key in columns):
                raise ValueError(f"malformed row in {path}: {reader.line_num}")
            yield row


def source_id(row):
    value = int(row["source_id"])
    if not 0 < value < 2**63:
        raise ValueError(f"invalid Gaia source ID {value}")
    return value


def gaia_parent_rows(path, *, compressed=False):
    for row in csv_rows(path, PARENT, compressed=compressed):
        # ESA bulk CSV uses literal null where TAP CSV uses empty fields.
        for name in NULLABLE_PHOTOMETRY:
            if row[name] == "null":
                row[name] = ""
        yield row


def by_id(rows):
    matched = {}
    for row in rows:
        key = source_id(row)
        if key in matched:
            raise ValueError(f"duplicate Gaia source ID {key}")
        matched[key] = row
    return matched


def csv_write(path, fields, rows):
    with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", newline="",
                                     dir=path.parent, delete=False) as stream:
        temporary = Path(stream.name)
        try:
            writer = csv.DictWriter(stream, fields, extrasaction="ignore", lineterminator="\n")
            writer.writeheader()
            writer.writerows(rows)
            stream.close()
            temporary.replace(path)
        finally:
            temporary.unlink(missing_ok=True)


def retrieve(path, url, **parameters):
    if path.exists():
        return
    # Retain only completed responses. Each successful batch is reusable.
    request_url = f"{url}?{urlencode(parameters)}" if parameters else url
    for attempt in range(3):
        try:
            with urlopen(request_url, timeout=150) as request:
                data = request.read()
            write_json(path, data)
            return
        except (URLError, TimeoutError) as error:
            if attempt == 2 or (isinstance(error, HTTPError) and error.code < 500 and error.code != 429):
                raise
            time.sleep(2**attempt)


def gaia_flag(row, column):
    value = row[column].strip().lower()
    if value not in ("true", "false"):
        raise ValueError(f"invalid Gaia {column} flag for source {row['source_id']}: {value!r}")
    return value == "true"


def selected(row):
    return (bool(row["phot_g_mean_mag"]) and gaia_flag(row, "has_xp_continuous")
            and not gaia_flag(row, "duplicated_source"))


def census(rows):
    counts = Counter()
    for row in rows:
        if gaia_flag(row, "duplicated_source") or not row["phot_g_mean_mag"]:
            continue
        route = ROUTES[bool(row["phot_bp_mean_mag"]), bool(row["phot_rp_mean_mag"])]
        g = float(row["phot_g_mean_mag"])
        magnitude = "g<15" if g < 15 else "15<=g<17.65" if g < 17.65 else "g>=17.65"
        sky = "|b|<10" if abs(float(row["b"])) < 10 else "|b|>=10"
        blended = any(int(row[f"phot_{band}_n_blended_transits"] or 0) > 0 for band in ("bp", "rp"))
        counts[gaia_flag(row, "has_xp_continuous"), route, magnitude, sky, blended] += 1
    return [{"has_xp_continuous": xp, "route": route, "magnitude": g, "sky": sky,
             "reported_blended_bp_or_rp_transits": blended, "sources": count}
            for (xp, route, g, sky, blended), count in sorted(counts.items())]


def public_input(root, limit):
    query = ("SELECT " + ", ".join(PARENT) + "\nFROM gaiadr3.gaia_source\n"
             f"WHERE random_index < {limit}\nORDER BY random_index\n")
    freeze(root / "parent.adql", query.encode())
    parent = root / "parent.csv"
    retrieve(parent, TAP, REQUEST="doQuery", LANG="ADQL", FORMAT="csv", MAXREC=limit, QUERY=query)
    rows = list(gaia_parent_rows(parent))
    if len(rows) != limit or sorted(int(row["random_index"]) for row in rows) != list(range(limit)):
        raise ValueError("Gaia TAP comparison sample is truncated or has inconsistent random indices")
    gaia = by_id(row for row in rows if selected(row))
    if not gaia:
        raise ValueError("Gaia TAP sample contains no eligible XP IDs")
    batch_root = root / "heidelberg-xp"
    batch_root.mkdir(exist_ok=True)
    ids = list(gaia)
    batch_size = 40

    def batch(start):
        batch_ids = ids[start:start + batch_size]
        path = batch_root / f"{start // batch_size:04d}.csv"
        query = ("SELECT " + ", ".join(XP) + " FROM gaiadr3.xp_continuous_mean_spectrum "
                 f"WHERE source_id IN ({','.join(map(str, batch_ids))}) ORDER BY source_id")
        query_path = path.with_suffix(".adql")
        freeze(query_path, (query + "\n").encode())
        retrieve(path, XP_TAP, REQUEST="doQuery", LANG="ADQL", FORMAT="csv",
                 MAXREC=len(batch_ids), QUERY=query)
        records = by_id(csv_rows(path, XP))
        if records.keys() != set(batch_ids):
            raise ValueError(f"Heidelberg TAP response does not match requested IDs: {path}")
        return path, records

    batches = []
    xp = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for path, records in pool.map(batch, range(0, len(ids), batch_size)):
            xp.update(records)
            batches.extend((path, path.with_suffix(".adql")))
    files = [root / "parent.adql", parent, *batches]
    return gaia, xp, census(rows), {str(p.relative_to(root)): digest(p) for p in files}


def cache_input(root, cache, partitions, limit):
    """Use the approved pinned raw inventories to verify cached object identities."""
    config = tomllib.loads(CONFIG.read_text())
    objects = {}
    pins = {}
    for product in config["starlight"]["gaia_products"]:
        name = product["id"]
        manifest = root / f"{name}.md5"
        retrieve(manifest, product["checksum_manifest_url"])
        pin(manifest, product["checksum_manifest_sha256"])
        pins[manifest.name] = digest(manifest)
        official = {}
        for line in manifest.read_text().splitlines():
            fields = line.split()
            if len(fields) == 2:
                official[Path(fields[1].lstrip("*")).name] = fields[0]
        for partition in partitions:
            receipt_path = cache / "cache/receipts" / name / f"{partition}.json"
            receipt = json.loads(receipt_path.read_text())
            filename = product["filename_prefix"] + partition + product["filename_suffix"]
            expected_md5 = official.get(filename)
            expected_sha = receipt["sha256"]
            if (not expected_md5 or receipt["schema_version"] != 1
                    or receipt["filename"] != filename or receipt["product_id"] != name
                    or receipt["partition_id"] != partition
                    or receipt["source_url"] != product["base_url"] + filename
                    or receipt["official_checksum_algorithm"] != "md5"
                    or receipt["official_checksum"] != expected_md5
                    or len(expected_sha) != 64 or any(c not in "0123456789abcdef" for c in expected_sha)):
                raise ValueError(f"receipt disagrees with approved Gaia inventory: {receipt_path}")
            path = cache / "cache/objects/sha256" / expected_sha
            if not path.is_file():
                path = Path(receipt["object_path"])
            if path.stat().st_size != receipt["bytes"]:
                raise ValueError(f"cached Gaia object has wrong size: {path}")
            pin(path, expected_sha)
            pin(path, expected_md5, "md5")
            objects[name, partition] = path
            pins[str(path)] = expected_sha
            pins[str(receipt_path)] = digest(receipt_path)

    gaia = {}
    xp = {}
    parent_rows = []
    for partition in partitions:
        rows = by_id(row for row in gaia_parent_rows(objects["gaia-source", partition], compressed=True)
                     if int(row["random_index"]) < limit)
        parent_rows.extend(rows.values())
        wanted = {sid for sid, row in rows.items() if selected(row)}
        records = by_id(row for row in csv_rows(objects["xp-continuous", partition], XP, compressed=True)
                        if source_id(row) in wanted)
        if records.keys() != wanted or xp.keys() & wanted:
            raise ValueError(f"missing or duplicate selected XP IDs in partition {partition}")
        xp.update(records)
        gaia.update({sid: rows[sid] for sid in wanted})
    if not xp:
        raise ValueError("no XP IDs hit the random-index cut; increase --random-index-limit")
    return gaia, xp, census(parent_rows), pins


def read_review(path, policy):
    review = json.loads(path.read_text())
    if not isinstance(review, dict):
        raise ValueError("review must be a JSON object")
    inputs = {}
    for name in ("gaia_source", "spectral_reference", "target_population_evidence",
                 "reference_evidence", "xp_absolute_calibration_evidence"):
        entry = review[name]
        source = (path.parent / entry["path"]).resolve()
        pin(source, entry["sha256"])
        inputs[name] = source
    for name in ("selection_and_crossmatch", "wavelength_operator", "uncertainty_model",
                 "reviewer"):
        if not isinstance(review.get(name), str) or not review[name].strip():
            raise ValueError(f"review requires {name}")
    if (review["measurement"] != {"vacuum_nm": [445, 551], "unit": "W m-2 nm-1"}
            or review.get("reference_independent_of_gaia_xp_and_photometry") is not True):
        raise ValueError("candidate requires independent vacuum monochromatic energy flux density")
    settings = {*EDGES, *SUPPORT, "seed", "sky_nside", "xp_shared_calibration_fraction"}
    if not isinstance(review["policy"], dict) or review["policy"].keys() != settings:
        raise ValueError("review must predeclare every policy setting and the shared XP fractions")
    policy.update(review["policy"])
    policy["reference_provenance"] = (
        f"Reviewed independent reference and population evidence, review SHA-256 {digest(path)}"
    )
    thresholds = review["acceptance"]
    if (type(thresholds["min_test_per_cell"]) is not int or thresholds["min_test_per_cell"] < 2
            or not all(type(thresholds[key]) in (float, int) and math.isfinite(thresholds[key])
                       and 0 < thresholds[key] <= 1 for key in
                       ("max_abs_bias", "max_p95_absolute_relative_error", "min_coverage_68",
                        "min_coverage_95"))):
        raise ValueError("invalid predeclared acceptance thresholds")
    return inputs, thresholds


def check_candidate(report, thresholds, target):
    dimensions = ("route", "galactic_nested_pixel", "g_bin", "bp_g_bin", "g_rp_bin")
    actual = {tuple(row[key] for key in dimensions): row for row in report["cells"]}
    required = {tuple(row[key] for key in dimensions) for row in json.loads(target.read_text())["required_cells"]}
    if not required or report["test_sources_without_bins"] or required - actual.keys():
        raise ValueError("reference labels do not cover the reviewed target population cells")
    for key, row in actual.items():
        if not row["fitted"] or row["evaluated"] < thresholds["min_test_per_cell"]:
            raise ValueError(f"insufficient reference support for calibration cell {key}")
        for band in row["bands_b_v"]:
            bias = band["relative_bias_sum"]
            p95 = band["absolute_relative_error_percentiles"]["95"]
            if (bias is None or p95 is None or abs(bias) > thresholds["max_abs_bias"]
                    or p95 > thresholds["max_p95_absolute_relative_error"]
                    or band["coverage_68"] < thresholds["min_coverage_68"]
                    or band["coverage_95"] < thresholds["min_coverage_95"]):
                raise ValueError(f"held-out acceptance failed for calibration cell {key}")


def file_checksums(root):
    for name in ("README.txt", "bv-calibration.json", "held-out.json", "label-covariance.csv",
                 "labels.csv", "policy.json", "provenance.json"):
        path = root / name
        if path.is_file():
            yield f"{digest(path)}  {path.name}\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--nsb-data", type=Path, required=True)
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--public", action="store_true", help="ESA archive XP diagnostic")
    source.add_argument("--cache", type=Path, help="existing NSB Gaia cache workspace")
    source.add_argument("--xp-extract", type=Path, help="manifest of two existing matched Gaia/XP CSVs")
    source.add_argument("--review", type=Path, help="pinned independent references and review")
    parser.add_argument("--partition", action="append", default=[])
    parser.add_argument("--random-index-limit", type=int)
    args = parser.parse_args()
    output = args.out.resolve()
    if output.is_relative_to(REPO) or REPO.is_relative_to(output):
        parser.error("use a separate output directory outside the repository")
    if bool(args.partition) != bool(args.cache):
        parser.error("--cache requires one or more --partition values")
    partitions = sorted(set(args.partition))
    if any(len(p) != 13 or not p[:6].isdigit() or p[6] != "-" or not p[7:].isdigit()
           for p in partitions):
        parser.error("expected partition identifiers in NNNNNN-NNNNNN format")
    limit = args.random_index_limit
    if limit is None:
        limit = 45_000 if args.public else 15_000_000
    if limit <= 0 or (args.public and limit > 100_000):
        parser.error("invalid random-index limit (public TAP supports up to 100,000 here)")

    output.mkdir(parents=True, exist_ok=True)
    inputs_root = output / "inputs"
    inputs_root.mkdir(exist_ok=True)
    with (inputs_root / ".lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        prepare(args, inputs_root, partitions, limit)


def prepare(args, inputs_root, partitions, limit):
    output = inputs_root.parent
    pin(DESIGN, DESIGN_SHA)
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
    build = args.nsb_data.resolve()
    policy = dict(DIAGNOSTIC)
    review_path = args.review.resolve() if args.review else None
    local_path = args.xp_extract.resolve() if args.xp_extract else None
    freeze(inputs_root / "request.json", json_bytes({
        "commit": commit, "binary_sha256": digest(build), "public": args.public,
        "cache": str(args.cache.resolve()) if args.cache else None,
        "review_sha256": digest(review_path) if review_path else None,
        "extract_sha256": digest(local_path) if local_path else None,
        "partitions": partitions, "random_index_limit": limit if args.public or args.cache else None,
    }))
    comparison = None
    if review_path:
        evidence, thresholds = read_review(review_path, policy)
        gaia_path = evidence["gaia_source"]
        spectral_path = evidence["spectral_reference"]
        spectral_arg = "--spectral-reference"
        source_pins = {str(p): digest(p) for p in (review_path, *evidence.values())}
    elif local_path:
        extract = json.loads(local_path.read_text())
        if not isinstance(extract.get("origin"), str) or not extract["origin"].strip():
            raise ValueError("existing XP extracts require a reproducible origin")
        paths = {}
        for key in ("gaia_source", "xp_continuous"):
            entry = extract[key]
            paths[key] = (local_path.parent / entry["path"]).resolve()
            pin(paths[key], entry["sha256"])
        gaia_path, spectral_path = paths["gaia_source"], paths["xp_continuous"]
        source_pins = {str(p): digest(p) for p in (local_path, *paths.values())}
        spectral_arg = "--xp-continuous"
    else:
        if args.public:
            gaia, xp, comparison, source_pins = public_input(inputs_root, limit)
        else:
            gaia, xp, comparison, source_pins = cache_input(inputs_root, args.cache.resolve(), partitions, limit)
        gaia_path, spectral_path = inputs_root / "gaia.csv", inputs_root / "continuous.csv"
        csv_write(gaia_path, GAIA, (gaia[sid] for sid in sorted(gaia)))
        for row in xp.values():
            for key in XP:
                value = row[key]
                if value.startswith("{") and value.endswith("}"):
                    row[key] = "[" + value[1:-1] + "]"
        csv_write(spectral_path, XP, (xp[sid] for sid in sorted(xp)))
        spectral_arg = "--xp-continuous"
    if not review_path:
        policy["reference_provenance"] = (
            "Diagnostic Gaia DR3 XP coefficient-space reconstruction; no independent reference. "
            "See provenance.json for selection, input hashes and units."
        )

    policy.update(software_commit=commit, xp_design_sha256=DESIGN_SHA)
    policy_path = output / "policy.json"
    freeze(policy_path, json_bytes(policy))
    load_policy(policy_path, exporting=bool(review_path))
    input_pins = {"gaia_source_sha256": digest(gaia_path), "spectral_input_sha256": digest(spectral_path)}
    freeze(inputs_root / "input-pins.json", json_bytes(input_pins))

    audit_path = output / "label-covariance.csv"
    subprocess.run([build, "dataset", "starlight", "diagnose", "bv-xp",
                    spectral_arg, spectral_path, "--gaia-source", gaia_path, "--output", audit_path],
                   check=True, cwd=REPO, env={**os.environ, "STARLIGHT_GAIAXPY_DESIGN_FIXTURE": str(DESIGN)})
    labels_path = output / "labels.csv"
    rows = list(csv_rows(audit_path, LABELS))
    csv_write(labels_path, LABELS, rows)
    routes = Counter(ROUTES[bool(row["bp"]), bool(row["rp"])] for row in rows if row["g"])
    report_path = output / "held-out.json"
    command = [sys.executable, str(TOOLS / "calibrate_starlight_bv.py"),
               str(labels_path), str(policy_path), str(report_path)]
    rejection = None
    candidate = output / "bv-calibration.json"
    staged = inputs_root / candidate.name
    staged.unlink(missing_ok=True)
    try:
        if review_path:
            command += ["--candidate-calibration", str(staged)]
        report_path.unlink(missing_ok=True)
        result = subprocess.run(command, check=False)
        if not report_path.is_file():
            raise ValueError(f"calibrator failed before writing the held-out report: {result.returncode}")
        report = json.loads(report_path.read_text())
        if report["policy_sha256"] != digest(policy_path) or report["references_sha256"] != digest(labels_path):
            raise ValueError("report does not describe the current handoff")
        if review_path:
            try:
                if result.returncode or not staged.is_file():
                    raise ValueError("calibrator did not export a candidate; inspect held-out.json")
                if any(not list(cells_for(row, policy, False)) for row in rows):
                    raise ValueError("at least one reference is outside the predeclared policy domain")
                check_candidate(report, thresholds, evidence["target_population_evidence"])
            except ValueError as error:
                rejection = str(error)
            else:
                staged.replace(candidate)
        elif result.returncode:
            raise ValueError(f"diagnostic calibrator failed: {result.returncode}")
    finally:
        staged.unlink(missing_ok=True)
    write_json(output / "provenance.json", json_bytes({
        "status": "rejected" if rejection else "reviewed-candidate" if review_path else "diagnostic",
        "scientific_rejection": rejection,
        "software_commit": commit, "binary_sha256": digest(build),
        "workflow_sha256": digest(Path(__file__)), "calibrator_sha256": digest(TOOLS / "calibrate_starlight_bv.py"),
        "xp_design_sha256": DESIGN_SHA, "references_nm_vacuum": [445, 551], "flux_unit": "W m-2 nm-1",
        **input_pins,
        "public_endpoints": {"gaia_source": TAP, "xp_continuous": XP_TAP} if args.public else None,
        "source_pins": source_pins, "source_count": len(rows),
        "label_routes": {route: routes[route] for route in ROUTES.values()},
        "gaia_comparison_counts": comparison, "calibration_command": command,
        "independent_label_noise": "Label covariance retained for audit; the fitter retains label noise in residual scatter.",
    }))
    status = ("Candidate rejected: " + rejection if rejection else
              "Reviewed candidate, pending map-level validation" if review_path else
              "Gaia XP diagnostic. Production calibration remains unapproved.")
    readme = (
        "NSB Starlight B/V calibration handoff\n"
        f"{status}\n\n"
        f"Software {commit}\nXP design SHA-256 {DESIGN_SHA}\n"
        f"Reference rows {len(rows)}\n"
        + "\n".join(f"{route}: {routes[route]}" for route in ROUTES.values())
        + "\n\nlabels.csv contains observations and signed vacuum 445/551 nm flux densities "
        "in W m-2 nm-1. label-covariance.csv preserves both variances and the "
        "signed cross term in squared units. policy.json was frozen before fitting. "
        "held-out.json reports spatial support, prediction errors and interval coverage. "
        "provenance.json records input checksums, origin and software.\n"
        + ("The external reference and absolute scale evidence are pinned by the review.\n" if review_path else
           "The references are Gaia DR3 continuous XP. Independent spectrophotometry "
           "and absolute standards are required to estimate transfer to the faint, crowded and "
           "naturally incomplete population and to pin the shared XP calibration term.\n")
        + "\nRun sha256sum -c checksums.sha256 in this directory. See the Ladon procedure in "
        "docs/nsb_components/starlight/bv-photometry-317.md in PR #319.\n"
    )
    write_json(output / "README.txt", readme.encode())
    write_json(output / "checksums.sha256", "".join(file_checksums(output)).encode())
    if rejection:
        raise ValueError(rejection)
    if review_path:
        print(f'[starlight.bv_photometry]\nartifact_path = "{candidate}"\nsha256 = "{digest(candidate)}"')
    else:
        print(f"Diagnostic B/V handoff: {output}; {len(rows)} labels; calibration remains unapproved")


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, TypeError, ValueError, subprocess.CalledProcessError) as error:
        sys.exit(f"Starlight B/V: {error}")
