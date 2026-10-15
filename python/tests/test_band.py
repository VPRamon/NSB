"""Independent sub-band contracts; same scientific model as the public integral."""
from datetime import datetime, timezone
import math
import nsb
import pytest
import siderust


def query(components=None):
    return nsb.PointQuery(
        siderust.Observer(lon_deg=-70.4045,lat_deg=-24.6272,height_m=2635),
        datetime(2023,9,15,5,tzinfo=timezone.utc),
        siderust.Direction(ra_deg=10,dec_deg=-20),
        components=components if components is not None else nsb.ComponentMask.ZODIACAL | nsb.ComponentMask.AIRGLOW)


def test_full_band_matches_existing_integral_and_subbands_add():
    evaluator=nsb.NsbEvaluator(nsb.NsbModelConfig.generic_clear_sky())
    q=query()
    full=evaluator.integrate_band(q,300,650)
    assert math.isfinite(full) and full>0
    assert full==pytest.approx(evaluator.evaluate(q).integrated_photons_cm2_ns_sr,rel=1e-12)
    assert evaluator.integrate_band(q,300,450.25)+evaluator.integrate_band(q,450.25,650)==pytest.approx(full,rel=1e-12)
    assert 0 < evaluator.integrate_band(q,360,570)<full


@pytest.mark.parametrize('lower,upper',[(299,650),(300,651),(650,300),(400,400),(float('nan'),550)])
def test_invalid_domain_errors(lower,upper):
    with pytest.raises(nsb.NsbError):
        nsb.NsbEvaluator().integrate_band(query(),lower,upper)


def test_integrated_starlight_cannot_be_reconstructed():
    with pytest.raises(nsb.UnsupportedError,match='starlight'):
        nsb.NsbEvaluator().integrate_band(query(nsb.ComponentMask.STARLIGHT),360,570)
