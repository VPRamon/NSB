#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
"""Prepare, submit, resume and verify the separate Starlight B/V candidate on Ladon."""

import argparse
import fcntl
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

import tomllib
from calibrate_starlight_bv import json_bytes
from fit_starlight_bv_shape import prepare
from prepare_starlight_bv import (
    CONFIG,
    DESIGN,
    DESIGN_SHA,
    REPO,
    digest,
    freeze,
    pin,
    retrieve,
)

DATA_ROOT = Path("/mnt/beegfs/valles/nsb-data")
BRIGHT = CONFIG.with_name("starlight-bright-stars-combined-v1.ladon.toml")
BASELINE = REPO / "crates/nsb/data/starlight_nside128.compact.release.bin"
BASELINE_SHA = "11479bd0b6838d06df884b76dd74575bbc42d424ec85defecf847257c0f64d7f"
BASELINE_REPORT = BASELINE.with_name("merge_report.json")
BASELINE_REPORT_SHA = "60a18ca8b92b6587ce23ffff718b115a22e38e67c2c6c952bc568dbfb7b0063c"
INTEGRATED = ("ultraviolet_correction", "photometric_inference", "selection_function")
ARTIFACT = "starlight-bright-stars-v1.json"
MANIFEST = "starlight-bright-stars-v1-build-manifest.json"


def run(*argv, capture=False, env=None):
    return subprocess.run(
        argv,
        cwd=REPO,
        text=True,
        check=True,
        env=env,
        stdout=subprocess.PIPE if capture else None,
    ).stdout


def read_json(path):
    return json.loads(path.read_text())


def path_literal(path):
    return json.dumps(str(path), ensure_ascii=False)


def discover_cache(root, configured, override):
    if override:
        candidates = [override]
    else:
        preferred = root / configured.name
        if (preferred / "inventories/gaia-source.inventory.json").is_file():
            return preferred
        candidates = sorted(
            {
                p.parent.parent
                for p in root.glob("*/inventories/gaia-source.inventory.json")
            }
        )
    if len(candidates) != 1 or not all(
        (candidates[0] / "inventories" / f"{p}.inventory.json").is_file()
        for p in ("gaia-source", "xp-continuous")
    ):
        raise ValueError(
            f"expected one cache with both inventories under {root}; use --cache for the original workspace; found {candidates}"
        )
    return candidates[0]


def pinned_file(root, raw, sha):
    configured = Path(raw)
    if not configured.is_absolute():
        path = REPO / configured
    elif configured.is_relative_to(DATA_ROOT):
        path = root / configured.relative_to(DATA_ROOT)
    else:
        path = configured
    if path.is_file() and digest(path) == sha:
        return path.resolve()
    # The approved photometric artifact is also distributed byte-for-byte in Git.
    bundled = (
        REPO
        / "crates/nsb-data-tools/tests/fixtures/photometric_xp_anchored_v1/artifact.json"
    )
    if path.name == "photometric-artifact.json" and digest(bundled) == sha:
        return bundled
    if configured.is_absolute():
        for directory, children, filenames in os.walk(root):
            children[:] = sorted(
                name
                for name in children
                if name not in ("cache", "workers", "target", "runs")
                and not name.startswith("starlight-bv-")
            )
            if path.name in filenames:
                candidate = Path(directory) / path.name
                if digest(candidate) == sha:
                    return candidate.resolve()
    raise ValueError(f"missing or changed pinned input {path} (SHA-256 {sha})")


def verified_cache(products, cache, inventory_downloads):
    """Check receipt identities for every object; workers SHA-256 each object before reading."""
    inventories = {}
    receipts = {}
    for product in products:
        name = product["id"]
        path = cache / "inventories" / f"{name}.inventory.json"
        inventory = read_json(path)
        for key, want in {
            "schema_version": 1,
            "product_id": name,
            "base_url": product["base_url"],
            "checksum_manifest_url": product["checksum_manifest_url"],
            "checksum_manifest_sha256": product["checksum_manifest_sha256"],
            "official_checksum_algorithm": "md5",
        }.items():
            if inventory[key] != want:
                raise ValueError(
                    f"cache inventory identity {key} disagrees with {CONFIG}: {path}"
                )
        md5_path = inventory_downloads / f"{name}.md5"
        retrieve(md5_path, product["checksum_manifest_url"])
        pin(md5_path, product["checksum_manifest_sha256"])
        official = {}
        for line in md5_path.read_text().splitlines():
            fields = line.split()
            if len(fields) == 2:
                official[Path(fields[1].lstrip("*")).name] = fields[0]
        entries = inventory["entries"]
        if len(entries) != product["expected_partitions"]:
            raise ValueError(
                f"{name} inventory contains {len(entries)} objects, expected {product['expected_partitions']}"
            )
        if [e["filename"] for e in entries] != sorted(e["filename"] for e in entries):
            raise ValueError(f"catalogue inventory is out of order: {path}")
        for entry in entries:
            part = entry["partition_id"]
            if not re.fullmatch(r"[0-9]{6}-[0-9]{6}", part) or (name, part) in receipts:
                raise ValueError(f"invalid or repeated {name} partition {part}")
            filename = product["filename_prefix"] + part + product["filename_suffix"]
            expected = dict(
                schema_version=1,
                product_id=name,
                partition_id=part,
                filename=filename,
                source_url=product["base_url"] + filename,
                official_checksum_algorithm="md5",
                official_checksum=official.get(filename),
            )
            if not expected["official_checksum"] or entry != {
                "partition_id": part,
                "filename": filename,
                "url": expected["source_url"],
                "official_checksum": expected["official_checksum"],
            }:
                raise ValueError(f"{name}/{part} differs from the pinned ESA inventory")
            receipt_path = cache / "cache/receipts" / name / f"{part}.json"
            receipt = read_json(receipt_path)
            if not all(
                receipt[k] == v for k, v in expected.items()
            ) or not re.fullmatch(r"[0-9a-f]{64}", receipt["sha256"]):
                raise ValueError(f"invalid cache receipt {receipt_path}")
            local = cache / "cache/objects/sha256" / receipt["sha256"]
            obj = (local if local.is_file() else Path(receipt["object_path"])).resolve(
                strict=True
            )
            if not obj.is_file() or obj.stat().st_size != receipt["bytes"]:
                raise ValueError(
                    f"missing or wrong-size Gaia cache object for {receipt_path}: {obj}"
                )
            receipts[name, part] = {**receipt, "object_path": str(obj)}
        inventories[name] = path
    ids = [{part for pid, part in receipts if pid == p["id"]} for p in products]
    if len(ids) != 2 or ids[0] != ids[1]:
        raise ValueError(
            "GaiaSource and continuous XP cache inventories are not paired"
        )
    return inventories, receipts, sorted(ids[0])


def snapshot(map_dir, inventories, receipts):
    (map_dir / "inventories").mkdir(parents=True, exist_ok=True)
    for name, path in inventories.items():
        freeze(map_dir / "inventories" / f"{name}.inventory.json", path.read_bytes())
    for (name, part), receipt in receipts.items():
        dst = map_dir / "cache/receipts" / name / f"{part}.json"
        dst.parent.mkdir(parents=True, exist_ok=True)
        freeze(dst, json_bytes(receipt))


def replace_quoted_path(text, old, new):
    # Rewrite checked-in pinned paths only; preserve surrounding TOML structure.
    literal = path_literal(old)
    if literal not in text:
        raise ValueError(f"missing pinned path in template: {old}")
    return text.replace(literal, path_literal(new))


def preflight(root, cache_override, inventory_downloads):
    if not root.is_dir():
        raise ValueError(f"Ladon data root is unavailable: {root}")
    for name in ("cargo", "git", "sbatch", "squeue", "sacct", "scontrol"):
        if shutil.which(name) is None:
            raise ValueError(f"required Ladon tool is unavailable: {name}")
    config_text = CONFIG.read_text()
    config = tomllib.loads(config_text)
    run(
        "scontrol",
        "show",
        "partition",
        config["execution"]["slurm"]["partition"],
        capture=True,
    )
    controller = run("scontrol", "show", "config", capture=True)
    limits = [
        int(m)
        for m in re.findall(
            r"^\s*MaxArraySize\s*=\s*([0-9]+)", controller, re.MULTILINE
        )
    ]
    array_size = min(
        [config["execution"]["slurm"].get("max_array_size", 1000), *limits]
    )
    if array_size < 1:
        raise ValueError("Slurm job arrays are disabled")
    config_text = re.sub(r"(?m)^max_array_size\s*=.*\n", "", config_text)
    config_text = config_text.replace(
        "[execution.slurm]\n", f"[execution.slurm]\nmax_array_size = {array_size}\n", 1
    )
    cache = discover_cache(root, Path(config["workspace"]["root"]), cache_override)
    inventories, receipts, parts = verified_cache(
        config["starlight"]["gaia_products"], cache, inventory_downloads
    )
    config_text = config_text.split("\n[publish]", 1)[0]
    paths = {}
    for name in INTEGRATED:
        entry = config["starlight"][name]
        paths[name] = pinned_file(root, entry["artifact_path"], entry["sha256"])
        config_text = replace_quoted_path(
            config_text, entry["artifact_path"], paths[name]
        )
    config_text = replace_quoted_path(config_text, config["workspace"]["root"], cache)
    bright_text = BRIGHT.read_text()
    bright = tomllib.loads(bright_text)
    model = bright["spectral_model_path"]
    bright_text = replace_quoted_path(
        bright_text, model, pinned_file(root, model, bright["spectral_model_sha256"])
    )
    imports = [
        bright[k]
        for k in (
            "hipparcos2",
            "xhip",
            "hip_gaia_crossmatch",
            "gaia_quality",
            "gaia_positional_fallback",
        )
    ]
    for entry in [
        *imports,
        *bright["tycho2"],
        *bright["additional_checksum_pinned_inputs"],
    ]:
        raw = entry["path"]
        bright_text = replace_quoted_path(
            bright_text, raw, pinned_file(root, raw, entry["provenance"]["sha256"])
        )
    pin(BASELINE, BASELINE_SHA)
    pin(BASELINE_REPORT, BASELINE_REPORT_SHA)
    pin(DESIGN, DESIGN_SHA)
    return (
        cache,
        inventories,
        receipts,
        parts,
        paths,
        config_text,
        "include_bv_photometry = true\n" + bright_text,
    )


def build_configs(workspace, preparation):
    cache, inventories, receipts, parts, paths, config_text, bright_text = preparation
    map_dir = workspace / "map"
    snapshot(map_dir, inventories, receipts)
    commit = run("git", "rev-parse", "HEAD", capture=True).strip()
    freeze(
        workspace / "software.json", json_bytes({"commit": commit, "cache": str(cache)})
    )
    binary = workspace / "bin/nsb-data"
    env = {**os.environ, "CARGO_TARGET_DIR": str(REPO / "target")}
    if not binary.exists():
        run(
            "cargo",
            "build",
            "--locked",
            "-p",
            "nsb-data-tools",
            "--release",
            "--bin",
            "nsb-data",
            env=env,
        )
        binary.parent.mkdir(exist_ok=True)
        tmp = binary.with_suffix(".tmp")
        shutil.copy2(REPO / "target/release/nsb-data", tmp)
        tmp.replace(binary)
    freeze(workspace / "binary-sha256.json", json_bytes({"sha256": digest(binary)}))
    # Unequally sized catalogue partitions span the ordered Gaia HEALPix inventory.
    sample = [parts[(2 * i + 1) * len(parts) // 48] for i in range(24)]
    fit_dir = workspace / "fit"
    fit_receipt = fit_dir / "completed.sha256.json"
    shape = fit_dir / "bv-shape.json"
    if fit_receipt.exists():
        for name, expected in read_json(fit_receipt).items():
            pin(fit_dir / name, expected)
    else:
        prepare(
            fit_dir,
            binary,
            paths["photometric_inference"],
            cache=map_dir,
            partitions=sample,
        )
        freeze(
            fit_receipt,
            json_bytes(
                {
                    name: digest(fit_dir / name)
                    for name in (
                        "bv-shape.json",
                        "shape-held-out.json",
                        "shape-references.csv",
                        "shape-inputs.json",
                    )
                }
            ),
        )
    bright_cfg = workspace / "bright-stars.toml"
    freeze(bright_cfg, bright_text.encode())
    bright_dir = workspace / "bright-stars"
    artifact = bright_dir / ARTIFACT
    manifest = bright_dir / MANIFEST
    if not manifest.is_file():
        run(
            binary,
            "dataset",
            "starlight",
            "bright-stars",
            "build",
            "--config",
            bright_cfg,
            "--config-sha256",
            digest(bright_cfg),
            "--expected-build-commit",
            commit,
            "--output-directory",
            bright_dir,
        )
    built = read_json(manifest)
    pin(bright_cfg, built["build_config_sha256"])
    pin(artifact, built["artifact_sha256"])
    pin(
        bright_dir / "starlight-bright-stars-v1-diagnostics.json",
        built["diagnostics_sha256"],
    )
    config_text = replace_quoted_path(config_text, cache, map_dir)
    for table, path in (("bright_star_supplement", artifact), ("bv_photometry", shape)):
        config_text += f'\n[starlight.{table}]\nartifact_path = {path_literal(path)}\nsha256 = "{digest(path)}"\n'
    cfg = workspace / "combined-bv.toml"
    freeze(cfg, config_text.encode())
    return binary, cfg, map_dir


def finish(binary, cfg, map_dir, workspace):
    run(
        binary,
        "dataset",
        "starlight",
        "validate",
        "--config",
        cfg,
        "--executor",
        "local",
    )
    bundle = map_dir / "bv-candidate"
    compact = bundle / "starlight.compact.bin"
    comparison = run(
        binary,
        "dataset",
        "starlight",
        "diagnose",
        "compare-bv-integrated",
        "--baseline",
        BASELINE,
        "--baseline-sha256",
        BASELINE_SHA,
        "--candidate",
        compact,
        "--candidate-sha256",
        digest(compact),
        capture=True,
    ).strip()
    baseline = read_json(BASELINE_REPORT)
    merge = map_dir / "outputs/merge_report.json"
    observed = read_json(merge)
    for key in (
        "observed_sources",
        "admitted_sources",
        "excluded_sources",
        "exclusion_reasons",
        "source_record_accounting",
        "bright_star_replacement",
        "ultraviolet_applicability",
    ):
        if observed[key] != baseline[key]:
            raise ValueError(f"legacy population accounting changed in {key}: {merge}")
    uncertainty = bundle / "uncertainties.json"
    pixel_data = read_json(uncertainty)["pixels"]
    routes = Counter()
    support = Counter()
    for pixel in pixel_data.values():
        routes.update(pixel["source_counts"])
        support.update(pixel.get("inference_support", {}))
    files = [
        compact,
        uncertainty,
        bundle / "manifest.json",
        merge,
        map_dir / "validation.json",
        cfg,
        workspace / "fit/bv-shape.json",
        workspace / "fit/shape-held-out.json",
        workspace / "bright-stars" / MANIFEST,
    ]
    status = workspace / "candidate-evidence.json"
    freeze(
        status,
        json_bytes(
            dict(
                status="technical_research_candidate",
                scientific_approval="pending",
                integrated_differential=comparison,
                approved_baseline_sha256=BASELINE_SHA,
                approved_merge_report_sha256=BASELINE_REPORT_SHA,
                population_accounting_matches=True,
                source_routes=dict(routes),
                inference_support=dict(support),
                files_sha256={str(path): digest(path) for path in files},
            )
        ),
    )
    print(
        comparison, f"\nTechnical candidate: {compact}\nEvidence: {status}", flush=True
    )


def builds(map_dir):
    records = []
    for path in sorted((map_dir / "runs/starlight").glob("*/run.json")):
        manifest = read_json(path)
        if manifest["operation"] == "build" and manifest["executor"] == "slurm":
            records.append((path, manifest))
    return records


def execute(binary, cfg, map_dir, workspace, count, submit_only):
    # A complete initial manifest supplies the progress denominator even when
    # later retries contain just the missing partition IDs.
    records = builds(map_dir)
    if not records:
        run(
            binary,
            "dataset",
            "starlight",
            "build",
            "--config",
            cfg,
            "--executor",
            "slurm",
        )
        if submit_only:
            return
        records = builds(map_dir)
    [initial] = [path for path, m in records if len(m["partitions"]) == count]
    retried = False
    while True:
        records = builds(map_dir)
        job_ids = {
            job
            for _, m in records
            for job in (m["slurm_job_id"] or "").split(",")
            if job
        }
        active = set(run("squeue", "--noheader", "--format=%F", capture=True).split())
        if job_ids & active:
            if submit_only:
                print(
                    f"Active Slurm jobs: {', '.join(sorted(job_ids & active))}",
                    flush=True,
                )
                return
            time.sleep(60)
            continue
        status = json.loads(
            run(binary, "run", "status", "--run", initial, capture=True)
        )
        if status["partitions_complete"] == count:
            finish(binary, cfg, map_dir, workspace)
            return
        if retried:
            raise ValueError(
                f"{status['partitions_pending']} partitions remain after a retry. Inspect Slurm logs before resuming."
            )
        run(binary, "run", "resume", "--run", initial)
        retried = True
        if submit_only:
            return


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify cluster and pinned inputs without submitting",
    )
    parser.add_argument(
        "--submit-only",
        action="store_true",
        help="return once arrays are active; resume to finish",
    )
    parser.add_argument(
        "--resume",
        type=lambda p: Path(p).resolve(),
        help="reuse an existing new candidate workspace",
    )
    parser.add_argument("--data-root", type=lambda p: Path(p).resolve())
    parser.add_argument("--cache", type=lambda p: Path(p).resolve())
    args = parser.parse_args()
    root = args.data_root or (args.resume.parent if args.resume else DATA_ROOT)
    cache = args.cache
    if args.resume and (args.resume / "software.json").is_file():
        recorded_cache = Path(read_json(args.resume / "software.json")["cache"])
        if cache and cache != recorded_cache:
            parser.error(
                "--cache differs from the existing candidate's pinned cache; start a fresh workspace"
            )
        cache = recorded_cache
    with tempfile.TemporaryDirectory(prefix="nsb-ladon-preflight-") as temporary:
        inventory_downloads = Path(temporary)
        if args.resume:
            for path in (args.resume / "fit").glob("*.md5"):
                shutil.copyfile(path, inventory_downloads / path.name)
        preparation = preflight(root, cache, inventory_downloads)
        print(
            f"Verified {len(preparation[3])} paired receipts in {preparation[0]}",
            flush=True,
        )
        if args.check:
            for name in preparation[1]:
                receipt = preparation[2][name, preparation[3][0]]
                pin(Path(receipt["object_path"]), receipt["sha256"])
            print(
                "Slurm partition, pinned calibrations, catalogue inventories and sample object hashes verified",
                flush=True,
            )
            return
        if run("git", "status", "--porcelain", capture=True).strip():
            raise ValueError(
                "the reviewed checkout must be clean to bind the Ladon binary and CK04 build to its commit"
            )
        workspace = args.resume or Path(
            tempfile.mkdtemp(prefix="starlight-bv-candidate-", dir=root)
        )
        if (
            not workspace.is_dir()
            or workspace.is_relative_to(REPO)
            or workspace.parent != root
            or not workspace.name.startswith("starlight-bv-candidate-")
        ):
            raise ValueError(f"invalid candidate workspace: {workspace}")
        print(
            f"Candidate workspace: {workspace}\nResume with --resume {workspace}",
            flush=True,
        )
        with (workspace / ".driver.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            inputs = workspace / "fit"
            inputs.mkdir(exist_ok=True)
            for path in inventory_downloads.iterdir():
                freeze(inputs / path.name, path.read_bytes())
            design = workspace / "gaia-xp-design.json"
            freeze(design, DESIGN.read_bytes())
            os.environ["STARLIGHT_GAIAXPY_DESIGN_FIXTURE"] = str(design)
            binary, cfg, map_dir = build_configs(workspace, preparation)
            execute(
                binary, cfg, map_dir, workspace, len(preparation[3]), args.submit_only
            )


if __name__ == "__main__":
    try:
        main()
    except (OSError, KeyError, ValueError, subprocess.CalledProcessError) as exc:
        sys.exit(f"Starlight B/V regeneration: {exc}")
