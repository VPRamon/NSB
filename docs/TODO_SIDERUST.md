# Units that should migrate upstream into `qtty`

The following types live in `nsb::units` because they are not yet available in
`qtty`. They should be promoted to `qtty` once their API stabilises, so other
crates in the ecosystem can share them.

| Type                       | Dimensions / meaning                                  | Status |
|----------------------------|--------------------------------------------------------|--------|
| `S10`                      | Brightness in 10th-magnitude stars per square degree   | TODO: implement in siderust |
| `SpectralPhotonRadiance`   | `ph / (s · cm² · sr · Å)`                              | TODO: implement in siderust |
| `BandPhotonRadiance`       | `ph / (cm² · ns · sr)` — band-integrated NSB output     | TODO: implement in siderust |
| `SurfaceBrightness`        | `mag / arcsec²` (with photometric zero-point conversion) | TODO: implement in siderust |
| `erg_to_photon(λ)`         | `erg/(s·cm²·sr·Å)` → `ph/(s·cm²·sr·Å)` using `5.03e7·λ_Å` | TODO: implement in siderust as conversion trait |

When these are added upstream, replace the local newtypes with re-exports and
remove this file.
