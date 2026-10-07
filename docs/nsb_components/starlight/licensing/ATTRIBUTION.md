# Starlight source attribution

NSB's Starlight tooling can derive maps from several external catalogues and
reference datasets. The exact sources used by a promoted product must be pinned
in that product's provenance and redistribution review.

## Gaia

GaiaSource and XP continuous products come from ESA's Gaia Archive. Their
release, licence, source URLs, checksum manifests, and applicable attribution
must be recorded for every promoted derived product.

## Hipparcos and Tycho

A bright-star supplement may use Hipparcos-2 and Tycho-2 source material.
Those upstream catalogue bytes remain external to NSB. A promoted derived
product must record the exact releases, source URLs, checksums, and applicable
terms used by its build.

## XHIP and CK04

Spectral typing may use XHIP and Castelli–Kurucz/CK04 reference spectra.
A build must pin the exact source release and checksum. Version identifiers
belonging to those external products are retained as upstream provenance and
are independent of NSB's own schema numbering.

## CALSPEC and photometric response data

CALSPEC spectra and external photometric response curves may be used for
calibration or validation. Their exact identities and terms must be retained in
the candidate evidence; their raw bytes are not bundled by NSB 0.1.0.

## Derived NSB products

NSB-generated candidate maps, merge reports, validation reports, runtime maps,
and sidecars are not automatically redistributable merely because NSB authored
the transformation code. Promotion must account for the licences and
restrictions of every upstream source represented in the derived bytes.
