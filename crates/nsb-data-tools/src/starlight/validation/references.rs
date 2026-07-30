//! Checksum-pinned independent reference registry for Starlight validation.
//!
//! No reference in this registry ships with an invented checksum. Entries
//! that have not been physically acquired and hashed must carry
//! `status = "pending-acquisition"` and no `sha256`; the loader fails closed
//! if that pairing is violated in either direction.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const REFERENCES_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferencesDocument {
    pub schema_version: u32,
    /// Always true until at least one reference reaches `acquired`.
    pub acquisition_required: bool,
    pub notes: String,
    pub references: Vec<ReferenceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceEntry {
    pub id: String,
    pub citation: String,
    pub description: String,
    /// Free-text sky coverage, e.g. "all-sky", "dark-fields", "regional".
    pub coverage: String,
    pub wavelength_band_nm: [f64; 2],
    pub spectral_quantity: String,
    pub transformation_to_target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquisition_url: Option<String>,
    pub license: String,
    pub status: ReferenceStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    pub filename: String,
    pub acquisition_notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceStatus {
    PendingAcquisition,
    Acquired,
}

impl ReferencesDocument {
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != REFERENCES_SCHEMA_VERSION {
            bail!(
                "unsupported Starlight validation references schema_version {}",
                self.schema_version
            );
        }
        if self.notes.trim().is_empty() {
            bail!("references document requires non-empty notes");
        }
        if self.references.len() < 2 {
            bail!("references document requires at least two candidate references");
        }
        let mut ids = BTreeSet::new();
        let mut any_acquired = false;
        for reference in &self.references {
            reference.validate()?;
            if !ids.insert(reference.id.as_str()) {
                bail!("duplicate reference id {}", reference.id);
            }
            any_acquired |= reference.status == ReferenceStatus::Acquired;
        }
        if self.acquisition_required == any_acquired {
            bail!(
                "references document acquisition_required={} is inconsistent with acquired={}",
                self.acquisition_required,
                any_acquired
            );
        }
        Ok(())
    }

    pub fn acquired(&self) -> impl Iterator<Item = &ReferenceEntry> {
        self.references
            .iter()
            .filter(|reference| reference.status == ReferenceStatus::Acquired)
    }
}

impl ReferenceEntry {
    fn validate(&self) -> Result<()> {
        require_text("reference id", &self.id)?;
        require_text("reference citation", &self.citation)?;
        require_text("reference description", &self.description)?;
        require_text("reference coverage", &self.coverage)?;
        require_text("reference spectral quantity", &self.spectral_quantity)?;
        require_text(
            "reference transformation_to_target",
            &self.transformation_to_target,
        )?;
        require_text("reference license", &self.license)?;
        require_text("reference filename", &self.filename)?;
        require_text("reference acquisition_notes", &self.acquisition_notes)?;
        if !self.wavelength_band_nm[0].is_finite()
            || !self.wavelength_band_nm[1].is_finite()
            || self.wavelength_band_nm[0] >= self.wavelength_band_nm[1]
            || self.wavelength_band_nm[0] < 0.0
        {
            bail!("reference {} has an invalid wavelength band", self.id);
        }
        match (self.status, &self.sha256) {
            (ReferenceStatus::PendingAcquisition, Some(_)) => bail!(
                "reference {} is pending-acquisition but carries a checksum; clear it or mark acquired",
                self.id
            ),
            (ReferenceStatus::Acquired, None) => bail!(
                "reference {} is marked acquired but has no checksum; acquisition must fail closed",
                self.id
            ),
            (ReferenceStatus::Acquired, Some(sha256)) => require_sha256("reference", sha256)?,
            (ReferenceStatus::PendingAcquisition, None) => {}
        }
        Ok(())
    }
}

fn require_text(label: &str, value: &str) -> Result<()> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || ["placeholder", "todo", "tbd", "unknown", "unspecified"]
            .iter()
            .any(|marker| normalized == *marker)
    {
        bail!("{label} is missing or contains a placeholder");
    }
    Ok(())
}

fn require_sha256(label: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{label} SHA-256 must contain 64 lowercase hexadecimal characters");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_entry(id: &str) -> ReferenceEntry {
        ReferenceEntry {
            id: id.to_string(),
            citation: "Author (Year), Journal".to_string(),
            description: "test description".to_string(),
            coverage: "all-sky".to_string(),
            wavelength_band_nm: [300.0, 650.0],
            spectral_quantity: "photon radiance".to_string(),
            transformation_to_target: "identity".to_string(),
            acquisition_url: None,
            license: "unknown, request from publisher".to_string(),
            status: ReferenceStatus::PendingAcquisition,
            sha256: None,
            filename: format!("{id}.dat"),
            acquisition_notes: "requires manual literature request".to_string(),
        }
    }

    fn document(entries: Vec<ReferenceEntry>) -> ReferencesDocument {
        ReferencesDocument {
            schema_version: REFERENCES_SCHEMA_VERSION,
            acquisition_required: true,
            notes: "test notes".to_string(),
            references: entries,
        }
    }

    #[test]
    fn requires_at_least_two_references() {
        assert!(document(vec![valid_entry("a")]).validate().is_err());
        document(vec![valid_entry("a"), valid_entry("b")])
            .validate()
            .unwrap();
    }

    #[test]
    fn pending_acquisition_with_checksum_is_rejected() {
        let mut entry = valid_entry("a");
        entry.sha256 = Some("a".repeat(64));
        assert!(document(vec![entry, valid_entry("b")]).validate().is_err());
    }

    #[test]
    fn acquired_without_checksum_is_rejected() {
        let mut entry = valid_entry("a");
        entry.status = ReferenceStatus::Acquired;
        assert!(document(vec![entry, valid_entry("b")]).validate().is_err());
    }

    #[test]
    fn acquired_with_valid_checksum_passes_and_is_iterable() {
        let mut entry = valid_entry("a");
        entry.status = ReferenceStatus::Acquired;
        entry.sha256 = Some("a".repeat(64));
        let document = document(vec![entry, valid_entry("b")]);
        assert!(document.validate().is_err()); // acquisition_required stays true incorrectly
        let mut document = document;
        document.acquisition_required = false;
        document.validate().unwrap();
        assert_eq!(document.acquired().count(), 1);
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        assert!(document(vec![valid_entry("a"), valid_entry("a")])
            .validate()
            .is_err());
    }

    #[test]
    fn placeholder_text_is_rejected() {
        let mut entry = valid_entry("a");
        entry.citation = "TODO".to_string();
        assert!(document(vec![entry, valid_entry("b")]).validate().is_err());
    }
}
