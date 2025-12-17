//! Session-Typed Method Definitions for Bash using #[hub_method] macro
//!
//! This module demonstrates using the hub_method macro to define methods
//! where the function signature IS the schema source of truth.

use dialectic::types::{Choose, Continue, Done, Loop, Send};
use hub_macro::hub_method;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Input/Output Types
// ============================================================================

/// Input for the execute method
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExecuteInput {
    /// The bash command to execute
    pub command: String,
}

/// Standard output line from bash execution
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StdoutEvent {
    /// The output line
    pub line: String,
}

/// Standard error line from bash execution
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StderrEvent {
    /// The error line
    pub line: String,
}

/// Exit event when process completes
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ExitEvent {
    /// The exit code
    pub code: i32,
}

/// Stream event - either output or completion
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BashStreamEvent {
    /// Standard output
    Stdout(StdoutEvent),
    /// Standard error
    Stderr(StderrEvent),
    /// Process exited
    Exit(ExitEvent),
}

// ============================================================================
// Method Definition using #[hub_method]
// ============================================================================

/// Execute a bash command and stream stdout, stderr, and exit code
///
/// The server streams events until an Exit event is sent.
/// Protocol (server perspective):
/// - Recv command
/// - Loop: Choose to send Stdout/Stderr (continue) or Exit (break)
#[hub_method]
pub async fn execute(
    input: ExecuteInput,
) -> Loop<Choose<(Send<BashStreamEvent, Continue<0>>, Send<ExitEvent, Done>)>> {
    // This is just for schema extraction - actual implementation is elsewhere
    todo!()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plexus::ProtocolSchema;

    #[test]
    fn test_execute_schema() {
        let schema = execute_schema();

        assert_eq!(schema.name, "execute");
        assert!(schema
            .description
            .contains("Execute a bash command and stream"));

        // Server protocol: Recv input, Loop { Choose { ... } }
        match &schema.server_protocol {
            ProtocolSchema::Recv { then, .. } => match &**then {
                ProtocolSchema::Loop { body } => match &**body {
                    ProtocolSchema::Choose { branches } => {
                        assert_eq!(branches.len(), 2, "Should have 2 branches");
                    }
                    _ => panic!("Expected Choose in loop body"),
                },
                _ => panic!("Expected Loop after Recv"),
            },
            _ => panic!("Expected Recv first in server protocol"),
        }

        // Client protocol: Send input, Loop { Offer { ... } }
        match &schema.protocol {
            ProtocolSchema::Send { then, .. } => match &**then {
                ProtocolSchema::Loop { body } => match &**body {
                    ProtocolSchema::Offer { branches } => {
                        assert_eq!(branches.len(), 2, "Should have 2 branches");
                    }
                    _ => panic!("Expected Offer in loop body (dual of Choose)"),
                },
                _ => panic!("Expected Loop after Send"),
            },
            _ => panic!("Expected Send first in client protocol"),
        }
    }

    #[test]
    fn test_schema_has_input_type() {
        let schema = execute_schema();

        // Verify client protocol has input schema with command field
        if let ProtocolSchema::Send { payload, .. } = &schema.protocol {
            let props = payload.get("properties");
            assert!(props.is_some(), "Should have properties");
            let props = props.unwrap();
            assert!(props.get("command").is_some(), "Should have command field");
        }
    }

    #[test]
    fn test_schema_serialization() {
        let schema = execute_schema();
        let json = serde_json::to_string_pretty(&schema).unwrap();

        println!("Execute method schema:\n{}", json);

        assert!(json.contains("execute"));
        assert!(json.contains("protocol"));
        assert!(json.contains("server_protocol"));
        assert!(json.contains("ExecuteInput"));
    }
}
