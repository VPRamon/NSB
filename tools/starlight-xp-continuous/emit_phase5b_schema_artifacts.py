#!/usr/bin/env python3
"""Emit Phase 5B bulk/DataLink/canonical schema audit artifacts."""

from __future__ import annotations

import argparse
import gzip
import json
import re
from datetime import datetime, timezone
from io import TextIOWrapper
from pathlib import Path

CANONICAL_SCHEMA_VERSION = 2
CORRELATION_PACKING = "gaia_dr3_upper_triangle_column_major_excluding_diagonal"


def read_ecsv_metadata(path: Path) -> tuple[list[str], dict]:
    opener = gzip.open if path.suffix == ".gz" else open
    columns: list[dict] = []
    meta: dict = {"release": None, "table": None}
    current: dict | None = None
    with opener(path, "rb") as raw:
        text = TextIOWrapper(raw, encoding="utf-8", errors="replace")
        for line in text:
            if not line.startswith("#"):
                header = [part.strip() for part in line.strip().split(",")]
                return header, {"meta": meta, "datatype": columns}
            body = line[1:].strip()
            if body.startswith("- RELEASE:"):
                meta["release"] = body.split(":", 1)[1].strip()
            elif body.startswith("- TABLE:"):
                meta["table"] = body.split(":", 1)[1].strip()
            elif body.startswith("- name:"):
                if current:
                    columns.append(current)
                current = {"name": body.split(":", 1)[1].strip(), "datatype": None, "unit": None}
            elif current is not None and body.startswith("datatype:"):
                current["datatype"] = body.split(":", 1)[1].strip()
            elif current is not None and body.startswith("unit:"):
                current["unit"] = body.split(":", 1)[1].strip()
    raise RuntimeError(f"no data header in {path}")


def datalink_schema(path: Path) -> dict:
    text = path.read_text(encoding="utf-8", errors="replace")
    lines = [line for line in text.splitlines() if line.strip() and not line.startswith("#")]
    header = [part.strip() for part in lines[0].split(",")]
    sample = lines[1] if len(lines) > 1 else ""
    fields = []
    for name in header:
        representation = "parenthesis_array" if "_coefficients" in name or "correlations" in name else "scalar"
        if name in sample:
            token = sample.split(",", header.index(name))[0] if header.index(name) == 0 else None
        fields.append(
            {
                "name": name,
                "representation": representation,
                "array_notation": "parenthesis" if representation == "parenthesis_array" else None,
            }
        )
    return {
        "schema_id": "gaia_datalink_xp_continuous_v1",
        "source": str(path),
        "columns": header,
        "fields": fields,
        "null_representation": "empty_field",
    }


def mapping_rows() -> list[dict]:
    shared = [
        ("source_id", "source_id", "source_id", "int64 string", "u64 string", "identity", "exact match", True),
        ("bp_n_parameters", "bp_n_parameters", "bp_n_parameters", "integer", "usize", "parse", "positive", True),
        ("rp_n_parameters", "rp_n_parameters", "rp_n_parameters", "integer", "usize", "parse", "positive", True),
        ("bp_standard_deviation", "bp_standard_deviation", "bp_standard_deviation", "float", "f64", "parse", "finite > 0", True),
        ("rp_standard_deviation", "rp_standard_deviation", "rp_standard_deviation", "float", "f64", "parse", "finite > 0", True),
        ("bp_coefficients", "bp_coefficients", "bp_coefficients", "bracket JSON string", "Vec<f64>", "parse_gaia_tuple_array", "len == n_parameters", True),
        ("rp_coefficients", "rp_coefficients", "rp_coefficients", "bracket JSON string", "Vec<f64>", "parse_gaia_tuple_array", "len == n_parameters", True),
        ("bp_coefficient_errors", "bp_coefficient_errors", "bp_coefficient_errors", "bracket JSON string", "Vec<f64>", "parse_gaia_tuple_array", "non-negative", True),
        ("rp_coefficient_errors", "rp_coefficient_errors", "rp_coefficient_errors", "bracket JSON string", "Vec<f64>", "parse_gaia_tuple_array", "non-negative", True),
        (
            "bp_coefficient_correlations",
            "bp_coefficient_correlations",
            "bp_coefficient_correlations",
            "bracket JSON string",
            "Vec<f64> packed upper triangle",
            "parse_gaia_tuple_array",
            f"len == n(n-1)/2; packing={CORRELATION_PACKING}",
            True,
        ),
        (
            "rp_coefficient_correlations",
            "rp_coefficient_correlations",
            "rp_coefficient_correlations",
            "bracket JSON string",
            "Vec<f64> packed upper triangle",
            "parse_gaia_tuple_array",
            f"len == n(n-1)/2; packing={CORRELATION_PACKING}",
            True,
        ),
        ("bp_n_relevant_bases", "bp_n_relevant_bases", "bp_n_relevant_bases", "optional u16", "Option<u16>", "optional parse", "optional", True),
        ("rp_n_relevant_bases", "rp_n_relevant_bases", "rp_n_relevant_bases", "optional u16", "Option<u16>", "optional parse", "optional", True),
    ]
    rows = []
    for bulk, canonical, datalink, bulk_repr, canonical_repr, rule, validation, lossless in shared:
        rows.append(
            {
                "bulk_field": bulk,
                "canonical_field": canonical,
                "datalink_field": datalink,
                "bulk_representation": bulk_repr if bulk != canonical else bulk_repr.replace("bracket JSON string", "parenthesis or bracket"),
                "datalink_representation": bulk_repr.replace("bracket JSON string", "parenthesis tuple"),
                "canonical_representation": canonical_repr,
                "conversion_rule": rule,
                "validation_rule": validation,
                "lossless": lossless,
            }
        )
    return rows


def mapping_markdown(rows: list[dict]) -> str:
    lines = [
        "# Phase 5B XP continuous schema mapping",
        "",
        f"Generated: {datetime.now(timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ')}",
        "",
        "| bulk field | canonical field | DataLink field | bulk input | canonical output | conversion | validation | lossless |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for row in rows:
        lines.append(
            "| {bulk_field} | {canonical_field} | {datalink_field} | {bulk_representation} | {canonical_representation} | {conversion_rule} | {validation_rule} | {lossless} |".format(
                **row
            )
        )
    lines.extend(
        [
            "",
            "## Correlation packing",
            "",
            f"- Internal convention: `{CORRELATION_PACKING}`",
            "- Length: `n_parameters * (n_parameters - 1) / 2`",
            "- GaiaXPy expects correlation coefficients (not covariance); do not multiply by sigma unless upstream contract changes.",
            "- Symmetry: implied by packed upper triangle excluding diagonal.",
            "- Diagonal: implicit 1.0.",
            "",
        ]
    )
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bulk-gz", type=Path, required=True)
    parser.add_argument("--datalink-csv", type=Path, required=True)
    parser.add_argument("--output-dir", type=Path, required=True)
    args = parser.parse_args()

    args.output_dir.mkdir(parents=True, exist_ok=True)
    bulk_header, bulk_meta = read_ecsv_metadata(args.bulk_gz)
    bulk_schema = {
        "schema_id": "gaia_bulk_xp_continuous_mean_spectrum_v1",
        "source": str(args.bulk_gz),
        "meta": bulk_meta["meta"],
        "columns": bulk_header,
        "array_columns": [
            name
            for name in bulk_header
            if "coefficients" in name or "correlations" in name
        ],
        "array_notation": "bracket_json_string",
        "null_representation": "empty_csv_field",
        "source_id_type": "int64",
    }
    dl_schema = datalink_schema(args.datalink_csv)
    canonical_schema = {
        "schema_version": CANONICAL_SCHEMA_VERSION,
        "schema_id": "nsb_canonical_xp_continuous_v2",
        "correlation_packing": CORRELATION_PACKING,
        "required_fields": [
            "schema_version",
            "source_id",
            "bp_coefficients",
            "rp_coefficients",
            "bp_coefficient_errors",
            "rp_coefficient_errors",
            "bp_coefficient_correlations",
            "rp_coefficient_correlations",
            "bp_n_parameters",
            "rp_n_parameters",
            "bp_n_relevant_bases",
            "rp_n_relevant_bases",
            "quality_flags",
            "source_format",
            "source_checksum",
        ],
        "gaiaxpy_output_columns": [
            "source_id",
            "bp_n_parameters",
            "bp_standard_deviation",
            "rp_n_parameters",
            "rp_standard_deviation",
            "bp_coefficients",
            "bp_coefficient_errors",
            "bp_coefficient_correlations",
            "rp_coefficients",
            "rp_coefficient_errors",
            "rp_coefficient_correlations",
            "bp_n_relevant_bases",
            "rp_n_relevant_bases",
        ],
    }
    rows = mapping_rows()
    (args.output_dir / "phase5b_bulk_schema.json").write_text(
        json.dumps(bulk_schema, indent=2) + "\n", encoding="utf-8"
    )
    (args.output_dir / "phase5b_datalink_schema.json").write_text(
        json.dumps(dl_schema, indent=2) + "\n", encoding="utf-8"
    )
    (args.output_dir / "phase5b_canonical_schema.json").write_text(
        json.dumps(canonical_schema, indent=2) + "\n", encoding="utf-8"
    )
    (args.output_dir / "phase5b_schema_mapping.md").write_text(
        mapping_markdown(rows), encoding="utf-8"
    )
    print(f"phase5b schema artifacts -> {args.output_dir}")


if __name__ == "__main__":
    main()
