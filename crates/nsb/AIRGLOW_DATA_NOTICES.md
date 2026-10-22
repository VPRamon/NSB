# Additional PALACE oxygen data attribution

`data/airglow_palace_oxygen_v1.json` is a modified, selected representation of
`palace_lines.fits` and `palace_var.fits` from PALACE v1.0 (October 2024),
by Stefan Noll, Carsten Schmidt, Patrick Hannawald, Wolfgang Kausch and
Stefan Kimeswenger. Source: [Zenodo 14064023](https://doi.org/10.5281/zenodo.14064023).
The original README explicitly licenses these model data under
[Creative Commons Attribution 4.0](https://creativecommons.org/licenses/by/4.0/).

NSB selects four optical Og/Or oxygen lines and two 144-cell variability grids,
converts micrometres to vacuum nanometres, and serializes the source numbers as
JSON. The data retain CC BY 4.0. Changes and exact input checksums are documented
in `docs/nsb_components/airglow/validation/oxygen-and-calibration-v1.md`.
No PALACE program code is copied, imported, executed or redistributed.

Scientific description: Noll et al. (2025), *PALACE v1.0: Paranal Airglow Line
And Continuum Emission model*, Geosci. Model Dev. 18, 4353–4398,
[doi:10.5194/gmd-18-4353-2025](https://doi.org/10.5194/gmd-18-4353-2025).
