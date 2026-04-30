//! Atmospheric extinction and scattering.
//!
//! Scientific role:
//! the atmosphere is not just a viewing medium; it changes the apparent sky
//! brightness through absorption, extinction, and scattering.
//!
//! Contribution to the science:
//! this module groups atmosphere-specific helpers and future tables used by
//! NSB components, especially those that model how sunlight and moonlight are
//! redistributed before reaching the telescope.

pub mod single_scatter;
