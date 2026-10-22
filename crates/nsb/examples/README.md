# NSB examples

These examples are executable documentation and are compiled by normal CI.

- `point_query.rs` — minimal end-to-end point evaluation using the recommended public API.
- `threshold_window.rs` — canonical threshold/window planning workflow.
- `model_metadata.rs` — explicit model/component selection plus inspection of scientific maturity, provenance, and validated-domain metadata.

`cargo run -p nsb --release --example starlight_transport --locked` transports
three checksum-pinned synthetic HEALPix bins. See
[the spectral contract](../../../docs/specifications/starlight-transport.md)
for the optional frozen-map diagnostic and its limitations.

The optional `starlight_multiple_scattering --projected` command evaluates the
frozen Gaia map with the factorized solver and component/convergence JSON output.
Use the explicit settings and checksum-pinned command in
[`docs/validation/gaia-xp-factorized.md`](../../../docs/validation/gaia-xp-factorized.md).
No complete experimental map is needed by normal tests.

- `airglow_calibration`: runnable synthetic continuum fit, export and strict replay;
  see `docs/user-guide/airglow-calibration.md` for independent-input requirements.
