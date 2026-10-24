# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
import io
import json
import sys
import tarfile
from pathlib import Path
from types import SimpleNamespace

import astropy.units as u
import nbformat
import numpy as np
import pytest
from astropy.constants import c, h
from nbclient import NotebookClient

from nsb_observational import data
from nsb_observational.experiment import Config, analyze, prepare, synthetic
from nsb_observational.report import seal, verify
from nsb_observational.runtime import capabilities, source_identity
from nsb_observational.statistics import align, baseline, errors, scores


def test_photon_conversion_independent_astropy():
    # Analytic constant energy radiance: integral lambda d-lambda/(h*c).
    wave = np.array([3600.0, 4100.0, 5700.0, 6600.0])
    flux = np.full(4, 2e-17)
    measured = data.measured_radiance(wave, flux)
    energy = 2e-17 * u.erg / u.s / u.cm**2 / u.AA / u.arcsec**2
    exact = (energy * ((6500 * u.AA) ** 2 - (3650 * u.AA) ** 2) / (2 * h * c)).to_value(
        1 / u.cm**2 / u.ns / u.sr
    )
    assert measured == pytest.approx(exact, rel=2e-15)
    # Linear energy spectrum gives cubic antiderivative. Tests band-edge interpolation.
    slope = 1e-21
    linear = data.measured_radiance(wave, slope * wave)
    analytic = slope * (6500**3 - 3650**3) / 3 * 1e-8 / data.HC_ERG_CM * data.ARCSEC2_PER_SR * 1e-9
    assert linear == pytest.approx(analytic, rel=2e-15)
    assert np.isfinite(data.measured_radiance(wave, np.array([-1.0, 2.0, 3.0, 4.0]) * 1e-17))
    for x, y in ((wave[::-1], flux), (wave, np.full(4, np.nan)), (wave[1:], flux[1:])):
        with pytest.raises(ValueError):
            data.measured_radiance(x, y)


def test_cache_checksum_and_offline(tmp_path):
    p = tmp_path / "archive"
    p.write_bytes(b"corrupt")
    with pytest.raises(ValueError, match="Checksum"):
        data.acquire(p, "0" * 64, offline=True)
    assert data.acquire(p, data.sha256(p), offline=True) == p
    with pytest.raises(FileNotFoundError):
        data.acquire(tmp_path / "missing", "0" * 64, offline=True)


def archive(path, name="fxSKY.test.fits", kind=None, duplicate=False):
    with tarfile.open(path, "w") as tar:
        entry = tarfile.TarInfo(name)
        entry.size = 3
        if kind:
            entry.type = kind
            entry.linkname = "/etc/passwd"
        tar.addfile(entry, io.BytesIO(b"abc"))
        if duplicate:
            tar.addfile(entry, io.BytesIO(b"abc"))
    return data.sha256(path)


@pytest.mark.parametrize(
    "name,kind,duplicate",
    [
        ("../escape", None, False),
        ("/absolute", None, False),
        ("dir/file", None, False),
        ("link", tarfile.SYMTYPE, False),
        ("hard", tarfile.LNKTYPE, False),
        ("fxSKY.test.fits", None, True),
        ("directory", tarfile.DIRTYPE, False),
    ],
)
def test_unsafe_archives(tmp_path, name, kind, duplicate):
    path = tmp_path / "bad.tar"
    digest = archive(path, name, kind, duplicate)
    with pytest.raises(ValueError):
        data.extract(path, tmp_path / "out", expected=digest)
    assert not (tmp_path / "out").exists()


def test_extracted_inventory(tmp_path):
    path = tmp_path / "good.tar"
    digest = archive(path)
    out, inventory = data.extract(path, tmp_path / "out", expected=digest)
    assert len(inventory) == 1
    data.extract(path, out, expected=digest)
    (out / "extra").write_text("extra")
    with pytest.raises(ValueError, match="inventory"):
        data.extract(path, out, expected=digest)
    (out / "extra").unlink()
    (out / "fxSKY.test.fits").unlink()
    (out / "fxSKY.test.fits").symlink_to(path)
    with pytest.raises(ValueError):
        data.extract(path, out, expected=digest)


def selection_row():
    return dict(
        dark=0,
        grism="300V",
        filter="FREE",
        sun_alt=-19,
        moon_alt=-6,
        airmass=1.0,
        gal_lat=20.0,
        exposure=60.0,
        wave=np.array([3650.0, 6500.0]),
    )


@pytest.mark.parametrize(
    "key,value,reason",
    [
        ("dark", 1, "dark_flag"),
        ("grism", "600B", "grism"),
        ("filter", "V", "filter"),
        ("sun_alt", -18.0, "twilight"),
        ("moon_alt", -5.0, "moon"),
        ("airmass", 1.51, "airmass"),
        ("gal_lat", 19.99, "galactic_latitude"),
        ("exposure", 59.9, "exposure"),
        ("wave", np.array([3651.0, 6500.0]), "coverage"),
    ],
)
def test_selection_frozen_boundaries(key, value, reason):
    row = selection_row()
    assert data.exclusion(row) is None
    row[key] = value
    assert data.exclusion(row) == reason


def test_identity_and_missing_prediction_rejection():
    rows, pred, _ = synthetic()
    assert set(align(rows, pred["native"])) == {"NSB Standard", "NSB Oxygen", "nsb2"}
    pred["native"]["nsb2"].pop()
    with pytest.raises(ValueError, match="cohort"):
        align(rows, pred["native"])
    rows, pred, _ = synthetic()
    pred["native"]["nsb2"].append(pred["native"]["nsb2"][0])
    with pytest.raises(ValueError):
        align(rows, pred["native"])
    rows, pred, _ = synthetic()
    pred["native"]["nsb2"][0]["total"] = float("nan")
    with pytest.raises(ValueError):
        align(rows, pred["native"])


def test_baseline_and_cluster_statistics():
    rows, pred, _ = synthetic()
    value = baseline(rows)
    for row in rows:
        if row["split"] == "evaluation":
            row["observed"] *= 100
    assert baseline(rows) == value
    rows[-1]["night"] = rows[0]["night"]
    with pytest.raises(ValueError):
        baseline(rows)
    y = np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    nights = np.array([1, 1, 2, 2, 3, 3])
    a = scores(y, {"a": y + 0.5, "b": y + 0.2}, nights)
    b = scores(y, {"a": y + 0.5, "b": y + 0.2}, nights)
    assert a == b
    assert a["paired"]["a_minus_b"]["mae_ci95"] == pytest.approx([0.3, 0.3])
    assert errors([1, 1], [1, 1])["rmse"] == 0
    with pytest.raises(ValueError):
        errors([0], [1])
    with pytest.raises(ValueError):
        scores(y, {"a": y}, [1] * 6)


def test_missing_predictions_invalidate_whole_scenario():
    state = prepare(Config())
    state["predictions"]["native"]["nsb2"].pop()
    analyze(state)
    assert "native" not in state["metrics"]
    assert "matched" in state["metrics"]
    assert "INVALID" in state["availability"]["native"]


def test_model_capability_and_source_identity():
    with pytest.raises(RuntimeError, match="source build"):
        capabilities(SimpleNamespace())
    capabilities(
        SimpleNamespace(
            AirglowModel=SimpleNamespace(PARANAL_PALACE_V1=1, PARANAL_PALACE_OXYGEN_V1=2),
            NsbEvaluator=SimpleNamespace(integrate_band_components=True),
        )
    )
    with pytest.raises(ValueError, match="full exact"):
        source_identity(".", "main")


def test_unverified_solar_never_substituted(tmp_path):
    p = tmp_path / "solar.json"
    p.write_text('{"daily": {"2000-01-01":130}}')
    with pytest.raises(ValueError, match="Unverified"):
        data.solar_windows(p, [])


def test_manifest_integrity_and_inventory(tmp_path):
    path = tmp_path / "run"
    path.mkdir()
    (path / "file.json").write_text("{}")
    seal(path, {"test": True})
    verify(path)
    with pytest.raises(FileExistsError):
        seal(path, {})
    (path / "extra").write_text("extra")
    with pytest.raises(ValueError, match="inventory"):
        verify(path)
    (path / "extra").unlink()
    (path / "file.json").write_text("changed")
    with pytest.raises(ValueError, match="checksum"):
        verify(path)


def test_notebook_schema_and_offline_execution(tmp_path, monkeypatch):
    notebook = Path(__file__).resolve().parents[1] / "nsb-vs-nsb2.ipynb"
    nb = nbformat.read(notebook, as_version=4)
    nbformat.validate(nb)
    assert all(
        not c.get("outputs") and c.get("execution_count") is None
        for c in nb.cells
        if c.cell_type == "code"
    )
    # Use this exact locked interpreter in a temporary kernel registration.
    kernels = tmp_path / "jupyter" / "kernels" / "validation-test"
    kernels.mkdir(parents=True)
    (kernels / "kernel.json").write_text(
        json.dumps(
            {
                "argv": [sys.executable, "-m", "ipykernel_launcher", "-f", "{connection_file}"],
                "display_name": "test",
                "language": "python",
            }
        )
    )
    monkeypatch.setenv("JUPYTER_PATH", str(tmp_path / "jupyter"))
    monkeypatch.setenv("NSB_VALIDATION_MODE", "offline")
    monkeypatch.setenv("NSB_VALIDATION_OUTPUT", str(tmp_path / "results"))
    # Forbid scientific HTTP use in kernel. IPC sockets remain available to Jupyter.
    nb.cells.insert(
        0,
        nbformat.v4.new_code_cell(
            "import urllib.request\ndef no_network(*a, **k): raise AssertionError('offline network access')\nurllib.request.urlopen=no_network"
        ),
    )
    NotebookClient(
        nb,
        timeout=180,
        kernel_name="validation-test",
        resources={"metadata": {"path": str(tmp_path)}},
    ).execute()
    runs = list((tmp_path / "results").glob("run-*"))
    assert len(runs) == 1
    verify(runs[0])
    assert len(list((runs[0] / "figures").glob("*.png"))) >= 8
    admission = json.loads((runs[0] / "admission.json").read_text())
    assert not admission["independent_validation"] and not admission["experiment_complete"]
    assert "SYNTHETIC" in admission["classification"]
    assert all(
        (runs[0] / p).exists()
        for p in ("report.md", "report.html", "predictions.json", "metrics.json")
    )


def test_nsb2_template_source_uses_public_resolver(monkeypatch):
    """A spectral-coordinate array is not itself a wavelength spectrum."""
    from astropy.coordinates import EarthLocation

    from nsb_observational.models import Models

    model = Models.__new__(Models)
    model.site = EarthLocation.from_geodetic(-70 * u.deg, -24 * u.deg, 2600 * u.m)
    called = []
    spectral = SimpleNamespace(wvl=np.array([300.0, 400.0, 650.0, 700.0]) * u.nm)

    class Grid:
        wvl = spectral.wvl

        def __call__(self, coords):
            return np.ones((1, 1)) / u.cm**2 / u.ns / u.sr / u.nm

    class Field:
        spectral_grid = Grid()
        spectral_data = np.zeros((1, 1))
        weights = np.ones((1, 1))

        def resolve_spectra(self, band):
            called.append(band)
            return SimpleNamespace(integrate=lambda: 2e8 / u.s / u.cm**2 / u.sr)

    source = SimpleNamespace(query_direct=lambda *a: (Field(), None))
    monkeypatch.setitem(sys.modules, "nsb2.core.spectral", SimpleNamespace(Bandpass=lambda *a: a))
    row = dict(mjd=60000.0, ra=90.0, dec=-30.0)
    assert model.source_integral(source, row) == pytest.approx(0.2)
    assert len(called) == 1


def test_receipt_verifies_imported_native_and_rejects_tampering(tmp_path, monkeypatch):
    import zipfile

    from nsb_observational import runtime

    root = tmp_path / "site-packages"
    (root / "nsb").mkdir(parents=True)
    native = root / "nsb" / "nsb.abi3.so"
    wrapper = root / "nsb" / "__init__.py"
    native.write_bytes(b"native-fixture")
    wrapper.write_bytes(b"wrapper-fixture")
    wheel = tmp_path / "nsb.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.write(native, "nsb/nsb.abi3.so")
        archive.write(wrapper, "nsb/__init__.py")
    info = dict(
        mode="source",
        commit="a" * 40,
        source_clean=True,
        wheel=str(wheel),
        wheel_sha256=data.sha256(wheel),
        installed_files=runtime.wheel_files(wheel),
        version="test",
    )
    receipt = tmp_path / "receipt.json"
    receipt.write_text(json.dumps(info))
    module = SimpleNamespace(
        __file__=str(wrapper),
        AirglowModel=SimpleNamespace(PARANAL_PALACE_V1=1, PARANAL_PALACE_OXYGEN_V1=2),
        NsbEvaluator=SimpleNamespace(integrate_band_components=True),
    )
    monkeypatch.setitem(sys.modules, "nsb", module)
    monkeypatch.setitem(sys.modules, "nsb.nsb", SimpleNamespace(__file__=str(native)))
    monkeypatch.setattr(
        runtime.metadata,
        "distribution",
        lambda name: SimpleNamespace(locate_file=lambda name: root / name),
    )
    monkeypatch.setattr(runtime.metadata, "version", lambda name: "test")
    verified = runtime.verify_runtime(receipt, mode="source", commit="a" * 40)
    assert verified["native_extension_sha256"]["nsb.abi3.so"] == data.sha256(native)
    native.write_bytes(b"changed")
    with pytest.raises(ValueError, match="differs from built wheel"):
        runtime.verify_runtime(receipt, mode="source", commit="a" * 40)
    with pytest.raises(ValueError, match="mode"):
        runtime.verify_runtime(receipt, mode="published", package_version="test")


def test_source_receipt_requires_clean_exact_head(tmp_path):
    import subprocess

    from nsb_observational.runtime import command

    subprocess.run(["git", "init", "-q", str(tmp_path)], check=True)
    (tmp_path / "Cargo.lock").write_text(
        'version = 3\n[[package]]\nname = "tempoch"\nversion = "0.11.1"\n'
    )
    subprocess.run(["git", "-C", str(tmp_path), "add", "Cargo.lock"], check=True)
    subprocess.run(
        [
            "git",
            "-C",
            str(tmp_path),
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
        check=True,
    )
    commit = command("git", "rev-parse", "HEAD", cwd=tmp_path)
    assert source_identity(tmp_path, commit)["rust_providers"]["tempoch"] == "0.11.1"
    (tmp_path / "untracked").write_text("dirty")
    with pytest.raises(ValueError, match="clean"):
        source_identity(tmp_path, commit)


def test_manifest_digest_and_symlink_rejection(tmp_path):
    (tmp_path / "data.json").write_text("{}")
    seal(tmp_path, {})
    original = (tmp_path / "manifest.json").read_bytes()
    (tmp_path / "manifest.json").write_text("{}")
    with pytest.raises(ValueError, match="Manifest checksum"):
        verify(tmp_path)
    (tmp_path / "manifest.json").write_bytes(original)
    (tmp_path / "data.json").unlink()
    (tmp_path / "data.json").symlink_to(tmp_path / "manifest.json")
    with pytest.raises(ValueError, match="Symlink"):
        verify(tmp_path)
