/// Identifier types for the Cone plugin
///
/// Provides flexible identification of cones by name or UUID.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifier for a cone - either by name or UUID
///
/// Tagged enum with two variants:
/// - `{"by_name": {"name": "assistant"}}` - lookup by name (supports partial matching)
/// - `{"by_id": {"id": "550e8400-e29b-41d4-a716-446655440000"}}` - lookup by UUID
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConeIdentifier {
    /// Lookup cone by its human-readable name
    ByName {
        /// Cone name (supports partial matching, e.g., "assistant" or "assistant#550e")
        name: String,
    },
    /// Lookup cone by its UUID
    ById {
        /// Cone UUID
        id: Uuid,
    },
}
