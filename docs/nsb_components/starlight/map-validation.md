# Starlight dataset validation

`nsb-data dataset starlight validate --config <run.toml>` is the only supported
validation entry point. It verifies the built artifact set, immutable
checksums, required HEALPix headers and the configured scientific gates. The
versioned JSON report is stored in the configured workspace and is required by
`publish`.

Validation fails closed when an artifact is absent, changed, malformed,
incomplete or belongs to another dataset. Publication recomputes checksums so a
file changed after validation cannot enter `crates/nsb/data`.

Scientific production admission remains stricter than structural validity. A
production map also requires catalogue provenance, calibrated non-proxy
photometry, population accounting, coverage, finite non-negative radiance,
longitude wrapping, plane/pole behaviour, flux conservation where applicable,
independent comparison evidence and redistribution approval. These gates are
defined in [science requirements](science-requirements.md) and the [external
manifest](external-manifest.md).
