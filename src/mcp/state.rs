//! MCP Protocol State Machine
//!
//! Implements the state machine required by the Model Context Protocol.
//! The server must be initialized before accepting most requests.

use std::sync::RwLock;

/// MCP protocol states
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum McpState {
    /// Initial state - only `initialize` allowed
    Uninitialized,
    /// After `initialize` received, before `initialized` notification
    Initializing,
    /// Fully operational - all methods allowed
    Ready,
    /// Graceful shutdown in progress
    ShuttingDown,
}

impl std::fmt::Display for McpState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpState::Uninitialized => write!(f, "Uninitialized"),
            McpState::Initializing => write!(f, "Initializing"),
            McpState::Ready => write!(f, "Ready"),
            McpState::ShuttingDown => write!(f, "ShuttingDown"),
        }
    }
}

/// Error type for state machine operations
#[derive(Debug, Clone)]
pub enum McpStateError {
    /// Attempted an invalid state transition
    InvalidTransition { from: McpState, to: McpState },
    /// Operation requires a different state
    WrongState { expected: McpState, actual: McpState },
    /// Operation requires Ready state
    NotReady { actual: McpState },
}

impl std::fmt::Display for McpStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpStateError::InvalidTransition { from, to } => {
                write!(f, "Invalid state transition: {} → {}", from, to)
            }
            McpStateError::WrongState { expected, actual } => {
                write!(f, "Wrong state: expected {}, got {}", expected, actual)
            }
            McpStateError::NotReady { actual } => {
                write!(f, "Server not ready: current state is {}", actual)
            }
        }
    }
}

impl std::error::Error for McpStateError {}

/// Thread-safe MCP state machine
pub struct McpStateMachine {
    state: RwLock<McpState>,
}

impl McpStateMachine {
    /// Create a new state machine in Uninitialized state
    pub fn new() -> Self {
        Self {
            state: RwLock::new(McpState::Uninitialized),
        }
    }

    /// Get the current state
    pub fn current(&self) -> McpState {
        *self.state.read().unwrap()
    }

    /// Attempt to transition to a new state
    ///
    /// Valid transitions:
    /// - Uninitialized → Initializing (on `initialize` request)
    /// - Initializing → Ready (on `initialized` notification)
    /// - Ready → ShuttingDown (on shutdown)
    pub fn transition(&self, to: McpState) -> Result<(), McpStateError> {
        let mut state = self.state.write().unwrap();
        let from = *state;

        let valid = matches!(
            (from, to),
            (McpState::Uninitialized, McpState::Initializing)
                | (McpState::Initializing, McpState::Ready)
                | (McpState::Ready, McpState::ShuttingDown)
        );

        if valid {
            *state = to;
            tracing::debug!(from = %from, to = %to, "MCP state transition");
            Ok(())
        } else {
            Err(McpStateError::InvalidTransition { from, to })
        }
    }

    /// Require a specific state
    pub fn require(&self, required: McpState) -> Result<(), McpStateError> {
        let actual = self.current();
        if actual == required {
            Ok(())
        } else {
            Err(McpStateError::WrongState {
                expected: required,
                actual,
            })
        }
    }

    /// Require the Ready state (convenience method)
    pub fn require_ready(&self) -> Result<(), McpStateError> {
        let actual = self.current();
        if actual == McpState::Ready {
            Ok(())
        } else {
            Err(McpStateError::NotReady { actual })
        }
    }

    /// Check if in Ready state
    pub fn is_ready(&self) -> bool {
        self.current() == McpState::Ready
    }
}

impl Default for McpStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let sm = McpStateMachine::new();
        assert_eq!(sm.current(), McpState::Uninitialized);
    }

    #[test]
    fn test_valid_transitions() {
        let sm = McpStateMachine::new();

        // Uninitialized → Initializing
        assert!(sm.transition(McpState::Initializing).is_ok());
        assert_eq!(sm.current(), McpState::Initializing);

        // Initializing → Ready
        assert!(sm.transition(McpState::Ready).is_ok());
        assert_eq!(sm.current(), McpState::Ready);

        // Ready → ShuttingDown
        assert!(sm.transition(McpState::ShuttingDown).is_ok());
        assert_eq!(sm.current(), McpState::ShuttingDown);
    }

    #[test]
    fn test_invalid_transitions() {
        let sm = McpStateMachine::new();

        // Can't go directly to Ready
        assert!(sm.transition(McpState::Ready).is_err());

        // Can't go directly to ShuttingDown
        assert!(sm.transition(McpState::ShuttingDown).is_err());

        // Transition to Initializing
        sm.transition(McpState::Initializing).unwrap();

        // Can't go back to Uninitialized
        assert!(sm.transition(McpState::Uninitialized).is_err());
    }

    #[test]
    fn test_require_ready() {
        let sm = McpStateMachine::new();

        // Not ready initially
        assert!(sm.require_ready().is_err());

        sm.transition(McpState::Initializing).unwrap();
        assert!(sm.require_ready().is_err());

        sm.transition(McpState::Ready).unwrap();
        assert!(sm.require_ready().is_ok());
    }

    #[test]
    fn test_require_specific_state() {
        let sm = McpStateMachine::new();

        assert!(sm.require(McpState::Uninitialized).is_ok());
        assert!(sm.require(McpState::Ready).is_err());
    }
}
