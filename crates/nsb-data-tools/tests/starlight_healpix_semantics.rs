use nsb_data_tools::starlight::healpix::{
    gaia_source_id_equatorial_nested_pixel, galactic_nested_pixel_from_icrs_position,
    galactic_nested_to_ring, legacy_equatorial_bitshift_mislabelled_as_galactic_pixel,
    IcrsSkyPosition,
};

const NSIDE: u32 = 128;

/// Independent integer NESTED -> RING reference path.
fn reference_nest2ring(nside: u32, ipnest: u64) -> u64 {
    assert!(nside.is_power_of_two() && nside > 0);
    let nside = i64::from(nside);
    let npface = nside * nside;
    let npix = 12 * npface;
    let ipnest = i64::try_from(ipnest).expect("nested index fits i64");
    assert!((0..npix).contains(&ipnest));

    const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
    const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

    let face = usize::try_from(ipnest / npface).expect("face fits usize");
    let ipf = u64::try_from(ipnest % npface).expect("face-local index fits u64");
    let mut ix = 0_u64;
    let mut iy = 0_u64;
    for bit in 0..32_u32 {
        ix |= ((ipf >> (2 * bit)) & 1) << bit;
        iy |= ((ipf >> (2 * bit + 1)) & 1) << bit;
    }
    let ix = i64::try_from(ix).expect("x fits i64");
    let iy = i64::try_from(iy).expect("y fits i64");

    let jr = JRLL[face] * nside - ix - iy - 1;
    let nl4 = 4 * nside;
    let (nr, n_before, kshift) = if jr < nside {
        let nr = jr;
        (nr, 2 * nr * (nr - 1), 0)
    } else if jr > 3 * nside {
        let nr = nl4 - jr;
        (nr, npix - 2 * nr * (nr + 1), 0)
    } else {
        (
            nside,
            2 * nside * (nside - 1) + (jr - nside) * nl4,
            (jr - nside) & 1,
        )
    };

    let mut jp = (JPLL[face] * nr + ix - iy + 1 + kshift) / 2;
    if jp > nl4 {
        jp -= nl4;
    }
    if jp < 1 {
        jp += nl4;
    }

    u64::try_from(n_before + jp - 1).expect("RING index is non-negative")
}

#[test]
fn gaia_source_id_equatorial_and_galactic_pixels_are_not_interchangeable() {
    let source_id = (98_765_u64 << 35) | 9;
    let equatorial = gaia_source_id_equatorial_nested_pixel(source_id, NSIDE).unwrap();
    let position = IcrsSkyPosition::new(123.45, -12.34).unwrap();
    let galactic =
        galactic_nested_pixel_from_icrs_position(position.ra_deg, position.dec_deg, NSIDE).unwrap();
    let encoded_equatorial =
        legacy_equatorial_bitshift_mislabelled_as_galactic_pixel(source_id, NSIDE).unwrap();
    assert_ne!(equatorial, galactic);
    assert_ne!(encoded_equatorial, galactic);
}

#[test]
fn production_nest2ring_matches_independent_reference_on_samples() {
    for nest in [0_u64, 1, 42, 1_234, 12_345] {
        assert_eq!(
            reference_nest2ring(NSIDE, nest),
            galactic_nested_to_ring(NSIDE, nest).unwrap()
        );
    }
}
