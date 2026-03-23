//! Schema comparison tool - dumps schemas for bash (hub_methods) and ping (#[activation])

use plexus_substrate::activations::bash::Bash;
use plexus_substrate::activations::ping::Ping;
use plexus_substrate::plexus::Activation;

fn main() {
    // Get bash schema (hub_methods)
    let bash = Bash::new();
    let bash_schema = bash.plugin_schema();
    let bash_json = serde_json::to_string_pretty(&bash_schema)
        .expect("Failed to serialize bash schema");

    println!("=== BASH (hub_methods) SCHEMA ===");
    println!("{}", bash_json);
    println!();

    // Get ping schema (#[activation])
    let ping = Ping::new();
    let ping_schema = ping.plugin_schema();
    let ping_json = serde_json::to_string_pretty(&ping_schema)
        .expect("Failed to serialize ping schema");

    println!("=== PING (#[activation]) SCHEMA ===");
    println!("{}", ping_json);
    println!();

    // Write to files
    std::fs::write("/tmp/bash_schema.json", &bash_json)
        .expect("Failed to write bash schema");
    std::fs::write("/tmp/ping_schema.json", &ping_json)
        .expect("Failed to write ping schema");

    println!("Schemas written to /tmp/bash_schema.json and /tmp/ping_schema.json");
}
