import nsb


def test_import_and_versions_are_exposed():
    assert nsb.__version__
    assert nsb.MODEL_VERSION.startswith("nsb-model-")
    assert nsb.SIDERUST_VERSION == "0.12.0"
    assert nsb.SIDERUST_SOURCE == (
        "git:https://github.com/Siderust/siderust"
        "?branch=96-specialize-icrs-altitude-events"
    )
