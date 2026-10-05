#!/usr/bin/env python3
"""Build the external CK04/Hp spectral-model JSON used by NSB bright stars.

This preprocessing script requires astropy and numpy. It never downloads
inputs: every supplied upstream file is verified against an explicit SHA-256.
"""

import argparse
import gzip
import hashlib
import json
import math
import re
import xml.etree.ElementTree as ET
from pathlib import Path

import numpy as np
from astropy.io import fits

HC_J_M = 1.986_445_86e-25
C_M_S = 299_792_458.0


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_sha(path, expected):
    actual = sha256(path)
    if actual != expected:
        raise ValueError(f"checksum mismatch for {path}: {actual} != {expected}")


def xhip_codes(hip2_path, xhip_path):
    bright = set()
    with gzip.open(hip2_path, "rt", encoding="ascii") as stream:
        for line in stream:
            if float(line[129:136]) <= 4.0:
                bright.add(int(line[0:6]))
    codes = set()
    with gzip.open(xhip_path, "rt", encoding="ascii") as stream:
        for line in stream:
            if int(line[0:6]) not in bright:
                continue
            temperature = line[263:266].strip()
            luminosity = line[267:268].strip()
            if temperature and luminosity:
                codes.add((int(temperature), int(luminosity)))
    return codes


def target_temperature(code, mapping):
    anchors = {
        int(key): float(value)
        for key, value in mapping["temperature_anchors_kelvin"].items()
    }
    if (
        code > mapping["supported_temperature_code_max"]
        or code < mapping["supported_temperature_code_min"]
    ):
        return None
    lower = max(key for key in anchors if key <= code)
    upper = min(key for key in anchors if key >= code)
    if lower == upper:
        return float(anchors[lower])
    fraction = (code - lower) / (upper - lower)
    return anchors[lower] + fraction * (anchors[upper] - anchors[lower])


def target_logg(luminosity_code, mapping):
    value = mapping["luminosity_class_logg"].get(str(luminosity_code))
    return None if value is None else float(value)


def response_and_calibration(response_xml, vega_fits):
    root = ET.parse(response_xml).getroot()
    params = {
        element.attrib["name"]: element.attrib.get("value", "")
        for element in root.iter()
        if element.tag.endswith("PARAM") and "name" in element.attrib
    }
    rows = [
        [float(cell.text or "nan") for cell in row]
        for row in root.iter()
        if row.tag.endswith("TR")
        for cells in [[cell for cell in row if cell.tag.endswith("TD")]]
        if len(cells) == 2
    ]
    wavelength_a = np.asarray([row[0] for row in rows])
    throughput = np.asarray([row[1] for row in rows])
    if (
        len(rows) < 2
        or not np.all(np.diff(wavelength_a) > 0)
        or not np.all(np.isfinite(throughput))
        or not np.all((throughput >= 0) & (throughput <= 1))
    ):
        raise ValueError("invalid SVO response table")
    with fits.open(vega_fits) as hdus:
        table = hdus[1].data
        vega_wavelength_a = np.asarray(table["WAVELENGTH"], dtype=float)
        vega_f_lambda_si = np.asarray(table["FLUX"], dtype=float) * 1.0e7
    pivot_a = float(params["WavelengthPivot"])
    pivot_m = pivot_a * 1.0e-10
    pivot_f_lambda = float(np.interp(pivot_a, vega_wavelength_a, vega_f_lambda_si))
    pivot_f_nu_jy = pivot_f_lambda * pivot_m * pivot_m / C_M_S * 1.0e26
    scale = float(params["ZeroPoint"]) / pivot_f_nu_jy
    response_f_lambda = np.interp(wavelength_a, vega_wavelength_a, vega_f_lambda_si) * scale
    wavelength_m = wavelength_a * 1.0e-10
    zero_photons = float(
        np.trapz(wavelength_m / HC_J_M * response_f_lambda * throughput, wavelength_m)
    )
    if not math.isfinite(zero_photons) or zero_photons <= 0:
        raise ValueError("invalid Vega/Hp zero-point integral")
    response = {
        "band_id": params["filterID"],
        "wavelengths_m": wavelength_m.tolist(),
        "throughput": throughput.tolist(),
        "detector_convention": "photon_counting",
        "citation": "Bessell 2000 via SVO Filter Profile Service",
        "sha256": sha256(response_xml),
    }
    calibration = {
        "system": "Hipparcos Hp Vega",
        "band_id": params["filterID"],
        "zero_point_convention": "SVO Vega/Pogson ZeroPoint normalized CALSPEC reference",
        "reference_spectrum": f"CALSPEC {vega_fits.name} sha256:{sha256(vega_fits)}",
        "zero_point_photon_flux_ph_m2_s": zero_photons,
        "citation": "Bessell 2000; SVO FPS; STScI CALSPEC",
    }
    return response, calibration


def available_ck04(ck04_root):
    paths = {}
    for path in (ck04_root / "ckp00").glob("ckp00_*.fits"):
        paths[int(path.stem.split("_")[1])] = path
    if not paths:
        raise ValueError("no solar-metallicity CK04 spectra found")
    return paths


def read_template(path, requested_logg):
    with fits.open(path) as hdus:
        names = list(hdus[1].columns.names)
        gravity_names = [name for name in names if name.startswith("g")]
        wavelength_a = np.asarray(hdus[1].data["WAVELENGTH"], dtype=float)
        optical = (wavelength_a >= 3360) & (wavelength_a <= 6500)
        valid_gravity_names = [
            name
            for name in gravity_names
            if np.any(np.asarray(hdus[1].data[name], dtype=float)[optical] > 0)
        ]
        if not valid_gravity_names:
            raise ValueError(f"CK04 spectrum has no positive optical gravity column: {path}")
        gravity_name = min(
            valid_gravity_names,
            key=lambda name: abs(int(name[1:]) / 10 - requested_logg),
        )
        flux_si = np.asarray(hdus[1].data[gravity_name], dtype=float) * 1.0e7
    mask = (wavelength_a >= 2990) & (wavelength_a <= 9000)
    wavelength_m = wavelength_a[mask] * 1.0e-10
    flux_si = flux_si[mask]
    if len(wavelength_m) < 2 or not np.all(np.diff(wavelength_m) > 0) or np.any(flux_si < 0):
        raise ValueError(f"invalid CK04 spectrum {path}")
    template_id = f"ck04-v2-solar-t{path.stem.split('_')[1]}-{gravity_name}"
    return template_id, {
        "template_id": template_id,
        "wavelengths_m": wavelength_m.tolist(),
        "f_lambda_si": flux_si.tolist(),
        "provenance": f"{path.name}:{gravity_name}; CK04 v2 tar; CC BY 4.0 HLSP",
    }


def main():
    parser = argparse.ArgumentParser()
    for name in ("hip2", "xhip", "ck04_tar", "hp_response", "vega"):
        parser.add_argument(f"--{name.replace('_', '-')}", type=Path, required=True)
        parser.add_argument(f"--{name.replace('_', '-')}-sha256", required=True)
    parser.add_argument("--ck04-root", type=Path, required=True)
    parser.add_argument("--spectral-mapping", type=Path, required=True)
    parser.add_argument("--spectral-mapping-sha256", required=True)
    parser.add_argument("--uncertainty-calibration", type=Path, required=True)
    parser.add_argument("--uncertainty-calibration-sha256", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--software-commit", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[0-9a-f]{7,40}", args.software_commit):
        raise ValueError("software commit must be 7-40 lowercase hexadecimal characters")
    for name in ("hip2", "xhip", "ck04_tar", "hp_response", "vega"):
        require_sha(getattr(args, name), getattr(args, f"{name}_sha256"))
    require_sha(args.spectral_mapping, args.spectral_mapping_sha256)
    require_sha(args.uncertainty_calibration, args.uncertainty_calibration_sha256)
    mapping = json.loads(args.spectral_mapping.read_text())
    uncertainty_calibration = json.loads(args.uncertainty_calibration.read_text())
    if mapping.get("status") != "experimental-provisional":
        raise ValueError("unsupported spectral mapping status")
    if uncertainty_calibration.get("status") != "provisional-uncalibrated":
        raise ValueError("unsupported uncertainty calibration status")
    response, hp_calibration = response_and_calibration(args.hp_response, args.vega)
    available = available_ck04(args.ck04_root)
    templates = {}
    assignments = []
    unsupported = []
    for temperature_code, luminosity_code in sorted(xhip_codes(args.hip2, args.xhip)):
        target = target_temperature(temperature_code, mapping)
        logg = target_logg(luminosity_code, mapping)
        if target is None or logg is None:
            unsupported.append(
                {
                    "temperature_code": temperature_code,
                    "luminosity_class_code": luminosity_code,
                    "reason": "unsupported_spectral_mapping",
                }
            )
            continue
        selected_temperature = min(available, key=lambda value: abs(value - target))
        template_id, template = read_template(
            available[selected_temperature], logg
        )
        templates[template_id] = template
        assignments.append(
            {
                "temperature_code": temperature_code,
                "luminosity_class_code": luminosity_code,
                "template_id": template_id,
            }
        )
    model = {
        "model_id": "xhip-sptype-ck04-v2-hp-bessell2000-v1",
        "builder_software_commit": args.software_commit,
        "assignments": assignments,
        "templates": [templates[key] for key in sorted(templates)],
        "hp_response": response,
        "hp_calibration": hp_calibration,
        **uncertainty_calibration["uncertainty_parameters"],
        "uncertainty_calibration_status": uncertainty_calibration["status"],
        "uncertainty_calibration_sha256": args.uncertainty_calibration_sha256,
        "spectral_mapping_status": mapping["status"],
        "spectral_mapping_sha256": args.spectral_mapping_sha256,
        "unsupported_assignments": unsupported,
        "template_library_citation": "Castelli & Kurucz 2004; MAST REFERENCE-ATLASES DOI 10.17909/t9-khb7-4049",
        "spectral_type_mapping_citation": "; ".join(mapping["citations"]),
    }
    args.output.write_text(json.dumps(model, indent=2, sort_keys=True) + "\n")
    print(
        json.dumps(
            {
                "output": str(args.output),
                "sha256": sha256(args.output),
                "unsupported_spectral_classifications": len(unsupported),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
