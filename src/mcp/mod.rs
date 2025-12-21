//! MCP (Model Context Protocol) Interface
//!
//! This module implements the MCP 2025-03-26 Streamable HTTP specification,
//! exposing Plexus activations as MCP tools over SSE.

pub mod state;

pub use state::*;
