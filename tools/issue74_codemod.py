#!/usr/bin/env python3
from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MAP_SCHEMA = "nsb-healpix-starlight-candidate-v3"
REPORT_SCHEMA = "nsb-starlight-merge-report-v4"
REPRESENTATION = "sparse"
OMITTED = "zero_flux_and_source_counts"


def replace_once(text: str, old: str, new: str, path: Path) -> str:
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one occurrence, found {count}: {old[:80]!r}")
    return text.replace(old, new, 1)


def write_text(path: Path, text: str) -> None:
    path.write_text(text, encoding="utf-8")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def update_product() -> None:
    path = ROOT / "crates/nsb-data-tools/src/starlight/map/product.rs"
    text = path.read_text(encoding="utf-8")

    text = replace_once(
        text,
        'const REPORT_SCHEMA_VERSION: u32 = 3;\n'
        'const MAP_SCHEMA: &str = "nsb-healpix-starlight-candidate-v2";\n'
        'const MAP_ORDERING: &str = "nested";\n',
        'const REPORT_SCHEMA_VERSION: u32 = 4;\n'
        'const MAP_SCHEMA: &str = "nsb-healpix-starlight-candidate-v3";\n'
        'const MAP_ORDERING: &str = "nested";\n'
        'const MAP_REPRESENTATION: &str = "sparse";\n'
        'const MAP_OMITTED_PIXEL_SEMANTICS: &str = "zero_flux_and_source_counts";\n',
        path,
    )

    text = replace_once(
        text,
        '    pub derivation: String,\n'
        '    pub occupied_pixels: u64,\n',
        '    pub derivation: String,\n'
        '    pub representation: String,\n'
        '    pub omitted_pixel_semantics: String,\n'
        '    pub pixel_domain_size: u64,\n'
        '    pub occupied_pixels: u64,\n',
        path,
    )

    text = replace_once(
        text,
        '            derivation: MAP_DERIVATION.to_string(),\n'
        '            occupied_pixels: u64::try_from(emitted_pixels.len())\n',
        '            derivation: MAP_DERIVATION.to_string(),\n'
        '            representation: MAP_REPRESENTATION.to_string(),\n'
        '            omitted_pixel_semantics: MAP_OMITTED_PIXEL_SEMANTICS.to_string(),\n'
        '            pixel_domain_size: pixel_domain_size(canonical_nside)?,\n'
        '            occupied_pixels: u64::try_from(emitted_pixels.len())\n',
        path,
    )

    text = replace_once(
        text,
        '    let flux_passed = report.canonical_map.total_flux_ph_m2_s.is_finite()\n'
        '        && report.canonical_map.total_flux_ph_m2_s >= 0.0;\n',
        '    let flux_passed = report.canonical_map.total_flux_ph_m2_s.is_finite()\n'
        '        && report.canonical_map.total_flux_ph_m2_s >= 0.0;\n'
        '    let expected_pixel_domain = pixel_domain_size(canonical_nside)?;\n'
        '    let cardinality_passed = report.canonical_map.representation == MAP_REPRESENTATION\n'
        '        && report.canonical_map.omitted_pixel_semantics == MAP_OMITTED_PIXEL_SEMANTICS\n'
        '        && report.canonical_map.pixel_domain_size == expected_pixel_domain\n'
        '        && report.canonical_map.occupied_pixels > 0\n'
        '        && report.canonical_map.occupied_pixels <= expected_pixel_domain;\n',
        path,
    )

    text = replace_once(
        text,
        '        ValidationGate {\n'
        '            name: "canonical-map-flux".to_string(),\n',
        '        ValidationGate {\n'
        '            name: "canonical-map-cardinality".to_string(),\n'
        '            passed: cardinality_passed,\n'
        '            detail: format!(\n'
        '                "{} occupied of {} pixels; representation={}; omitted={}",\n'
        '                report.canonical_map.occupied_pixels,\n'
        '                report.canonical_map.pixel_domain_size,\n'
        '                report.canonical_map.representation,\n'
        '                report.canonical_map.omitted_pixel_semantics\n'
        '            ),\n'
        '        },\n'
        '        ValidationGate {\n'
        '            name: "canonical-map-flux".to_string(),\n',
        path,
    )

    text = replace_once(
        text,
        '        || report.canonical_map.derivation != MAP_DERIVATION\n',
        '        || report.canonical_map.derivation != MAP_DERIVATION\n'
        '        || report.canonical_map.representation != MAP_REPRESENTATION\n'
        '        || report.canonical_map.omitted_pixel_semantics != MAP_OMITTED_PIXEL_SEMANTICS\n',
        path,
    )

    text = replace_once(
        text,
        '    let occupied_pixels =\n'
        '        u64::try_from(pixels.len()).context("occupied pixel count exceeds u64")?;\n'
        '    if report.canonical_map.occupied_pixels != occupied_pixels\n',
        '    let occupied_pixels =\n'
        '        u64::try_from(pixels.len()).context("occupied pixel count exceeds u64")?;\n'
        '    let expected_pixel_domain = pixel_domain_size(report.canonical_map.nside)?;\n'
        '    if report.canonical_map.pixel_domain_size != expected_pixel_domain\n'
        '        || occupied_pixels > expected_pixel_domain\n'
        '        || report.canonical_map.occupied_pixels != occupied_pixels\n',
        path,
    )

    text = replace_once(
        text,
        '        ("ordering".to_string(), MAP_ORDERING.to_string()),\n'
        '        ("nside".to_string(), expected_nside.to_string()),\n',
        '        ("ordering".to_string(), MAP_ORDERING.to_string()),\n'
        '        ("representation".to_string(), MAP_REPRESENTATION.to_string()),\n'
        '        (\n'
        '            "omitted_pixel_semantics".to_string(),\n'
        '            MAP_OMITTED_PIXEL_SEMANTICS.to_string(),\n'
        '        ),\n'
        '        ("nside".to_string(), expected_nside.to_string()),\n',
        path,
    )

    text = replace_once(
        text,
        '    let mut pixels = BTreeMap::new();\n'
        '    for line in data_lines {\n',
        '    let mut pixels = BTreeMap::new();\n'
        '    let mut previous_pixel = None;\n'
        '    for line in data_lines {\n',
        path,
    )

    text = replace_once(
        text,
        '        if pixels\n'
        '            .insert(\n'
        '                pixel,\n'
        '                MapPixel {\n'
        '                    flux,\n'
        '                    admitted,\n'
        '                    excluded,\n'
        '                    ..MapPixel::default()\n'
        '                },\n'
        '            )\n'
        '            .is_some()\n'
        '        {\n'
        '            bail!("{} contains duplicate pixel {pixel}", path.display());\n'
        '        }\n',
        '        if pixels.contains_key(&pixel) {\n'
        '            bail!("{} contains duplicate pixel {pixel}", path.display());\n'
        '        }\n'
        '        if let Some(previous) = previous_pixel {\n'
        '            if pixel < previous {\n'
        '                bail!(\n'
        '                    "{} contains non-canonical pixel ordering: {pixel} follows {previous}",\n'
        '                    path.display()\n'
        '                );\n'
        '            }\n'
        '        }\n'
        '        previous_pixel = Some(pixel);\n'
        '        pixels.insert(\n'
        '            pixel,\n'
        '            MapPixel {\n'
        '                flux,\n'
        '                admitted,\n'
        '                excluded,\n'
        '                ..MapPixel::default()\n'
        '            },\n'
        '        );\n',
        path,
    )

    text = replace_once(
        text,
        'fn map_totals(pixels: &BTreeMap<u32, MapPixel>) -> Result<(f64, u64, u64)> {\n',
        'fn pixel_domain_size(nside: u32) -> Result<u64> {\n'
        '    u64::from(nside)\n'
        '        .checked_mul(u64::from(nside))\n'
        '        .and_then(|pixels_per_face| pixels_per_face.checked_mul(12))\n'
        '        .context("HEALPix pixel-domain size overflow")\n'
        '}\n'
        '\n'
        'fn map_totals(pixels: &BTreeMap<u32, MapPixel>) -> Result<(f64, u64, u64)> {\n',
        path,
    )

    text = replace_once(
        text,
        '         # ordering={MAP_ORDERING}\\n\\\n'
        '         # nside={nside}\\n\\\n',
        '         # ordering={MAP_ORDERING}\\n\\\n'
        '         # representation={MAP_REPRESENTATION}\\n\\\n'
        '         # omitted_pixel_semantics={MAP_OMITTED_PIXEL_SEMANTICS}\\n\\\n'
        '         # nside={nside}\\n\\\n',
        path,
    )

    anchor = '''    #[test]
    fn validation_rejects_unknown_map_header() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let text = fs::read_to_string(&path).unwrap().replacen(
            "# map_type=healpix",
            "# unexpected=value\\n# map_type=healpix",
            1,
        );
        artifact_store::atomic_write(&path, text.as_bytes()).unwrap();
        assert!(validate_map(&path, 128).is_err());
    }
'''
    additions = anchor + '''
    #[test]
    fn validation_rejects_out_of_order_map_pixels() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let text = fs::read_to_string(&path).unwrap();
        let mut lines = text.lines().map(str::to_string).collect::<Vec<_>>();
        let first_data = lines
            .iter()
            .position(|line| line == "pixel,flux_ph_m2_s,admitted_sources,excluded_sources")
            .unwrap()
            + 1;
        lines.swap(first_data, first_data + 1);
        artifact_store::atomic_write(&path, format!("{}\\n", lines.join("\\n")).as_bytes()).unwrap();
        let error = validate_map(&path, 128).unwrap_err().to_string();
        assert!(error.contains("non-canonical pixel ordering"));
    }

    #[test]
    fn validation_rejects_missing_sparse_representation_header() {
        let temp = TempDir::new().unwrap();
        emit_fixture(&temp, 128);
        let path = temp.path().join("outputs/starlight_nside128.csv");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("# representation=sparse\\n", "");
        artifact_store::atomic_write(&path, text.as_bytes()).unwrap();
        assert!(validate_map(&path, 128).is_err());
    }

    #[test]
    fn validation_rejects_incompatible_report_representation() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.representation = "full-sky".to_string();
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_report_cardinality_mismatch() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.occupied_pixels += 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }

    #[test]
    fn validation_rejects_wrong_pixel_domain_size() {
        let temp = TempDir::new().unwrap();
        let mut report = emit_fixture(&temp, 128);
        report.canonical_map.pixel_domain_size -= 1;
        rewrite_report(&temp, &report);
        assert!(validate_report(&temp.path().join("outputs/merge_report.json")).is_err());
    }
'''
    text = replace_once(text, anchor, additions, path)
    write_text(path, text)


def update_engine_core() -> None:
    path = ROOT / "crates/nsb-data-tools/src/dataset/engine_core.rs"
    text = path.read_text(encoding="utf-8")
    text = text.replace('"nsb-starlight-merge-report-v3"', '"nsb-starlight-merge-report-v4"')
    text = text.replace('"nsb-healpix-starlight-candidate-v2"', '"nsb-healpix-starlight-candidate-v3"')

    text = replace_once(
        text,
        '    if dataset == DatasetName::Starlight {\n'
        '        asset["schema"] = toml_edit::value(starlight_asset_schema(name));\n'
        '    }\n',
        '    if dataset == DatasetName::Starlight {\n'
        '        asset["schema"] = toml_edit::value(starlight_asset_schema(name));\n'
        '        if name.starts_with("starlight_nside") && name.ends_with(".csv") {\n'
        '            let nside = name\n'
        '                .strip_prefix("starlight_nside")\n'
        '                .and_then(|value| value.strip_suffix(".csv"))\n'
        '                .context("canonical Starlight filename has no nside")?;\n'
        '            let mut header = toml_edit::Table::new();\n'
        '            header["schema"] = toml_edit::value("nsb-healpix-starlight-candidate-v3");\n'
        '            header["map_type"] = toml_edit::value("healpix");\n'
        '            header["coordinate_frame"] = toml_edit::value("galactic");\n'
        '            header["ordering"] = toml_edit::value("nested");\n'
        '            header["representation"] = toml_edit::value("sparse");\n'
        '            header["omitted_pixel_semantics"] =\n'
        '                toml_edit::value("zero_flux_and_source_counts");\n'
        '            header["nside"] = toml_edit::value(nside);\n'
        '            header["flux_quantity"] = toml_edit::value("integrated_per_pixel");\n'
        '            header["flux_unit"] = toml_edit::value("ph_m-2_s-1");\n'
        '            header["derivation"] =\n'
        '                toml_edit::value("canonical_gaia_source_accumulation");\n'
        '            header["source_count_semantics"] =\n'
        '                toml_edit::value("exact_source_membership");\n'
        '            asset["header"] = toml_edit::Item::Table(header);\n'
        '        }\n'
        '    }\n',
        path,
    )

    text = replace_once(
        text,
        '        assert_eq!(candidate["runtime_embedded"].as_bool(), Some(false));\n',
        '        assert_eq!(candidate["runtime_embedded"].as_bool(), Some(false));\n'
        '        assert_eq!(candidate["header"]["representation"].as_str(), Some("sparse"));\n'
        '        assert_eq!(\n'
        '            candidate["header"]["omitted_pixel_semantics"].as_str(),\n'
        '            Some("zero_flux_and_source_counts")\n'
        '        );\n',
        path,
    )
    write_text(path, text)


def update_scientific_assets_test() -> None:
    path = ROOT / "crates/nsb-data-tools/tests/scientific_assets.rs"
    text = path.read_text(encoding="utf-8")
    text = text.replace(
        'asset.schema == "nsb-healpix-starlight-candidate-v2"',
        'asset.schema == "nsb-healpix-starlight-candidate-v3"',
    )
    text = replace_once(
        text,
        '    if candidates != ["starlight_nside128.csv"] {\n'
        '        bail!("expected exactly one Gaia-derived canonical map, found {candidates:?}");\n'
        '    }\n'
        '    Ok(())\n',
        '    if candidates != ["starlight_nside128.csv"] {\n'
        '        bail!("expected exactly one Gaia-derived canonical map, found {candidates:?}");\n'
        '    }\n'
        '    let candidate = manifest\n'
        '        .assets\n'
        '        .iter()\n'
        '        .find(|asset| asset.path == "starlight_nside128.csv")\n'
        '        .context("canonical Gaia candidate is missing")?;\n'
        '    if candidate.header.get("representation").map(String::as_str) != Some("sparse")\n'
        '        || candidate\n'
        '            .header\n'
        '            .get("omitted_pixel_semantics")\n'
        '            .map(String::as_str)\n'
        '            != Some("zero_flux_and_source_counts")\n'
        '    {\n'
        '        bail!("canonical Gaia candidate lacks the sparse representation contract");\n'
        '    }\n'
        '    Ok(())\n',
        path,
    )
    write_text(path, text)


def update_map_and_report() -> tuple[str, str]:
    map_path = ROOT / "crates/nsb/data/starlight_nside128.csv"
    text = map_path.read_text(encoding="utf-8")
    if "# representation=" not in text:
        text = replace_once(
            text,
            "# ordering=nested\n# nside=128\n",
            "# ordering=nested\n# representation=sparse\n"
            "# omitted_pixel_semantics=zero_flux_and_source_counts\n# nside=128\n",
            map_path,
        )
    text = text.replace(
        "# schema=nsb-healpix-starlight-candidate-v2",
        "# schema=nsb-healpix-starlight-candidate-v3",
        1,
    )
    lines = [
        line
        for line in text.splitlines()
        if line and not line.startswith("#") and not line.startswith("pixel,")
    ]
    pixels = [int(line.split(",", 1)[0]) for line in lines]
    if not pixels:
        raise RuntimeError("candidate map has no data rows")
    for previous, current in zip(pixels, pixels[1:]):
        if current <= previous:
            raise RuntimeError(
                f"candidate map is not strictly ordered: {current} follows {previous}"
            )
    domain = 12 * 128 * 128
    if len(pixels) > domain:
        raise RuntimeError("candidate map row count exceeds HEALPix domain")
    write_text(map_path, text if text.endswith("\n") else text + "\n")
    map_digest = sha256(map_path)

    report_path = ROOT / "crates/nsb/data/merge_report.json"
    report = json.loads(report_path.read_text(encoding="utf-8"))
    report["schema_version"] = 4
    canonical = report["canonical_map"]
    canonical["schema"] = MAP_SCHEMA
    canonical["representation"] = REPRESENTATION
    canonical["omitted_pixel_semantics"] = OMITTED
    canonical["pixel_domain_size"] = domain
    canonical["occupied_pixels"] = len(pixels)
    canonical["sha256"] = map_digest
    write_text(report_path, json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    return map_digest, sha256(report_path)


def update_manifest(map_digest: str, report_digest: str) -> None:
    path = ROOT / "crates/nsb/data/manifest.toml"
    text = path.read_text(encoding="utf-8")
    text = text.replace('schema = "nsb-starlight-merge-report-v3"', f'schema = "{REPORT_SCHEMA}"')
    text = text.replace('schema = "nsb-healpix-starlight-candidate-v2"', f'schema = "{MAP_SCHEMA}"')
    text = re.sub(
        r'(path = "merge_report\.json"\n(?:.*\n)*?sha256 = ")[0-9a-f]{64}(")',
        rf'\g<1>{report_digest}\2',
        text,
        count=1,
    )
    text = re.sub(
        r'(path = "starlight_nside128\.csv"\n(?:.*\n)*?sha256 = ")[0-9a-f]{64}(")',
        rf'\g<1>{map_digest}\2',
        text,
        count=1,
    )
    marker = 'runtime_embedded = false\n'
    candidate_index = text.index('path = "starlight_nside128.csv"')
    insert_at = text.index(marker, candidate_index) + len(marker)
    header = (
        '\n[assets.header]\n'
        f'schema = "{MAP_SCHEMA}"\n'
        'map_type = "healpix"\n'
        'coordinate_frame = "galactic"\n'
        'ordering = "nested"\n'
        'representation = "sparse"\n'
        'omitted_pixel_semantics = "zero_flux_and_source_counts"\n'
        'nside = "128"\n'
        'flux_quantity = "integrated_per_pixel"\n'
        'flux_unit = "ph_m-2_s-1"\n'
        'derivation = "canonical_gaia_source_accumulation"\n'
        'source_count_semantics = "exact_source_membership"\n'
    )
    if text.find('[assets.header]', candidate_index) == -1:
        text = text[:insert_at] + header + text[insert_at:]
    write_text(path, text)


def update_docs(map_digest: str, report_digest: str) -> None:
    validation = ROOT / "docs/nsb_components/starlight/map-validation.md"
    text = validation.read_text(encoding="utf-8")
    text = text.replace("candidate-v2", "candidate-v3").replace("Report schema v3", "Report schema v4")
    text = replace_once(
        text,
        "ordering=nested\nflux_quantity=integrated_per_pixel\n",
        "ordering=nested\nrepresentation=sparse\n"
        "omitted_pixel_semantics=zero_flux_and_source_counts\n"
        "flux_quantity=integrated_per_pixel\n",
        validation,
    )
    text = replace_once(
        text,
        "The nside header and filename come from `canonical_nside`. Validation rejects\n"
        "missing, unknown, duplicate, or incompatible headers; malformed or duplicate\n"
        "rows; out-of-range pixels; negative or non-finite flux; and empty maps.\n",
        "The nside header and filename come from `canonical_nside`. The canonical CSV is\n"
        "sparse: rows must be strictly increasing by pixel ID, and an omitted HEALPix\n"
        "pixel means zero integrated flux, zero admitted sources, and zero excluded\n"
        "sources. Validation rejects missing, unknown, duplicate, or incompatible\n"
        "headers; malformed, duplicate, or out-of-order rows; out-of-range pixels;\n"
        "negative or non-finite flux; empty maps; and row counts larger than\n"
        "`12 * nside^2`.\n",
        validation,
    )
    text = replace_once(
        text,
        "occupied-pixel count, integrated flux, admitted sources, and excluded sources\n"
        "to match the report.",
        "representation, omitted-pixel semantics, pixel-domain size, occupied-pixel\n"
        "count, integrated flux, admitted sources, and excluded sources to match the\n"
        "report.",
        validation,
    )
    text = text.replace(
        "- `canonical-map-integrity`;\n- `canonical-map-flux`;",
        "- `canonical-map-integrity`;\n- `canonical-map-cardinality`;\n- `canonical-map-flux`;",
    )
    write_text(validation, text)

    generation = ROOT / "docs/nsb_components/starlight/map-generation.md"
    text = generation.read_text(encoding="utf-8")
    text = replace_once(
        text,
        "`flux_ph_m2_s` is integrated photon flux per HEALPix pixel in `ph m-2 s-1`.\n",
        "The canonical candidate uses a sparse, strictly pixel-sorted representation.\n"
        "Omitted HEALPix pixels have zero integrated flux and zero source counts; the\n"
        "report records both the occupied row count and the full `12 * nside^2` pixel\n"
        "domain. `flux_ph_m2_s` is integrated photon flux per HEALPix pixel in\n"
        "`ph m-2 s-1`.\n",
        generation,
    )
    write_text(generation, text)

    existing = ROOT / "docs/nsb_components/starlight/existing-datasets.md"
    text = existing.read_text(encoding="utf-8")
    text = text.replace(
        "`ab9ed8db9c81d35887642ae7453e3fea69a4f2ebfa475662edc758133d01ffda`",
        f"`{map_digest}`",
    )
    text = text.replace(
        "`9a09a9be25b6fef472eb53bc36fd7567f76775504859c133c9278ea36f14b371`",
        f"`{report_digest}`",
    )
    text = replace_once(
        text,
        "The nside-128 scientific rows are identical to the artifact first published by\n"
        "commit `6e515a6e7dc01b37594a765021d415fd5f7e768a`. PR #77 adds explicit v2\n"
        "metadata headers, so its byte checksum changes from\n"
        "`09ca9bd57407beab49ff26cf1fe8ab305ccf9394e244563ee833b059a2287d35`.\n"
        "The Gaia production pipeline was not rerun for this metadata-only migration.\n",
        "The nside-128 scientific rows are identical to the artifact first published by\n"
        "commit `6e515a6e7dc01b37594a765021d415fd5f7e768a`. PR #77 added the v2 physical\n"
        "metadata. Issue #74 adds only the v3 sparse-representation headers and report\n"
        "cardinality fields; the Gaia production pipeline was not rerun for either\n"
        "metadata-only migration. The sparse file contains 196,604 strictly ordered\n"
        "rows in a 196,608-pixel domain. Its four omitted pixels have zero integrated\n"
        "flux and zero admitted/excluded source counts by contract.\n",
        existing,
    )
    write_text(existing, text)


def main() -> None:
    update_product()
    update_engine_core()
    update_scientific_assets_test()
    map_digest, report_digest = update_map_and_report()
    update_manifest(map_digest, report_digest)
    update_docs(map_digest, report_digest)


if __name__ == "__main__":
    main()
