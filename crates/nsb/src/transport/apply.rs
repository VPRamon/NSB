//! Apply transport transmission to typed radiance quantities.

use qtty::radiometry::WattsPerSquareMeterSteradianNanometer;
use qtty::{Quantity, Unit};
use siderust::qtty::Nanometers;

use super::geometry::DirectPathGeometry;
use super::model::TransportModel;
use super::origin::RadianceOrigin;
use crate::error::Result;
use crate::site::AtmosphericConditions;

/// Propagate energy spectral radiance through the selected transport model.
pub fn apply_spectral_radiance(
    transport: &TransportModel,
    origin: RadianceOrigin,
    incident: WattsPerSquareMeterSteradianNanometer,
    wavelength: Nanometers,
    geometry: DirectPathGeometry,
    atmosphere: AtmosphericConditions,
) -> Result<WattsPerSquareMeterSteradianNanometer> {
    transport.apply_spectral_for_origin(origin, incident, wavelength, geometry, atmosphere)
}

/// Scale a monochromatic radiance by direct-path transmission.
///
/// This is the typed helper for band-integrated celestial products (for
/// example admitted starlight) when the caller supplies an explicit
/// representative wavelength. It does not mutate or reinterpret any
/// underlying sky-map asset.
pub fn apply_monochromatic<U: Unit>(
    transport: &TransportModel,
    origin: RadianceOrigin,
    incident: Quantity<U>,
    wavelength: Nanometers,
    geometry: DirectPathGeometry,
    atmosphere: AtmosphericConditions,
) -> Result<Quantity<U>> {
    transport.ensure_origin_compatible(origin)?;
    let transmission = transport.transmission(wavelength, geometry, atmosphere);
    Ok(Quantity::<U>::new(incident.value() * transmission.value()))
}
