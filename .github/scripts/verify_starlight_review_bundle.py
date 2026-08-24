#!/usr/bin/env python3
"""Fail-closed verification of the human Starlight review evidence bundle."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path

BUNDLE_SCHEMA = "nsb-starlight-review-bundle-v1"
BUNDLE_CONDITION_ID = "review-bundle-v1"
REQUIRED_ARTIFACT_IDS = {
    "candidate_map",
    "merge_report",
    "release_candidate_gates",
    "redistribution_inventory",
    "validation_artifact_manifest",
}
SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")


def fail(message: str) -> "NoReturn":
    raise SystemExit(f"fail-closed: {message}")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def repository_path(root: Path, value: str, label: str) -> Path:
    relative = Path(value)
    if relative.is_absolute() or ".." in relative.parts:
        fail(f"{label} must be a repository-relative path: {value!r}")
    resolved = (root / relative).resolve()
    try:
        resolved.relative_to(root)
    except ValueError:
        fail(f"{label} escapes repository root: {value!r}")
    return resolved


def load_json(path: Path, label: str) -> dict:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        fail(f"cannot read {label} {path}: {error}")
    if not isinstance(value, dict):
        fail(f"{label} must be a JSON object")
    return value


def require_bundle_condition(decision: dict, expected_path: str, label: str) -> str:
    conditions = decision.get("conditions")
    if not isinstance(conditions, list):
        fail(f"{label} conditions must be a list")
    matches = [
        condition
        for condition in conditions
        if isinstance(condition, dict) and condition.get("id") == BUNDLE_CONDITION_ID
    ]
    if len(matches) != 1:
        fail(
            f"{label} must contain exactly one structured {BUNDLE_CONDITION_ID!r} condition"
        )
    condition = matches[0]
    if not str(condition.get("description", "")).strip():
        fail(f"{label} review-bundle condition must have a description")
    verifier = condition.get("verifier")
    if not isinstance(verifier, dict):
        fail(f"{label} review-bundle condition must have a verifier object")
    if verifier.get("type") != "repository_file_sha256":
        fail(f"{label} review-bundle verifier must use repository_file_sha256")
    if verifier.get("path") != expected_path:
        fail(
            f"{label} review-bundle condition pins {verifier.get('path')!r}, expected {expected_path!r}"
        )
    digest = verifier.get("sha256")
    if not isinstance(digest, str) or not SHA256_RE.fullmatch(digest):
        fail(f"{label} review-bundle condition must pin a 64-digit SHA-256")
    return digest.lower()


def verify_bundle(root: Path, bundle_path: Path, candidate_sha256: str) -> None:
    try:
        bundle = tomllib.loads(bundle_path.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as error:
        fail(f"cannot read review bundle {bundle_path}: {error}")

    if bundle.get("schema_version") != 1 or bundle.get("schema") != BUNDLE_SCHEMA:
        fail(
            f"review bundle must use schema_version=1 and schema={BUNDLE_SCHEMA!r}"
        )

    artifacts = bundle.get("artifacts")
    if not isinstance(artifacts, list):
        fail("review bundle artifacts must be an array of tables")

    by_id: dict[str, dict] = {}
    for artifact in artifacts:
        if not isinstance(artifact, dict):
            fail("review bundle artifact entry must be a table")
        artifact_id = artifact.get("id")
        path_value = artifact.get("path")
        expected = artifact.get("sha256")
        if not isinstance(artifact_id, str) or not artifact_id.strip():
            fail("review bundle artifact id must be non-empty")
        if artifact_id in by_id:
            fail(f"review bundle contains duplicate artifact id {artifact_id!r}")
        if not isinstance(path_value, str) or not path_value.strip():
            fail(f"review bundle artifact {artifact_id!r} has no path")
        if not isinstance(expected, str) or not SHA256_RE.fullmatch(expected):
            fail(f"review bundle artifact {artifact_id!r} has an invalid SHA-256")
        path = repository_path(root, path_value, f"artifact {artifact_id}")
        if not path.is_file():
            fail(f"review bundle artifact {artifact_id!r} is missing: {path_value}")
        actual = sha256_file(path)
        if actual.lower() != expected.lower():
            fail(
                f"review bundle artifact {artifact_id!r} checksum mismatch: expected {expected}, actual {actual}"
            )
        by_id[artifact_id] = artifact

    missing = REQUIRED_ARTIFACT_IDS.difference(by_id)
    if missing:
        fail(f"review bundle is missing required artifacts: {sorted(missing)}")

    bundle_candidate = str(by_id["candidate_map"]["sha256"]).lower()
    if bundle_candidate != candidate_sha256.lower():
        fail(
            "review bundle candidate_map SHA-256 does not match the candidate pinned by the human decisions"
        )


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repository-root", default=".")
    parser.add_argument("--bundle", required=True)
    parser.add_argument("--scientific-decision", required=True)
    parser.add_argument("--redistribution-decision", required=True)
    args = parser.parse_args()

    root = Path(args.repository_root).resolve()
    bundle_rel = str(Path(args.bundle))
    bundle_path = repository_path(root, bundle_rel, "review bundle")
    scientific_path = repository_path(
        root, args.scientific_decision, "scientific decision"
    )
    redistribution_path = repository_path(
        root, args.redistribution_decision, "redistribution decision"
    )

    if not bundle_path.is_file():
        fail(f"review bundle is missing: {bundle_rel}")

    scientific = load_json(scientific_path, "scientific decision")
    redistribution = load_json(redistribution_path, "redistribution decision")

    scientific_candidate = scientific.get("candidate_sha256")
    redistribution_candidate = redistribution.get("candidate_sha256")
    if not isinstance(scientific_candidate, str) or not SHA256_RE.fullmatch(
        scientific_candidate
    ):
        fail("scientific decision candidate_sha256 is invalid")
    if not isinstance(redistribution_candidate, str) or not SHA256_RE.fullmatch(
        redistribution_candidate
    ):
        fail("redistribution decision candidate_sha256 is invalid")
    if scientific_candidate.lower() != redistribution_candidate.lower():
        fail("human decisions pin different candidate SHA-256 values")

    scientific_bundle = require_bundle_condition(
        scientific, bundle_rel, "scientific decision"
    )
    redistribution_bundle = require_bundle_condition(
        redistribution, bundle_rel, "redistribution decision"
    )
    if scientific_bundle != redistribution_bundle:
        fail("human decisions pin different review-bundle SHA-256 values")

    actual_bundle = sha256_file(bundle_path)
    if actual_bundle != scientific_bundle:
        fail(
            f"review bundle checksum mismatch: decisions pin {scientific_bundle}, actual {actual_bundle}"
        )

    verify_bundle(root, bundle_path, scientific_candidate)
    print(
        f"review bundle verified: sha256:{actual_bundle}, candidate sha256:{scientific_candidate.lower()}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
