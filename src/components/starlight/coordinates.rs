use crate::evaluator::Target;
use qtty::angular::Degrees;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GalacticCoordinates {
    pub lon: Degrees,
    pub lat: Degrees,
}

pub fn equatorial_to_galactic(target: Target) -> GalacticCoordinates {
    let ra = target.ra().value().to_radians();
    let dec = target.dec().value().to_radians();
    let cos_dec = dec.cos();
    let eq = [cos_dec * ra.cos(), cos_dec * ra.sin(), dec.sin()];

    // IAU J2000/ICRS equatorial to Galactic rotation matrix.
    let gal = [
        -0.054_875_560_416_215_4 * eq[0]
            - 0.873_437_090_234_885_1 * eq[1]
            - 0.483_835_015_548_713_2 * eq[2],
        0.494_109_427_875_583_7 * eq[0] - 0.444_829_629_960_011_2 * eq[1]
            + 0.746_982_244_580_286_6 * eq[2],
        -0.867_666_149_019_004_7 * eq[0] - 0.198_076_373_431_201_5 * eq[1]
            + 0.455_983_776_175_066_9 * eq[2],
    ];

    let mut lon = gal[1].atan2(gal[0]).to_degrees();
    if lon < 0.0 {
        lon += 360.0;
    }
    let lat = gal[2].clamp(-1.0, 1.0).asin().to_degrees();

    GalacticCoordinates {
        lon: Degrees::new(lon),
        lat: Degrees::new(lat),
    }
}
