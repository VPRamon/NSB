# SPDX-License-Identifier: AGPL-3.0-only
# Copyright (C) 2026 Vallés Puig, Ramon
# Tests execute definitions from the distributed notebook, never a companion module.
import ast
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

NOTEBOOK = Path(__file__).resolve().parents[1] / "NSB_Validation.ipynb"
nb = nbformat.read(NOTEBOOK, as_version=4)
namespace = {
    "__name__": "notebook_test",
    "MODE": "synthetic",
    "RUNTIME": "developer",
    "DEPENDENCIES": {
        "numpy": "2.5.4",
        "astropy": "8.0.2",
        "matplotlib": "3.11.2",
        "siderust": "0.5.1",
        "tempoch": "0.5.0",
    },
    "NSB2_COMMIT": "bc9320db03fbff69997ce366c6e76a37339c5d00",
}
# Import statements and definitions are safe to load without installing/running models.
# Retain literal constants needed by numerical functions; execution cells stay excluded.
definition_tags = {
    "runtime",
    "acquisition",
    "measurement",
    "solar",
    "synthetic",
    "source_spectra",
    "models",
    "statistics",
    "records",
}
for cell in nb.cells:
    if cell.cell_type != "code":
        continue
    tags = set(cell.metadata.get("tags", []))
    tree = ast.parse(cell.source)
    if tags in ({"setup"}, {"configuration"}, {"imports"}):
        nodes = [n for n in tree.body if isinstance(n, (ast.Import, ast.ImportFrom))]
    elif tags & definition_tags:
        nodes = []
        for node in tree.body:
            if isinstance(node, (ast.FunctionDef, ast.ClassDef)):
                nodes.append(node)
            elif isinstance(node, ast.Assign):
                try:
                    ast.literal_eval(node.value)
                    nodes.append(node)
                except (ValueError, TypeError):
                    if tags & {"acquisition", "synthetic"}:
                        nodes.append(node)
    else:
        continue
    exec(
        compile(ast.Module(body=nodes, type_ignores=[]), "<notebook definitions>", "exec"),
        namespace,
    )
data = runtime = SimpleNamespace(**namespace)
synthetic = namespace["synthetic"]
align = namespace["align"]
baseline = namespace["baseline"]
errors = namespace["errors"]
scores = namespace["scores"]
capabilities = namespace["capabilities"]
Models = namespace["Models"]
make_record = namespace["make_record"]
compare_records = namespace["compare_records"]
save_record = namespace["save_record"]
load_record = namespace["load_record"]
verify_wheel = namespace["verify_wheel"]


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


def test_model_capability():
    with pytest.raises(RuntimeError, match="source build|developer mode"):
        capabilities(SimpleNamespace())
    capabilities(
        SimpleNamespace(
            AirglowModel=SimpleNamespace(PARANAL_PALACE_V1=1, PARANAL_PALACE_OXYGEN_V1=2),
            NsbEvaluator=SimpleNamespace(integrate_band_components=True),
        )
    )


def test_unverified_solar_never_substituted(tmp_path):
    p = tmp_path / "solar.json"
    p.write_text('{"daily": {"2000-01-01":130}}')
    with pytest.raises(ValueError, match="Unverified"):
        data.solar_windows(p, [])


def test_nsb2_template_source_uses_public_resolver(monkeypatch):
    """A spectral-coordinate array is not itself a wavelength spectrum."""
    from astropy.coordinates import EarthLocation

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


def test_notebook_single_file_offline(tmp_path, monkeypatch):
    import shutil

    copy = tmp_path / "NSB_Validation.ipynb"
    shutil.copyfile(NOTEBOOK, copy)
    book = nbformat.read(copy, as_version=4)
    nbformat.validate(book)
    assert all(
        not c.get("outputs") and c.get("execution_count") is None
        for c in book.cells
        if c.cell_type == "code"
    )
    assert "nsb_observational" not in copy.read_text()
    kernels = tmp_path / "jupyter/kernels/validation-test"
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
    monkeypatch.setenv("NSB_VALIDATION_MODE", "synthetic")
    monkeypatch.setenv("NSB_VALIDATION_INSTALL", "0")
    monkeypatch.setenv("NSB_VALIDATION_OUTPUT", str(tmp_path / "results"))
    book.cells.insert(
        0,
        nbformat.v4.new_code_cell(
            "import urllib.request, subprocess\n"
            "def forbidden(*a, **k): raise AssertionError('offline network/install')\n"
            "urllib.request.urlopen = forbidden\nsubprocess.run = forbidden"
        ),
    )
    # No checkout, relative module or editable package is reachable from cwd.
    NotebookClient(
        book,
        timeout=240,
        kernel_name="validation-test",
        resources={"metadata": {"path": str(tmp_path)}},
    ).execute()
    records = list((tmp_path / "results").glob("*.json"))
    assert len(records) == 1
    record = load_record(records[0])
    assert record["mode"] == "synthetic" and not record["complete"]
    assert "SYNTHETIC" in record["classification"]
    assert not record["independent_validation"]
    assert set(record["metrics"]) == {"native", "matched"}
    assert len(list((tmp_path / "results").glob("*.png"))) == 3
    assert (
        sum(
            out.output_type == "display_data" and "image/png" in out.data
            for cell in book.cells
            if cell.cell_type == "code"
            for out in cell.outputs
        )
        >= 4
    )


def comparison_fixture():
    rows, predictions, _ = synthetic()
    return {
        "schema": 1,
        "mode": "observational",
        "complete": True,
        "protocol": {"name": "TEST ONLY", "scenarios": ["native", "matched"]},
        "observations": [{k: v for k, v in r.items() if k not in ("wave", "energy")} for r in rows],
        "predictions": predictions,
    }


def test_version_comparison_paired_and_incompatible(tmp_path):
    import copy

    # This is a simulated record, explicitly not evidence from two real releases.
    previous = comparison_fixture()
    namespace.update(REPETITIONS=2000, SEED=182)
    result = compare_records(previous, copy.deepcopy(previous))
    for scenario in result.values():
        for score in scenario.values():
            assert score["relative_mae_difference"] == 0
            assert score["relative_mae_ci95"] == [0, 0]
    current = copy.deepcopy(previous)
    for row in current["predictions"]["matched"]["NSB Oxygen"]:
        row["total"] *= 0.8
    diff = compare_records(previous, current)["matched"]["NSB Oxygen"]
    assert diff["relative_mae_difference"] > 0
    assert diff["relative_mae_ci95"][0] > 0
    for mutation in (
        lambda r: r["protocol"].update(band_nm=[300, 650]),
        lambda r: r["observations"][0].update(split="evaluation"),
        lambda r: r["observations"][0].update(observed=99),
        lambda r: r.update(mode="synthetic"),
        lambda r: r.update(complete=False),
    ):
        changed = copy.deepcopy(previous)
        mutation(changed)
        with pytest.raises(ValueError):
            compare_records(previous, changed)
    path = save_record(previous, tmp_path)
    assert load_record(path) == previous
    path.write_text("{}")
    with pytest.raises(ValueError, match="SHA-256"):
        load_record(path)


def test_cluster_bootstrap_independent_unequal_night_oracle():
    # Unequal cluster sizes distinguish whole-night from per-spectrum resampling,
    # and equal-spectrum estimand from equal-night averaging.
    y = np.array([1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    p = y + np.array([0.1, 0.1, 0.5, 0.9, 0.9, 0.9])
    nights = np.array([1, 1, 2, 3, 3, 3])
    result = scores(y, {"a": p, "b": y}, nights)
    random = np.random.default_rng(182)
    samples = []
    for _ in range(2000):
        selected = random.choice([1, 2, 3], size=3, replace=True)
        sample = [j for night in selected for j in range(len(y)) if nights[j] == night]
        samples.append(np.mean(abs(p[sample] - y[sample]) / y[sample]))
    expected = np.quantile(samples, [0.025, 0.975])
    assert result["models"]["a"]["relative_mae_ci95"] == pytest.approx(expected)
    assert result["paired"]["a_minus_b"]["relative_mae_ci95"] == pytest.approx(expected)


def test_wheel_identity_and_tampering(tmp_path, monkeypatch):
    import zipfile

    root = tmp_path / "site-packages"
    (root / "nsb").mkdir(parents=True)
    native = root / "nsb/nsb.abi3.so"
    wrapper = root / "nsb/__init__.py"
    native.write_bytes(b"native-fixture")
    wrapper.write_bytes(b"wrapper-fixture")
    wheel = tmp_path / "fixture.whl"
    with zipfile.ZipFile(wheel, "w") as archive:
        archive.write(native, "nsb/nsb.abi3.so")
        archive.write(wrapper, "nsb/__init__.py")
    monkeypatch.setitem(
        sys.modules,
        "nsb",
        SimpleNamespace(
            __file__=str(wrapper),
            AirglowModel=SimpleNamespace(PARANAL_PALACE_V1=1, PARANAL_PALACE_OXYGEN_V1=2),
            NsbEvaluator=SimpleNamespace(integrate_band_components=True),
        ),
    )
    monkeypatch.setitem(sys.modules, "nsb.nsb", SimpleNamespace(__file__=str(native)))
    monkeypatch.setattr(
        namespace["metadata"],
        "distribution",
        lambda name: SimpleNamespace(version="test", locate_file=lambda name: root / name),
    )
    digest = data.sha256(wheel)
    assert verify_wheel(wheel, digest, "test")["wheel_sha256"] == digest
    native.write_bytes(b"changed")
    with pytest.raises(ValueError, match="differs from selected wheel"):
        verify_wheel(wheel, digest, "test")


def test_missing_model_invalidates_whole_scenario(monkeypatch):
    rows, predictions, _ = synthetic()
    predictions["native"]["nsb2"].pop()
    selected = np.array([i for i, r in enumerate(rows) if r["split"] == "evaluation"])
    context = dict(namespace)
    context.update(
        rows=rows,
        predictions=predictions,
        evaluation=selected,
        y=np.array([rows[i]["observed"] for i in selected]),
        provenance={},
        availability={"native": "available", "matched": "available"},
        metrics={},
        REPETITIONS=2000,
        SEED=182,
        PRACTICAL_MARGIN=0.10,
        LABEL="SYNTHETIC",
    )
    context["display"] = lambda *args: None
    cell = next(c for c in nb.cells if c.metadata.get("tags") == ["analyze"])
    exec(compile(cell.source, "<notebook analyze>", "exec"), context)
    assert "native" not in context["metrics"]
    assert "matched" in context["metrics"]
    assert "INVALID" in context["availability"]["native"]


def test_complete_centered_solar_window(tmp_path, monkeypatch):
    from datetime import date, timedelta

    center = date(2000, 4, 9)
    dates = [center + timedelta(days=i) for i in range(-13, 14)]
    source = tmp_path / "solar.txt"
    source.write_text("\n".join(f"{day:%Y%m%d} {90 + i}" for i, day in enumerate(dates)))
    monkeypatch.setitem(namespace, "SOLAR_SHA", data.sha256(source))
    mjd = namespace["Time"](center.isoformat()).mjd
    assert data.solar_windows(source, [{"id": "test", "mjd": mjd}])["test"] == 103
    source.write_text("\n".join(f"{day:%Y%m%d} {90 + i}" for i, day in enumerate(dates[1:])))
    monkeypatch.setitem(namespace, "SOLAR_SHA", data.sha256(source))
    with pytest.raises(ValueError, match="Incomplete centered"):
        data.solar_windows(source, [{"id": "test", "mjd": mjd}])


def test_scientific_signature_tracks_executed_algorithms():
    signature = namespace["scientific_signature"]
    sources = [cell.source for cell in nb.cells if cell.cell_type == "code"]
    expected = signature(sources)
    assert signature(["%matplotlib inline", *sources]) == expected
    # A subsequent interactive override must change compatibility, even if
    # the notebook on disk and manually assigned protocol name are unchanged.
    assert signature([*sources, "def measured_radiance(*args): return 0"]) != expected
    with pytest.raises(ValueError, match="Run all"):
        signature([])
