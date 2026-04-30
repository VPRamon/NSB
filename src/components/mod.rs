//! Physical and empirical NSB component models.
//!
//! Each submodule in this directory corresponds to one contributor to the
//! night-sky background:
//!
//! * `zodiacal` — sunlight scattered by interplanetary dust
//! * `starlight` — unresolved integrated starlight
//! * `airglow` — emission from Earth's upper atmosphere
//! * `moonlight` — scattered moonlight in the atmosphere
//!
//! Scientific role:
//! the total NSB prediction is a sum of components with different physical
//! origins and different dependencies on sky direction, time, and observing
//! conditions. Keeping them separate lets the evaluator expose both totals and
//! scientifically interpretable per-component contributions.

pub mod airglow;
pub mod moonlight;
pub mod starlight;
pub mod zodiacal;
