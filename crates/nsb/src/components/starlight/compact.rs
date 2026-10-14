// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Vallés Puig, Ramon
//! Lossless, endian-independent storage of the packed HEALPix CSV contract.
use super::map::parse_header_metadata;
use super::{StarlightMap, StarlightPixel, StarlightProvenance};
use crate::error::{NsbError, Result};
use crate::units::radiometry::{PhotonsPerSquareCentimeterNanosecondSteradian as Radiance, S10s};
use siderust::checksum::sha256;
use siderust::healpix::{HealpixGrid, HealpixOrdering, Nside};

pub(super) const MAGIC: &[u8; 8] = b"NSBSTL01";

fn invalid(message: impl Into<String>) -> NsbError {
    NsbError::InvalidMap {
        message: message.into(),
    }
}

/// Verify framing and integrity before exposing the UTF-8 provenance header.
pub(super) fn parts(bytes: &[u8]) -> Result<(&str, &[u8])> {
    if bytes.len() < 44 || &bytes[..8] != MAGIC {
        return Err(invalid(
            "invalid compact Starlight magic/version or truncated file",
        ));
    }
    let end = bytes.len() - 32;
    if sha256(&bytes[..end]).as_slice() != &bytes[end..] {
        return Err(invalid("compact Starlight SHA-256 mismatch"));
    }
    let length = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    let header_end = 12usize
        .checked_add(length)
        .filter(|&n| n <= end)
        .ok_or_else(|| invalid("compact Starlight header length exceeds file"))?;
    let header = std::str::from_utf8(&bytes[12..header_end])
        .map_err(|_| invalid("compact Starlight header is not UTF-8"))?;
    Ok((header, &bytes[header_end..end]))
}

impl StarlightMap {
    /// Encode a supported packed CSV as version-1 compact bytes, preserving all
    /// four scientific f64 fields and the complete provenance header. This is a
    /// representation conversion, not production admission or redistribution approval.
    pub fn compact_csv(raw: &str, provenance: StarlightProvenance) -> Result<Vec<u8>> {
        let map = Self::from_csv_str(raw, provenance)?;
        let metadata = parse_header_metadata(raw);
        let header: String = metadata
            .iter()
            .map(|(k, v)| format!("# {k}={v}\n"))
            .collect();
        let length =
            u32::try_from(header.len()).map_err(|_| invalid("compact header too large"))?;
        let mut bytes = Vec::with_capacity(44 + header.len() + map.pixels().len() * 32);
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&length.to_le_bytes());
        bytes.extend_from_slice(header.as_bytes());
        for pixel in map.pixels() {
            for value in [
                pixel.integrated,
                pixel.statistical_uncertainty.unwrap(),
                pixel.systematic_uncertainty.unwrap(),
                pixel.total_uncertainty.unwrap(),
            ] {
                bytes.extend_from_slice(&value.value().to_le_bytes());
            }
        }
        let digest = sha256(&bytes);
        bytes.extend_from_slice(&digest);
        Ok(bytes)
    }

    /// Decode a version-1 compact packed map after verifying its internal SHA-256.
    /// Production callers should use `ValidatedStarlightMap` for sidecar and
    /// scientific admission checks in addition to this structural validation.
    pub fn from_compact_bytes(bytes: &[u8], provenance: StarlightProvenance) -> Result<Self> {
        let (header, payload) = parts(bytes)?;
        let metadata = parse_header_metadata(header);
        let get = |key: &str| {
            metadata
                .get(key)
                .map(String::as_str)
                .ok_or_else(|| invalid(format!("compact Starlight missing {key}")))
        };
        if !get("coordinate_frame")?.eq_ignore_ascii_case("galactic")
            || get("s10_diagnostics")? != "not_provided"
        {
            return Err(invalid(
                "compact Starlight requires Galactic frame and absent S10 diagnostics",
            ));
        }
        let nside = get("nside")?
            .parse::<u32>()
            .map_err(|_| invalid("invalid compact nside"))?;
        let ordering = match get("ordering")?.to_ascii_lowercase().as_str() {
            "ring" => HealpixOrdering::Ring,
            "nested" => HealpixOrdering::Nested,
            _ => return Err(invalid("invalid compact HEALPix ordering")),
        };
        let grid = HealpixGrid::new(
            Nside::new(nside).map_err(|e| invalid(e.to_string()))?,
            ordering,
        )
        .map_err(|e| invalid(e.to_string()))?;
        let count = usize::try_from(grid.npix())
            .map_err(|_| invalid("compact map exceeds address space"))?;
        if count.checked_mul(32) != Some(payload.len()) {
            return Err(invalid("compact Starlight pixel count/length mismatch"));
        }
        let pixels = payload
            .as_chunks::<32>()
            .0
            .iter()
            .map(|row| {
                let mut fields = row
                    .as_chunks::<8>()
                    .0
                    .iter()
                    .map(|v| Radiance::new(f64::from_le_bytes(*v)));
                StarlightPixel::new(fields.next().unwrap(), S10s::new(0.0), S10s::new(0.0))
                    .without_s10_diagnostics()
                    .with_uncertainties(
                        fields.next().unwrap(),
                        fields.next().unwrap(),
                        fields.next().unwrap(),
                    )
            })
            .collect();
        let provenance = StarlightProvenance::from_header_metadata(&metadata, provenance);
        Self::from_healpix(grid, pixels, provenance)
    }
}
