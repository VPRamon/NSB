//! Single-scattering correction tables (`sscatcor_m15s1.dat`).
//!
//! TODO: parse the tabulated single-scattering correction grid and provide a
//! bilinear interpolator. Used by the moonlight component.
//!
//! Scientific role:
//! this file is reserved for the atmospheric single-scattering correction data
//! needed by a more detailed moonlight model.
//!
//! Contribution to the science:
//! at the moment it documents a missing piece of the physical scattering
//! pipeline. Its presence makes the intended scientific extension explicit:
//! moving from a compact analytic moonlight approximation toward a fuller
//! atmosphere-driven scattering treatment.
