//! Internal scalar output for Zodiacal-light evaluation.

use qtty::radiometry::{
    PhotonsPerSquareCentimeterNanosecondSteradian as BandPhotonRadiance, S10s as S10,
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct ZodiacalOutputs {
    pub(crate) integrated: BandPhotonRadiance,
    pub(crate) b_flux_s10: S10,
    pub(crate) v_flux_s10: S10,
}
