//! Substrate-level core types
//!
//! These types are shared across all activations and the plexus layer.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Handle pointing to external data with versioning
///
/// Display format: `plugin@version::method:meta[0]:meta[1]:...`
///
/// Examples:
/// - `cone@1.0.0::chat:msg-123:user:bob`
/// - `claudecode@1.0.0::chat:msg-456:assistant`
/// - `bash@1.0.0::execute:cmd-789`
/// - `cone@1.0.0::chat` (empty meta)
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Handle {
    /// Plugin identifier (e.g., "cone", "claudecode", "bash")
    pub plugin: String,

    /// Plugin version (semantic version: "MAJOR.MINOR.PATCH")
    pub version: String,

    /// Creation method that produced this handle (e.g., "chat", "execute")
    pub method: String,

    /// Metadata parts - variable length list of strings
    /// For messages: typically [message_uuid, role, optional_extra...]
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub meta: Vec<String>,
}

impl Handle {
    /// Create a new handle
    pub fn new(plugin: impl Into<String>, version: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            plugin: plugin.into(),
            version: version.into(),
            method: method.into(),
            meta: Vec::new(),
        }
    }

    /// Add metadata to the handle
    pub fn with_meta(mut self, meta: Vec<String>) -> Self {
        self.meta = meta;
        self
    }

    /// Add a single metadata item
    pub fn push_meta(mut self, item: impl Into<String>) -> Self {
        self.meta.push(item.into());
        self
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Format: plugin@version::method:meta[0]:meta[1]:...
        write!(f, "{}@{}::{}", self.plugin, self.version, self.method)?;
        for m in &self.meta {
            write!(f, ":{}", m)?;
        }
        Ok(())
    }
}

impl FromStr for Handle {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse: plugin@version::method:meta[0]:meta[1]:...

        // Split on @ to get plugin and rest
        let (plugin, rest) = s.split_once('@')
            .ok_or_else(|| format!("Invalid handle format, missing '@': {}", s))?;

        // Split on :: to get version and method+meta
        let (version, method_and_meta) = rest.split_once("::")
            .ok_or_else(|| format!("Invalid handle format, missing '::': {}", s))?;

        // Split method and meta on :
        let mut parts = method_and_meta.split(':');
        let method = parts.next()
            .ok_or_else(|| format!("Invalid handle format, missing method: {}", s))?;

        let meta: Vec<String> = parts.map(|s| s.to_string()).collect();

        Ok(Handle {
            plugin: plugin.to_string(),
            version: version.to_string(),
            method: method.to_string(),
            meta,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_display() {
        let handle = Handle::new("cone", "1.0.0", "chat")
            .push_meta("msg-123")
            .push_meta("user");
        assert_eq!(handle.to_string(), "cone@1.0.0::chat:msg-123:user");
    }

    #[test]
    fn test_handle_parse() {
        let handle: Handle = "cone@1.0.0::chat:msg-123:user".parse().unwrap();
        assert_eq!(handle.plugin, "cone");
        assert_eq!(handle.version, "1.0.0");
        assert_eq!(handle.method, "chat");
        assert_eq!(handle.meta, vec!["msg-123", "user"]);
    }

    #[test]
    fn test_handle_parse_no_meta() {
        let handle: Handle = "bash@1.0.0::execute".parse().unwrap();
        assert_eq!(handle.plugin, "bash");
        assert_eq!(handle.method, "execute");
        assert!(handle.meta.is_empty());
    }
}
