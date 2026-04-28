//! Spectral data and loaders.
//!
//! NSB's local `Spectrum` wrapper has been collapsed into
//! [`siderust::spectra::SampledSpectrum`]; this module hosts only the
//! NSB-specific loaders (solar, starlight, airglow, ozone) that wrap the
//! upstream typed spectrum.

pub mod solar;
pub mod starlight;
pub mod airglow_cont;
pub mod ozone;

pub use siderust::spectra::SampledSpectrum;
