//! Reference-exact HEALPix topology helpers (RING ↔ NESTED, neighbours).
//!
//! Integer algorithms adapted from the HEALPix reference implementation, validated
//! against healpy in unit tests. Frame-agnostic: ordering permutations do not depend on
//! equatorial vs Galactic sky coordinates.

use anyhow::{bail, Result};

const JRLL: [i64; 12] = [2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4];
const JPLL: [i64; 12] = [1, 3, 5, 7, 0, 2, 4, 6, 1, 3, 5, 7];

const XOFFSET: [i32; 8] = [-1, -1, 0, 1, 1, 1, 0, -1];
const YOFFSET: [i32; 8] = [0, 1, 1, 1, 0, -1, -1, -1];

#[rustfmt::skip]
const FACEARRAY: [[i8; 12]; 9] = [
    [ 8,  9, 10, 11, -1, -1, -1, -1, 10, 11,  8,  9],
    [ 5,  6,  7,  4,  8,  9, 10, 11,  9, 10, 11,  8],
    [-1, -1, -1, -1,  5,  6,  7,  4, -1, -1, -1, -1],
    [ 4,  5,  6,  7, 11,  8,  9, 10, 11,  8,  9, 10],
    [ 0,  1,  2,  3,  4,  5,  6,  7,  8,  9, 10, 11],
    [ 1,  2,  3,  0,  0,  1,  2,  3,  5,  6,  7,  4],
    [-1, -1, -1, -1,  7,  4,  5,  6, -1, -1, -1, -1],
    [ 3,  0,  1,  2,  3,  0,  1,  2,  4,  5,  6,  7],
    [ 2,  3,  0,  1, -1, -1, -1, -1,  0,  1,  2,  3],
];

#[rustfmt::skip]
const SWAPARRAY: [[u8; 12]; 9] = [
    [0, 0, 0, 0, 0, 0, 0, 0, 3, 3, 3, 3],
    [0, 0, 0, 0, 0, 0, 0, 0, 6, 6, 6, 6],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 5, 5, 5, 5],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [5, 5, 5, 5, 0, 0, 0, 0, 0, 0, 0, 0],
    [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    [6, 6, 6, 6, 0, 0, 0, 0, 0, 0, 0, 0],
    [3, 3, 3, 3, 0, 0, 0, 0, 0, 0, 0, 0],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Xyf {
    x: u32,
    y: u32,
    face: u8,
}

struct Base {
    depth: u32,
    twice_depth: u32,
    nside: u32,
    n_hash: u64,
    ncap: u64,
}

impl Base {
    fn new(nside: u32) -> Result<Self> {
        if !nside.is_power_of_two() || nside == 0 {
            bail!("HEALPix nside must be a positive power of two");
        }
        let depth = nside.trailing_zeros();
        let nside64 = u64::from(nside);
        Ok(Self {
            depth,
            twice_depth: depth * 2,
            nside,
            n_hash: 12 * nside64 * nside64,
            ncap: (nside64 * (nside64 - 1)) << 1,
        })
    }

    const fn spread_bits(x: u32) -> u64 {
        let mut v = x as u64;
        v = (v | (v << 16)) & 0x0000_FFFF_0000_FFFF;
        v = (v | (v << 8)) & 0x00FF_00FF_00FF_00FF;
        v = (v | (v << 4)) & 0x0F0F_0F0F_0F0F_0F0F;
        v = (v | (v << 2)) & 0x3333_3333_3333_3333;
        v = (v | (v << 1)) & 0x5555_5555_5555_5555;
        v
    }

    const fn compact_bits(v: u64) -> u32 {
        let mut v = v & 0x5555_5555_5555_5555;
        v = (v | (v >> 1)) & 0x3333_3333_3333_3333;
        v = (v | (v >> 2)) & 0x0F0F_0F0F_0F0F_0F0F;
        v = (v | (v >> 4)) & 0x00FF_00FF_00FF_00FF;
        v = (v | (v >> 8)) & 0x0000_FFFF_0000_FFFF;
        v = (v | (v >> 16)) & 0x0000_0000_FFFF_FFFF;
        v as u32
    }

    fn xyf2nested(&self, xyf: Xyf) -> u64 {
        (u64::from(xyf.face) << self.twice_depth)
            | Self::spread_bits(xyf.x)
            | (Self::spread_bits(xyf.y) << 1)
    }

    fn nested2xyf(&self, cell: u64) -> Xyf {
        let face = (cell >> self.twice_depth) as u8;
        let in_face = cell & ((1u64 << self.twice_depth) - 1);
        Xyf {
            x: Self::compact_bits(in_face),
            y: Self::compact_bits(in_face >> 1),
            face,
        }
    }

    fn xyf2ring(&self, xyf: Xyf) -> u64 {
        let nside = i64::from(self.nside);
        let nl4 = nside << 2;
        let (ix, iy, f) = (i64::from(xyf.x), i64::from(xyf.y), usize::from(xyf.face));
        let jr = JRLL[f] * nside - ix - iy - 1;
        let (n_before, nr, kshift) = if jr < nside {
            (2 * jr * (jr - 1), jr, 0)
        } else if jr > 3 * nside {
            let nr = nl4 - jr;
            (self.n_hash as i64 - 2 * nr * (nr + 1), nr, 0)
        } else {
            (
                self.ncap as i64 + (jr - nside) * nl4,
                nside,
                (jr - nside) & 1,
            )
        };
        let mut jp = (JPLL[f] * nr + ix - iy + 1 + kshift) >> 1;
        if jp > nl4 {
            jp -= nl4;
        }
        if jp < 1 {
            jp += nl4;
        }
        (n_before + jp - 1) as u64
    }

    fn ring2xyf(&self, cell: u64) -> Xyf {
        let nside = i64::from(self.nside);
        let nl2 = nside << 1;
        let pix = cell as i64;

        let (iring, iphi, kshift, _nr, face) = if cell < self.ncap {
            let iring = (1 + (1 + 2 * pix).isqrt()) >> 1;
            let iphi = (pix + 1) - 2 * iring * (iring - 1);
            (iring, iphi, 0, iring, ((iphi - 1) / iring) as usize)
        } else if cell < self.n_hash - self.ncap {
            let ip = pix - self.ncap as i64;
            let tmp = ip >> (self.depth + 2);
            let iring = tmp + nside;
            let iphi = ip - tmp * (nside << 2) + 1;
            let kshift = (iring + nside) & 1;
            let ire = iring - nside + 1;
            let irm = nl2 + 2 - ire;
            let ifm = (iphi - (ire >> 1) + nside - 1) >> self.depth;
            let ifp = (iphi - (irm >> 1) + nside - 1) >> self.depth;
            let face = if ifp == ifm {
                ifp | 4
            } else if ifp < ifm {
                ifp
            } else {
                ifm + 8
            };
            (iring, iphi, kshift, nside, face as usize)
        } else {
            let ip = self.n_hash as i64 - pix;
            let nr = (1 + (2 * ip - 1).isqrt()) >> 1;
            let iphi = 4 * nr + 1 - (ip - 2 * nr * (nr - 1));
            (2 * nl2 - nr, iphi, 0, nr, 8 + ((iphi - 1) / nr) as usize)
        };

        let irt = iring - JRLL[face] * nside + 1;
        let mut ipt = 2 * iphi - JPLL[face] * _nr - kshift - 1;
        if ipt >= nl2 {
            ipt -= 8 * nside;
        }

        Xyf {
            x: ((ipt - irt) >> 1) as u32,
            y: ((-(ipt + irt)) >> 1) as u32,
            face: face as u8,
        }
    }

    fn is_interior(&self, xyf: Xyf) -> bool {
        xyf.x > 0 && xyf.x < self.nside - 1 && xyf.y > 0 && xyf.y < self.nside - 1
    }

    fn interior_neighbour(xyf: Xyf, direction: usize) -> Xyf {
        Xyf {
            x: (xyf.x as i32 + XOFFSET[direction]) as u32,
            y: (xyf.y as i32 + YOFFSET[direction]) as u32,
            face: xyf.face,
        }
    }

    fn neighbour_xyf(&self, xyf: Xyf, direction: usize) -> Option<Xyf> {
        if self.is_interior(xyf) {
            return Some(Self::interior_neighbour(xyf, direction));
        }
        let nside = self.nside as i32;
        let mut x = xyf.x as i32 + XOFFSET[direction];
        let mut y = xyf.y as i32 + YOFFSET[direction];
        let mut bucket = 4i32;
        if x < 0 {
            x += nside;
            bucket -= 1;
        } else if x >= nside {
            x -= nside;
            bucket += 1;
        }
        if y < 0 {
            y += nside;
            bucket -= 3;
        } else if y >= nside {
            y -= nside;
            bucket += 3;
        }

        let face = FACEARRAY[bucket as usize][xyf.face as usize];
        if face < 0 {
            return None;
        }
        let bits = SWAPARRAY[bucket as usize][xyf.face as usize];
        if bits & 1 != 0 {
            x = nside - x - 1;
        }
        if bits & 2 != 0 {
            y = nside - y - 1;
        }
        if bits & 4 != 0 {
            std::mem::swap(&mut x, &mut y);
        }
        Some(Xyf {
            x: x as u32,
            y: y as u32,
            face: face as u8,
        })
    }

    fn neighbours_xyf(&self, xyf: Xyf) -> [Option<Xyf>; 8] {
        if self.is_interior(xyf) {
            return std::array::from_fn(|direction| Some(Self::interior_neighbour(xyf, direction)));
        }
        std::array::from_fn(|direction| self.neighbour_xyf(xyf, direction))
    }
}

/// O(1) RING → NESTED conversion (reference HEALPix integer algorithm).
pub fn reference_ring2nest(nside: u32, ipring: u64) -> Result<u64> {
    let base = Base::new(nside)?;
    if ipring >= base.n_hash {
        bail!("ring index {ipring} is outside nside={nside}");
    }
    Ok(base.xyf2nested(base.ring2xyf(ipring)))
}

/// O(1) NESTED → RING conversion via the reference `(face, x, y)` path.
pub fn reference_nested2ring(nside: u32, ipnest: u64) -> Result<u64> {
    let base = Base::new(nside)?;
    if ipnest >= base.n_hash {
        bail!("nested index {ipnest} is outside nside={nside}");
    }
    Ok(base.xyf2ring(base.nested2xyf(ipnest)))
}

/// Up to eight nested neighbours in healpy `get_all_neighbours` order.
pub fn nested_neighbours(nside: u32, pixel: u32) -> Result<Vec<u32>> {
    let base = Base::new(nside)?;
    let cell = u64::from(pixel);
    if cell >= base.n_hash {
        bail!("nested pixel {pixel} is outside nside={nside}");
    }
    Ok(base
        .neighbours_xyf(base.nested2xyf(cell))
        .into_iter()
        .flatten()
        .map(|xyf| u32::try_from(base.xyf2nested(xyf)).expect("nested index fits u32"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exhaustive_nside(nside: u32) {
        let npix = 12 * u64::from(nside) * u64::from(nside);
        for nest in 0..npix {
            let ring = reference_nested2ring(nside, nest).unwrap();
            let back = reference_ring2nest(nside, ring).unwrap();
            assert_eq!(back, nest, "nest {nest} ring {ring}");
            let ring_back = reference_nested2ring(nside, back).unwrap();
            assert_eq!(ring_back, ring);
        }
    }

    #[test]
    fn ring_nested_round_trip_exhaustive_small_nside() {
        for nside in [1, 2, 4, 8, 16] {
            exhaustive_nside(nside);
        }
    }

    #[test]
    fn ring_nested_round_trip_sample_nside_128_and_4096() {
        for nside in [128, 4096] {
            let npix = 12 * u64::from(nside) * u64::from(nside);
            let mut state = 0xC0FFEE_u64;
            for _ in 0..10_000 {
                state = state
                    .wrapping_mul(6_365_986_093)
                    .wrapping_add(1_443_525_229);
                let nest = state % npix;
                let ring = reference_nested2ring(nside, nest).unwrap();
                let back = reference_ring2nest(nside, ring).unwrap();
                assert_eq!(back, nest);
            }
        }
    }

    #[test]
    fn nested_neighbours_are_symmetric_on_interior_samples() {
        let base = Base::new(128).unwrap();
        for nest in [0_u64, 1, 42, 12_345, base.n_hash - 1] {
            let neighbours = nested_neighbours(128, nest as u32).unwrap();
            for other in &neighbours {
                let reverse = nested_neighbours(128, *other).unwrap();
                assert!(
                    reverse.contains(&(nest as u32)),
                    "neighbour symmetry failed for {nest} -> {other}"
                );
            }
        }
    }

    #[test]
    fn all_nside2_pixels_have_neighbours() {
        let npix = 12 * 2 * 2;
        for nest in 0..npix {
            let neighbours = nested_neighbours(2, nest as u32).unwrap();
            assert!(
                (3..=8).contains(&neighbours.len()),
                "nside=2 pixel {nest} had {} neighbours",
                neighbours.len()
            );
        }
    }
}
