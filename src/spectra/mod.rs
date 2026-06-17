//! Spectral data and loaders.
//!
//! NSB's local `Spectrum` wrapper has been collapsed into
//! [`optica::spectrum::SampledSpectrum`]; this module hosts only the
//! NSB-specific loaders (solar, airglow, ozone) that wrap the
//! upstream typed spectrum.
//!
//! Scientific role:
//! several NSB components are driven by reference spectra rather than by a
//! single scalar brightness. This module groups the loaders for those bundled
//! spectral inputs.
//!
//! Contribution to the science:
//! these loaders preserve the provenance and units of the reference tables used
//! by the component models, so the final NSB predictions remain traceable to
//! their scientific input data.

pub mod airglow_cont;
pub mod ozone;
pub mod solar;

pub use optica::spectrum::SampledSpectrum;
