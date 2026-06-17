use siderust::coordinates::centers::Geodetic;
use siderust::coordinates::frames::{EquatorialMeanJ2000, ECEF};
use siderust::coordinates::spherical::Direction as SphericalDirection;
use siderust::event::horizontal::star_horizontal;
use siderust::qtty::Degrees;
use tempoch::{Time, JD, TT, UTC};

pub(crate) fn target_altitude(
    time: Time<UTC>,
    location: Geodetic<ECEF>,
    target: SphericalDirection<EquatorialMeanJ2000>,
) -> Degrees {
    let jd = time.to::<TT>().to::<JD>();
    star_horizontal(target.ra(), target.dec(), &location, jd).alt()
}
