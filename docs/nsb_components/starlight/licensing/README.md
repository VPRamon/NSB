# Starlight redistribution and licensing

Status: Current redistribution policy for Starlight data products.

NSB 0.1.0 does not redistribute a Gaia-derived production Starlight map. Raw
Gaia, Hipparcos, Tycho, CALSPEC, CK04, and other upstream catalogue/reference
bytes are not bundled by the runtime package.

The attribution requirements and source families relevant to a future
Starlight product are documented in [ATTRIBUTION.md](ATTRIBUTION.md) and the
project-wide [THIRD_PARTY_NOTICES.md](../../../../THIRD_PARTY_NOTICES.md).

## Promotion contract

`nsb-data` implements a fail-closed redistribution review contract. A future
candidate promotion must provide:

- an inventory of every distributed and referenced artifact;
- exact SHA-256 identities for distributed derived outputs;
- source/release/licence information for upstream inputs;
- the intended distribution channels;
- an authorized human redistribution decision; and
- any machine-verifiable conditions attached to that decision.

The Rust implementation validates those records but cannot grant
redistribution approval itself. Approval belongs to an authorized human
reviewer.

## First-release rule

Until a Gaia-derived product has completed that process, it remains external to
the NSB release. Generating or validating a candidate does not make it a bundled
runtime asset.
