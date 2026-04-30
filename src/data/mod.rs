//! Embedded data tables (Leinert, etc.) referenced from the components.
//!
//! Scientific role:
//! several NSB component models depend on published lookup tables rather than
//! closed-form formulas. This module groups those embedded numerical
//! references.
//!
//! Contribution to the science:
//! keeping these tables versioned in-source makes the model reproducible:
//! the computed NSB depends not only on formulas, but also on the exact
//! scientific tabulations shipped with the crate.

pub mod leinert;
