//! Zodiacal-light scientific-model and atmospheric-propagation selection.
//!
//! Public callers select the celestial source model with [`ZodiacalModel`] and
//! the independent atmospheric treatment with [`ZodiacalExtinction`] through
//! [`crate::NsbModelConfig`]. Evaluation is performed by [`crate::NsbEvaluator`]
//! and returned through the shared [`crate::NsbComponent`] contract.
//!
//! The first-release source model is [`ZodiacalModel::Leinert1998`]: the
//! Leinert et al. (1998) empirical S10 brightness table, the bundled solar
//! reference spectrum, and the Leinert wavelength-reddening prescription.
//! [`ZodiacalExtinction::Noll2012Approx`] is the default ground-observer
//! propagation approximation; [`ZodiacalExtinction::None`] disables that
//! attenuation without changing the selected scientific source model.
//!
//! Concrete evaluators, component-only outputs, custom brightness grids, and
//! solar-spectrum replacement are implementation/validation details rather
//! than a second application evaluation API.
//!
//! # References
//!
//! - Leinert et al. (1998), *A&AS* 127, 1–99.
//! - Noll et al. (2012), *A&A* 543, A92.

pub(crate) mod extinction;
pub(crate) mod geometry;
pub(crate) mod leinert;
pub(crate) mod model;
pub(crate) mod output;
pub(crate) mod reddening;
pub(crate) mod spectrum;

pub use extinction::ZodiacalExtinction;
pub(crate) use model::ZodiacalLight;
pub use model::ZodiacalModel;

#[cfg(test)]
mod tests;
