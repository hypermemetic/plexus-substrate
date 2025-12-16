use super::path::Provenance;
use serde::{Deserialize, Serialize};

/// Inner stream item type (the actual event data)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PlexusStreamEvent {
    /// Progress update
    #[serde(rename = "progress")]
    Progress {
        provenance: Provenance,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        percentage: Option<f32>,
    },

    /// Data chunk with type information
    #[serde(rename = "data")]
    Data {
        provenance: Provenance,
        content_type: String,
        data: serde_json::Value,
    },

    /// Error occurred
    #[serde(rename = "error")]
    Error {
        provenance: Provenance,
        error: String,
        recoverable: bool,
    },

    /// Stream completed successfully
    #[serde(rename = "done")]
    Done { provenance: Provenance },
}

/// Plexus stream item with hash for cache invalidation
///
/// Every response includes the plexus_hash, allowing clients to detect
/// when their cached schema is stale.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlexusStreamItem {
    /// Hash of all activations for cache invalidation
    pub plexus_hash: String,

    /// The actual event data (flattened into same object)
    #[serde(flatten)]
    pub event: PlexusStreamEvent,
}

impl PlexusStreamItem {
    /// Create a new stream item with the given hash
    pub fn new(plexus_hash: String, event: PlexusStreamEvent) -> Self {
        Self { plexus_hash, event }
    }

    /// Create a Progress item
    pub fn progress(plexus_hash: String, provenance: Provenance, message: String, percentage: Option<f32>) -> Self {
        Self::new(plexus_hash, PlexusStreamEvent::Progress { provenance, message, percentage })
    }

    /// Create a Data item
    pub fn data(plexus_hash: String, provenance: Provenance, content_type: String, data: serde_json::Value) -> Self {
        Self::new(plexus_hash, PlexusStreamEvent::Data { provenance, content_type, data })
    }

    /// Create an Error item
    pub fn error(plexus_hash: String, provenance: Provenance, error: String, recoverable: bool) -> Self {
        Self::new(plexus_hash, PlexusStreamEvent::Error { provenance, error, recoverable })
    }

    /// Create a Done item
    pub fn done(plexus_hash: String, provenance: Provenance) -> Self {
        Self::new(plexus_hash, PlexusStreamEvent::Done { provenance })
    }
}

// Legacy constructors for backwards compatibility during migration
// TODO: Remove these once all code is updated to use new constructors
impl PlexusStreamItem {
    /// Legacy: Create Data without explicit hash (uses empty string)
    #[doc(hidden)]
    pub fn data_legacy(provenance: Provenance, content_type: String, data: serde_json::Value) -> Self {
        Self::data(String::new(), provenance, content_type, data)
    }

    /// Legacy: Create Done without explicit hash (uses empty string)
    #[doc(hidden)]
    pub fn done_legacy(provenance: Provenance) -> Self {
        Self::done(String::new(), provenance)
    }

    /// Legacy: Create Error without explicit hash (uses empty string)
    #[doc(hidden)]
    pub fn error_legacy(provenance: Provenance, error: String, recoverable: bool) -> Self {
        Self::error(String::new(), provenance, error, recoverable)
    }
}
