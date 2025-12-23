//! Plexus builder - constructs a fully configured Plexus instance
//!
//! This module is used by both the main binary and examples.

use std::path::PathBuf;
use std::sync::Arc;

use crate::activations::{
    arbor::{Arbor, ArborConfig, ArborStorage},
    bash::Bash,
    claudecode::{ClaudeCode, ClaudeCodeStorage, ClaudeCodeStorageConfig},
    cone::{Cone, ConeStorageConfig},
    health::Health,
};
use crate::plexus::Plexus;

/// Get the substrate data directory in the current working directory
pub fn substrate_data_dir() -> PathBuf {
    let cwd = std::env::current_dir().expect("Failed to get current working directory");
    cwd.join(".substrate")
}

/// Ensure the substrate data directory exists and return paths for databases
pub fn init_data_dir() -> std::io::Result<(PathBuf, PathBuf, PathBuf)> {
    let data_dir = substrate_data_dir();
    std::fs::create_dir_all(&data_dir)?;

    let arbor_db = data_dir.join("arbor.db");
    let cone_db = data_dir.join("cone.db");
    let claudecode_db = data_dir.join("claudecode.db");

    Ok((arbor_db, cone_db, claudecode_db))
}

/// Build the plexus with all activations registered
pub async fn build_plexus() -> Plexus {
    let (arbor_db, cone_db, claudecode_db) =
        init_data_dir().expect("Failed to initialize substrate data directory");

    // Create shared arbor storage
    let arbor_config = ArborConfig {
        db_path: arbor_db,
        ..ArborConfig::default()
    };
    let arbor_storage = Arc::new(
        ArborStorage::new(arbor_config)
            .await
            .expect("Failed to initialize Arbor storage"),
    );

    // Cone shares the same arbor storage
    let cone_config = ConeStorageConfig { db_path: cone_db };

    // ClaudeCode shares the same arbor storage
    let claudecode_config = ClaudeCodeStorageConfig {
        db_path: claudecode_db,
    };
    let claudecode_storage = Arc::new(
        ClaudeCodeStorage::new(claudecode_config, arbor_storage.clone())
            .await
            .expect("Failed to initialize ClaudeCode storage"),
    );

    Plexus::new()
        .register(Health::new())
        .register(Bash::new())
        .register(Arbor::with_storage(arbor_storage.clone()))
        .register(Cone::new(cone_config, arbor_storage).await.unwrap())
        .register(ClaudeCode::new(claudecode_storage))
}
