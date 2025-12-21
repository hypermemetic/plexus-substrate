//! MCP Interface
//!
//! The main MCP interface that wraps Plexus and routes MCP protocol methods.

use std::sync::Arc;

use serde_json::Value;

use super::{
    error::McpError,
    state::McpStateMachine,
    types::ServerInfo,
};
use crate::plexus::Plexus;

/// The MCP Interface - routes MCP protocol methods to handlers
pub struct McpInterface {
    /// Reference to the Plexus for accessing activations
    plexus: Arc<Plexus>,
    /// Protocol state machine
    state: McpStateMachine,
    /// Server information
    server_info: ServerInfo,
}

impl McpInterface {
    /// Create a new MCP interface wrapping a Plexus instance
    pub fn new(plexus: Arc<Plexus>) -> Self {
        Self {
            plexus,
            state: McpStateMachine::new(),
            server_info: ServerInfo {
                name: "substrate".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            },
        }
    }

    /// Get the Plexus instance
    pub fn plexus(&self) -> &Arc<Plexus> {
        &self.plexus
    }

    /// Get the state machine
    pub fn state(&self) -> &McpStateMachine {
        &self.state
    }

    /// Get server info
    pub fn server_info(&self) -> &ServerInfo {
        &self.server_info
    }

    /// Route an MCP request to the appropriate handler
    ///
    /// This is the main entry point for MCP protocol methods.
    /// Methods are routed based on the method name.
    pub async fn handle(&self, method: &str, params: Value) -> Result<Value, McpError> {
        tracing::debug!(method = %method, "Handling MCP request");

        match method {
            // Lifecycle
            "initialize" => self.handle_initialize(params).await,
            "notifications/initialized" => self.handle_initialized(params).await,

            // Utility
            "ping" => self.handle_ping(params).await,

            // Tools
            "tools/list" => self.handle_tools_list(params).await,
            "tools/call" => self.handle_tools_call(params).await,

            // Resources
            "resources/list" => self.handle_resources_list(params).await,
            "resources/read" => self.handle_resources_read(params).await,

            // Prompts
            "prompts/list" => self.handle_prompts_list(params).await,
            "prompts/get" => self.handle_prompts_get(params).await,

            // Notifications
            "notifications/cancelled" => self.handle_cancelled(params).await,

            // Unknown method
            _ => Err(McpError::MethodNotFound(method.to_string())),
        }
    }

    // === Lifecycle Handlers (stubs - implemented in MCP-4, MCP-6) ===

    async fn handle_initialize(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("initialize".to_string()))
    }

    async fn handle_initialized(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("notifications/initialized".to_string()))
    }

    // === Utility Handlers (stubs - implemented in MCP-7) ===

    async fn handle_ping(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("ping".to_string()))
    }

    // === Tool Handlers (stubs - implemented in MCP-5, MCP-9) ===

    async fn handle_tools_list(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("tools/list".to_string()))
    }

    async fn handle_tools_call(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("tools/call".to_string()))
    }

    // === Resource Handlers (stubs - implemented in MCP-11) ===

    async fn handle_resources_list(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("resources/list".to_string()))
    }

    async fn handle_resources_read(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("resources/read".to_string()))
    }

    // === Prompt Handlers (stubs - implemented in MCP-12) ===

    async fn handle_prompts_list(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("prompts/list".to_string()))
    }

    async fn handle_prompts_get(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("prompts/get".to_string()))
    }

    // === Notification Handlers (stubs - implemented in MCP-10) ===

    async fn handle_cancelled(&self, _params: Value) -> Result<Value, McpError> {
        Err(McpError::NotImplemented("notifications/cancelled".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plexus::Plexus;

    #[tokio::test]
    async fn test_new_interface() {
        let plexus = Arc::new(Plexus::new());
        let mcp = McpInterface::new(plexus);

        assert_eq!(mcp.server_info().name, "substrate");
        assert!(!mcp.server_info().version.is_empty());
    }

    #[tokio::test]
    async fn test_unknown_method() {
        let plexus = Arc::new(Plexus::new());
        let mcp = McpInterface::new(plexus);

        let result = mcp.handle("unknown/method", Value::Null).await;
        assert!(matches!(result, Err(McpError::MethodNotFound(_))));
    }

    #[tokio::test]
    async fn test_stubs_return_not_implemented() {
        let plexus = Arc::new(Plexus::new());
        let mcp = McpInterface::new(plexus);

        // All methods should return NotImplemented until implemented
        let methods = [
            "initialize",
            "notifications/initialized",
            "ping",
            "tools/list",
            "tools/call",
            "resources/list",
            "resources/read",
            "prompts/list",
            "prompts/get",
            "notifications/cancelled",
        ];

        for method in methods {
            let result = mcp.handle(method, Value::Null).await;
            assert!(
                matches!(result, Err(McpError::NotImplemented(_))),
                "Method {} should return NotImplemented",
                method
            );
        }
    }
}
