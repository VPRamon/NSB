//! Versioned F10.7 store (checksum-pinned scientific asset).

use super::record::{F107Kind, F107Record, F107ValidationError};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

/// Supported F10.7 store schema version.
pub const F107_STORE_SCHEMA_VERSION: u32 = 1;

/// Versioned, provenance-carrying F10.7 dataset snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct F107Store {
    /// Schema version, currently [`F107_STORE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Stable dataset identity (for example `nsb-f107-bundled-offline`).
    pub dataset_id: String,
    /// Snapshot identity that pins a particular materialization.
    pub snapshot_id: String,
    /// Scientific convention identifier for the F10.7 quantity.
    pub convention: String,
    /// Human-readable notes about the quantity and known ambiguities.
    #[serde(default)]
    pub convention_notes: String,
    /// Documented climatological fallback in sfu.
    pub climatology_sfu: f64,
    /// Notes describing the climatological fallback.
    #[serde(default)]
    pub climatology_notes: String,
    /// When this snapshot was assembled / retrieved.
    #[serde(default)]
    pub retrieved_at_utc: Option<String>,
    /// Provenance-carrying records.
    pub records: Vec<F107Record>,
    /// SHA-256 of the serialized bytes this store was loaded from (runtime-filled).
    #[serde(skip)]
    pub checksum_sha256: Option<String>,
}

/// Store parse / validation error.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{0}")]
pub struct F107StoreError(pub String);

impl From<F107ValidationError> for F107StoreError {
    fn from(value: F107ValidationError) -> Self {
        Self(value.0)
    }
}

impl F107Store {
    /// Parse and validate a JSON store, attaching the content checksum.
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, F107StoreError> {
        let mut store: Self = serde_json::from_slice(bytes)
            .map_err(|error| F107StoreError(format!("invalid F10.7 store JSON: {error}")))?;
        store.checksum_sha256 = Some(hex_sha256(bytes));
        store.validate()?;
        Ok(store)
    }

    /// Parse and validate a JSON store from a UTF-8 string.
    pub fn from_json_str(input: &str) -> Result<Self, F107StoreError> {
        Self::from_json_bytes(input.as_bytes())
    }

    /// Serialize to canonical pretty JSON bytes (stable key order via serde field order).
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, F107StoreError> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| F107StoreError(format!("serialize F10.7 store: {error}")))?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Validate schema, climatology, and every record; reject conflicting duplicates.
    pub fn validate(&self) -> Result<(), F107StoreError> {
        if self.schema_version != F107_STORE_SCHEMA_VERSION {
            return Err(F107StoreError(format!(
                "unsupported F10.7 store schema {}",
                self.schema_version
            )));
        }
        if self.dataset_id.trim().is_empty() {
            return Err(F107StoreError("dataset_id must be non-empty".into()));
        }
        if self.snapshot_id.trim().is_empty() {
            return Err(F107StoreError("snapshot_id must be non-empty".into()));
        }
        if self.convention.trim().is_empty() {
            return Err(F107StoreError("convention must be non-empty".into()));
        }
        if !self.climatology_sfu.is_finite() || self.climatology_sfu <= 0.0 {
            return Err(F107StoreError(
                "climatology_sfu must be finite and positive".into(),
            ));
        }
        for record in &self.records {
            record.validate()?;
        }
        self.validate_no_conflicting_duplicates()?;
        Ok(())
    }

    fn validate_no_conflicting_duplicates(&self) -> Result<(), F107StoreError> {
        // Key identity for conflict detection.
        let mut by_key: BTreeMap<RecordKey, &F107Record> = BTreeMap::new();
        for record in &self.records {
            let key = RecordKey::from(record);
            if let Some(prior) = by_key.insert(key.clone(), record) {
                if prior.value_sfu.to_bits() != record.value_sfu.to_bits() {
                    return Err(F107StoreError(format!(
                        "conflicting F10.7 records for {} kind {} product {} with identical identity keys",
                        record.date,
                        record.kind.as_str(),
                        record.product
                    )));
                }
            }
        }
        Ok(())
    }

    /// Observed records whose validity covers `requested`.
    ///
    /// Callers that drive the Noll/SkyCalc Airglow correction should further
    /// filter to monthly cadence; raw daily observations are not the fitted quantity.
    pub fn observed_covering(
        &self,
        requested: NaiveDate,
    ) -> Result<Vec<&F107Record>, F107StoreError> {
        let mut matches = Vec::new();
        for record in &self.records {
            if record.kind != F107Kind::Observed {
                continue;
            }
            if record.covers(requested)? {
                matches.push(record);
            }
        }
        Ok(matches)
    }

    /// Forecast records whose validity covers `requested`.
    pub fn forecasts_covering(
        &self,
        requested: NaiveDate,
    ) -> Result<Vec<&F107Record>, F107StoreError> {
        let mut matches = Vec::new();
        for record in &self.records {
            if record.kind != F107Kind::Forecast {
                continue;
            }
            if record.covers(requested)? {
                matches.push(record);
            }
        }
        Ok(matches)
    }

    /// Merge `incoming` into a new store with a new snapshot id, keeping history.
    ///
    /// Existing records are retained. Incoming records replace prior records that
    /// share the same `(kind, date, product, valid_from, valid_through)` key.
    pub fn merge_with(
        &self,
        incoming: &[F107Record],
        snapshot_id: impl Into<String>,
        retrieved_at_utc: Option<String>,
    ) -> Result<Self, F107StoreError> {
        let mut index: BTreeMap<RecordKey, F107Record> = BTreeMap::new();
        for record in &self.records {
            record.validate()?;
            index.insert(RecordKey::from(record), record.clone());
        }
        for record in incoming {
            record.validate()?;
            index.insert(RecordKey::from(record), record.clone());
        }
        let mut records: Vec<F107Record> = index.into_values().collect();
        records.sort_by(|a, b| {
            (&a.date, a.kind.as_str(), &a.product).cmp(&(&b.date, b.kind.as_str(), &b.product))
        });
        let merged = Self {
            schema_version: F107_STORE_SCHEMA_VERSION,
            dataset_id: self.dataset_id.clone(),
            snapshot_id: snapshot_id.into(),
            convention: self.convention.clone(),
            convention_notes: self.convention_notes.clone(),
            climatology_sfu: self.climatology_sfu,
            climatology_notes: self.climatology_notes.clone(),
            retrieved_at_utc,
            records,
            checksum_sha256: None,
        };
        merged.validate()?;
        Ok(merged)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RecordKey {
    kind: String,
    date: String,
    product: String,
    valid_from: String,
    valid_through: String,
}

impl RecordKey {
    fn from(record: &F107Record) -> Self {
        // Identity deliberately excludes forecast_issued_at_utc and retrieved_at_utc
        // so re-downloads of the same product window replace rather than accumulate,
        // and so retrieval time cannot be mistaken for issuance in the key space.
        Self {
            kind: record.kind.as_str().to_string(),
            date: record.date.clone(),
            product: record.product.clone(),
            valid_from: record
                .valid_from
                .clone()
                .unwrap_or_else(|| record.date.clone()),
            valid_through: record
                .valid_through
                .clone()
                .unwrap_or_else(|| record.date.clone()),
        }
    }
}

/// Lowercase hex SHA-256.
pub fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
