//! DEPRECATED: Stream-based guidance replaces middleware
//!
//! This module is kept for historical reference only. Error guidance is now
//! provided via `PlexusStreamEvent::Guidance` events in error streams.
//!
//! **Migration:** Frontends should handle guidance events instead of parsing
//! JSON-RPC error data. See the frontend migration guide at:
//! `docs/architecture/16680880693241553663_frontend-guidance-migration.md`
//!
//! **Architecture:** See stream-based guidance design at:
//! `docs/architecture/16680881573410764543_guidance-stream-based-errors.md`

#![allow(dead_code)]

use std::sync::Arc;

/// Activation info needed for generating guided errors (DEPRECATED - kept for backwards compat)
#[derive(Clone, Debug)]
pub struct ActivationRegistry {
    /// List of available activation namespaces
    pub activations: Vec<String>,
}

impl ActivationRegistry {
    pub fn new(activations: Vec<String>) -> Self {
        Self { activations }
    }
}

/// Middleware that enriches error responses with guided `try` suggestions (DEPRECATED - no-op)
#[derive(Clone)]
pub struct GuidedErrorMiddleware<S> {
    inner: S,
    _registry: Arc<ActivationRegistry>,
}

impl<S> GuidedErrorMiddleware<S> {
    pub fn new(inner: S, registry: Arc<ActivationRegistry>) -> Self {
        Self { inner, _registry: registry }
    }
}
