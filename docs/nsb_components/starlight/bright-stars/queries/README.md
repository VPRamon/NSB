# Gaia DR3 bright-star query recipes

These files are the exact ADQL submitted to the Gaia Archive TAP service for
the experimental bright-star build. Submit each statement as an asynchronous
job against `https://gea.esac.esa.int/tap-server/tap`, export CSV without
rewriting numeric fields, and verify the result SHA-256 recorded in
`starlight-bright-stars-v1.ladon.toml`.

`gaia-dr3-hip-le-4-quality.adql` joins the Gaia DR3 Hipparcos-2 best-neighbour
table to `gaiadr3.gaia_source` and selects the identity, J2016 coordinates,
G magnitude, and XP availability for the frozen Hipparcos population.

`gaia-dr3-hip-le-4-positional-fallback.adql` contains the frozen one-arcsec
cone searches around positions propagated to J2016. It is split into statements
because of TAP query-size limits; concatenate the CSV data rows under one header
before checksum verification.

## EDR3 to DR3 identity contract

The upstream bulk filename is in the EDR3 download area. This is acceptable
only because Gaia DR3 uses the same source list as Gaia EDR3. The authoritative
Gaia DR3 documentation states that DR3 contains the same sources as EDR3 and
that EDR3 cross-match tables remain applicable to DR3. The builder additionally
requires every selected best-neighbour `source_id` to occur in the checksum-
pinned `gaiadr3.gaia_source` quality extract before it may influence a decision.
An identity absent from that exact DR3 extract fails closed.

References:

- [Gaia DR3 documentation, catalogue-consolidation motivation](https://gea.esac.esa.int/archive/documentation/GDR3/Catalogue_consolidation/chap_cu9dr2xm/sec_cu9dr2xm_motivation/)
- [Gaia DR3 data model and cross-match tables](https://gea.esac.esa.int/archive/documentation/GDR3/)
- [Gaia Archive extraction guide](https://www.cosmos.esa.int/web/gaia-users/archive/extract-data)
