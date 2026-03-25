# Schema Generation Comparison: hub_methods vs #[activation]

**Date**: 2026-03-25
**Context**: Macro migration root cause analysis
**Related**: `16672335815491646207-activation-macro-schema-endpoint-bug.md`

## Overview

This document provides a detailed technical comparison of schema functionality generation between the old `#[hub_methods]` macro (plexus-macros) and the new `#[activation]` macro (plexus-derive). It explains exactly what code each macro generates and identifies the missing pieces that cause schema endpoints to fail.

## The Schema System Architecture

Before diving into the comparison, understand that Plexus schema functionality operates at three layers:

### Layer 1: Trait Definition (Activation trait)
```rust
// In plexus-core/src/plexus/plexus.rs
pub trait Activation: Send + Sync {
    type Methods: MethodEnumSchema;

    fn namespace(&self) -> &str;
    fn version(&self) -> &str;
    fn methods(&self) -> Vec<&str>;
    fn method_help(&self, method: &str) -> Option<String>;
    fn plugin_schema(&self) -> PluginSchema;

    // Dynamic dispatch for all methods including schema
    async fn call(&self, method: &str, params: Value)
        -> Result<PlexusStream, PlexusError>;

    // Convert to jsonrpsee RPC methods
    fn into_rpc_methods(self) -> jsonrpsee::core::server::Methods;
}
```

### Layer 2: Method Registration (jsonrpsee)
RPC methods must be registered with jsonrpsee to be callable over the wire:
```rust
// Methods are registered via trait implementation
#[jsonrpsee::proc_macros::rpc(server, namespace = "bash")]
pub trait BashRpc {
    #[subscription(name = "execute", ...)]
    async fn execute(&self, ...) -> SubscriptionResult;

    // Missing in new macro:
    #[subscription(name = "schema", ...)]
    async fn schema(&self, ...) -> SubscriptionResult;
}
```

### Layer 3: Dynamic Routing (substrate main.rs)
Substrate's main.rs sets up a routing function for methods not found in direct RPC:
```rust
let route_fn: RouteFn = Arc::new(move |method, params| {
    let hub = hub_route.clone();
    Box::pin(async move { hub.route(&method, params).await })
});
```

**The Problem**: The new macro only generates Layer 1, skipping Layer 2 for schema.

---

## Old Macro: hub_methods (plexus-macros)

Source: `/workspace/hypermemetic/plexus-macros/src/codegen/activation.rs`

### What It Generates

Given this input:
```rust
#[hub_methods(namespace = "bash", version = "1.0.0")]
impl Bash {
    #[hub_method]
    async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
        self.executor.execute(&command).await
    }
}
```

The old macro generates **four key pieces** for schema functionality:

### Piece 1: Schema in methods() List

```rust
// In codegen/activation.rs:244-246
impl Activation for Bash {
    fn methods(&self) -> Vec<&str> {
        vec![
            "execute",
            "schema"  // ← Added by macro
        ]
    }
}
```

**Purpose**: Advertises that `schema` is a callable method.

### Piece 2: Schema in method_help()

```rust
// In codegen/activation.rs:248-254
impl Activation for Bash {
    fn method_help(&self, method: &str) -> Option<String> {
        match method {
            "execute" => Some("Execute a bash command...".to_string()),
            "schema" => Some(
                "Get plugin or method schema. Pass {\"method\": \"name\"} for a specific method."
                    .to_string()
            ),
            _ => None,
        }
    }
}
```

**Purpose**: Provides help text for the schema method.

### Piece 3: Schema in call() Dispatch

```rust
// In codegen/activation.rs:256-313
impl Activation for Bash {
    async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<PlexusStream, PlexusError> {
        match method {
            "execute" => {
                // Parse params, call self.execute(), wrap stream
                let command: String = params.get("command")...;
                let stream = Bash::execute(self, command).await;
                Ok(wrap_stream(stream, "bash.execute", vec!["bash".into()]))
            }
            "schema" => {
                // Check if a specific method was requested
                let method_name: Option<String> = params.get("method")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let plugin_schema = self.plugin_schema();

                let result = if let Some(ref name) = method_name {
                    // Return specific method schema
                    plugin_schema.methods.iter()
                        .find(|m| m.name == *name)
                        .map(|m| SchemaResult::Method(m.clone()))
                        .ok_or_else(|| PlexusError::MethodNotFound {
                            activation: "bash".to_string(),
                            method: name.clone(),
                        })?
                } else {
                    // Return full plugin schema
                    SchemaResult::Plugin(plugin_schema)
                };

                Ok(wrap_stream(
                    futures::stream::once(async move { result }),
                    "bash.schema",
                    vec!["bash".into()]
                ))
            }
            _ => {
                // For leaf plugins: method not found
                // For hub plugins: try routing to child
                Err(PlexusError::MethodNotFound {
                    activation: "bash".to_string(),
                    method: method.to_string(),
                })
            }
        }
    }
}
```

**Purpose**: Implements the actual schema logic. This is the **functional core** of schema.

### Piece 4: Schema in method_schemas()

```rust
// In codegen/method_enum.rs:301-310
impl BashMethod {
    fn compute_method_schemas() -> Vec<MethodSchema> {
        let mut methods = vec![
            // execute method with full params/returns schemas
            MethodSchema::new("execute", "Execute a bash command...", "3c23da789c6f1b06")
                .with_params(...)
                .with_returns(...)
                .with_streaming(false),
        ];

        // Add the auto-generated schema method
        let schema_method = MethodSchema::new(
            "schema".to_string(),
            "Get plugin or method schema. Pass {\"method\": \"name\"} for a specific method.".to_string(),
            "auto_schema".to_string(), // Fixed hash since it's auto-generated
        );
        methods.push(schema_method);

        methods
    }
}
```

**Purpose**: Includes schema in the metadata returned by `plugin_schema()`.

### How It Works Together

When a client calls `bash.schema`:

1. **jsonrpsee receives** the RPC call
2. **No direct RPC method** named `bash_schema` exists (same as new macro!)
3. **Substrate's route_fn** catches the missing method:
   ```rust
   hub.route("bash.schema", params).await
   ```
4. **DynamicHub.route()** parses "bash.schema" → calls `bash_activation.call("schema", params)`
5. **Activation::call() match** hits the `"schema"` arm (Piece 3)
6. **Schema logic** executes, returns wrapped stream with schema data

**Key Insight**: The old macro **never generated a direct RPC method** for schema either! It relied on dynamic routing through `Activation::call()`.

---

## New Macro: #[activation] (plexus-derive)

Source: `/workspace/hypermemetic/plexus-derive/src/codegen/`

### What It Generates

Given this input:
```rust
#[activation(namespace = "bash", version = "1.0.0", plexus)]
impl Bash {
    async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
        self.executor.execute(&command).await
    }
}
```

The new macro generates:

### Generated Code

```rust
// In codegen/core.rs - Method Enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BashMethod {
    Execute,
    // NO Schema variant!
}

impl BashMethod {
    fn method_schemas() -> Vec<MethodSchema> {
        // Schema for execute method
        vec![
            MethodSchema::new("execute", "...", "hash")
                .with_params(...)
                .with_returns(...),
        ]
        // NO schema method added!
    }
}

// In codegen/plexus_jsonrpc.rs - RPC Trait
#[jsonrpsee::proc_macros::rpc(server, namespace = "bash")]
pub trait BashRpc {
    #[subscription(name = "execute", unsubscribe = "unsubscribe_execute", item = Value)]
    async fn execute(&self, command: String) -> SubscriptionResult;

    // NO schema method!
}

// RPC Implementation
#[async_trait::async_trait]
impl BashRpcServer for Bash {
    async fn execute(
        &self,
        subscription_sink: PendingSubscriptionSink,
        command: String,
    ) -> SubscriptionResult {
        let sink = subscription_sink.accept().await?;
        let stream = Bash::execute(self, command).await;
        let wrapped = wrap_stream(stream, "bash.execute", vec!["bash".into()]);

        tokio::spawn(async move {
            let mut stream = wrapped;
            while let Some(item) = stream.next().await {
                if let Ok(raw) = serde_json::value::to_raw_value(&item) {
                    if sink.send(raw).await.is_err() { break; }
                }
            }
        });

        Ok(())
    }

    // NO schema implementation!
}

// In codegen/core.rs - Activation Trait Impl
#[async_trait::async_trait]
impl plexus_core::plexus::Activation for Bash {
    type Methods = BashMethod;

    fn namespace(&self) -> &str { "bash" }
    fn version(&self) -> &str { "1.0.0" }
    fn description(&self) -> &str { "Execute bash commands and stream output" }

    fn methods(&self) -> Vec<&str> {
        vec!["execute"]  // NO "schema"!
    }

    fn method_help(&self, method: &str) -> Option<String> {
        match method {
            "execute" => Some("Execute a bash command...".to_string()),
            // NO "schema" arm!
            _ => None,
        }
    }

    async fn call(&self, method: &str, params: Value)
        -> Result<PlexusStream, PlexusError>
    {
        match method {
            "execute" => {
                // Deserialize params
                #[derive(Deserialize)]
                struct Params { command: String }
                let p: Params = serde_json::from_value(params)?;

                // Call method
                let stream = Bash::execute(self, p.command).await;
                Ok(wrap_stream(stream, "bash.execute", vec!["bash".into()]))
            }
            // NO "schema" arm!
            _ => {
                // For leaf plugins: return MethodNotFound
                Err(PlexusError::MethodNotFound {
                    activation: "bash".to_string(),
                    method: method.to_string(),
                })
            }
        }
    }

    fn into_rpc_methods(self) -> Methods {
        self.into_rpc().into()
    }

    fn plugin_schema(&self) -> PluginSchema {
        PluginSchema::new(
            "bash",
            "1.0.0",
            "Execute bash commands and stream output",
            BashMethod::method_schemas(),  // Only has "execute"!
        )
    }
}
```

### What's Missing

Comparing the generated code:

| Feature | Old Macro (hub_methods) | New Macro (#[activation]) | Status |
|---------|------------------------|---------------------------|---------|
| **Schema in methods() list** | ✅ `vec!["execute", "schema"]` | ❌ `vec!["execute"]` | MISSING |
| **Schema in method_help()** | ✅ `"schema" => Some(...)` | ❌ No arm | MISSING |
| **Schema in call() dispatch** | ✅ `"schema" => { /* logic */ }` | ❌ No arm | MISSING |
| **Schema in method_schemas()** | ✅ Appends schema MethodSchema | ❌ Only user methods | MISSING |
| **Schema RPC method** | ❌ Not generated | ❌ Not generated | BOTH MISSING |

**Critical Missing Piece**: The `"schema"` arm in `call()` dispatch (Piece 3 above).

---

## Why The Old Macro Worked

The old macro's schema functionality worked through **dynamic routing**:

```
Client calls: bash.schema
    ↓
jsonrpsee: No bash_schema RPC method found
    ↓
substrate route_fn: hub.route("bash.schema", params)
    ↓
DynamicHub.route(): Splits "bash.schema" → activation="bash", method="schema"
    ↓
DynamicHub: Calls bash_activation.call("schema", params)
    ↓
Activation::call() match: "schema" => { /* Piece 3 logic */ }
    ↓
Returns: wrapped stream with schema data
```

**Key components**:
1. **substrate/main.rs** provides `route_fn` for missing methods
2. **DynamicHub.route()** does method dispatch
3. **Activation::call()** has `"schema"` arm that executes the logic

---

## Why The New Macro Fails

The new macro's schema call fails at the `Activation::call()` level:

```
Client calls: bash.schema
    ↓
jsonrpsee: No bash_schema RPC method found
    ↓
substrate route_fn: hub.route("bash.schema", params)
    ↓
DynamicHub.route(): Splits "bash.schema" → activation="bash", method="schema"
    ↓
DynamicHub: Calls bash_activation.call("schema", params)
    ↓
Activation::call() match: NO "schema" arm!
    ↓
Falls through to _ => Err(MethodNotFound)
    ↓
Returns: PlexusError::MethodNotFound
    ↓
jsonrpsee: Wraps as RPC error -32603 "Internal error"
```

**The break point**: `Activation::call()` has no `"schema"` match arm.

---

## The Missing Code

Here's exactly what needs to be added to the new macro's generated `Activation::call()`:

```rust
async fn call(&self, method: &str, params: Value)
    -> Result<PlexusStream, PlexusError>
{
    match method {
        "execute" => {
            // Existing code...
        }

        // ADD THIS:
        "schema" => {
            // Check if a specific method was requested
            let method_name: Option<String> = params.get("method")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let plugin_schema = self.plugin_schema();

            let result = if let Some(ref name) = method_name {
                // Find the specific method
                plugin_schema.methods.iter()
                    .find(|m| m.name == *name)
                    .map(|m| plexus_core::plexus::SchemaResult::Method(m.clone()))
                    .ok_or_else(|| plexus_core::plexus::PlexusError::MethodNotFound {
                        activation: #namespace.to_string(),
                        method: name.clone(),
                    })?
            } else {
                // Return full plugin schema
                plexus_core::plexus::SchemaResult::Plugin(plugin_schema)
            };

            Ok(plexus_core::plexus::wrap_stream(
                futures::stream::once(async move { result }),
                concat!(#namespace, ".schema"),
                vec![#namespace.into()]
            ))
        }

        _ => {
            // Existing fallback...
        }
    }
}
```

And add `"schema"` to the other lists:

```rust
fn methods(&self) -> Vec<&str> {
    vec![#(#method_names,)* "schema"]  // Add "schema" here
}

fn method_help(&self, method: &str) -> Option<String> {
    match method {
        #(#help_arms)*
        "schema" => Some("Get plugin or method schema. Pass {\"method\": \"name\"} for a specific method.".to_string()),
        _ => None,
    }
}
```

And add schema to `method_schemas()`:

```rust
impl #enum_name {
    pub fn method_schemas() -> Vec<MethodSchema> {
        let mut methods = vec![
            // User-defined methods with full schemas
            #(#method_schema_entries)*
        ];

        // Add auto-generated schema method
        methods.push(MethodSchema::new(
            "schema".to_string(),
            "Get plugin or method schema. Pass {\"method\": \"name\"} for a specific method.".to_string(),
            "auto_schema".to_string(),
        ));

        methods
    }
}
```

---

## Alternative: Generate RPC Method (Not Recommended)

While it would be **technically correct** to generate a direct RPC method:

```rust
#[jsonrpsee::proc_macros::rpc(server, namespace = "bash")]
pub trait BashRpc {
    #[subscription(name = "execute", ...)]
    async fn execute(&self, command: String) -> SubscriptionResult;

    // ADD THIS:
    #[subscription(name = "schema", unsubscribe = "unsubscribe_schema", item = Value)]
    async fn schema(&self, method: Option<String>) -> SubscriptionResult;
}
```

This is **not recommended** because:

1. **Inconsistent with old behavior** - Old macro used dynamic routing, changing this is a breaking change
2. **Code duplication** - Every activation gets identical schema RPC code
3. **Larger binary** - Increases binary size unnecessarily
4. **Breaks dynamic routing** - Clients expect `namespace.schema` pattern

The **correct fix** is to add the schema arm to `Activation::call()` to match the old macro's behavior.

---

## Summary Table

| Layer | Old Macro | New Macro | Fix Needed |
|-------|-----------|-----------|------------|
| **methods() list** | ✅ Includes "schema" | ❌ Missing | Add "schema" to vec |
| **method_help()** | ✅ Has schema arm | ❌ Missing | Add schema match arm |
| **call() dispatch** | ✅ Has schema arm with logic | ❌ Missing | Add schema match arm with full logic |
| **method_schemas()** | ✅ Appends schema MethodSchema | ❌ Missing | Append schema to methods vec |
| **RPC endpoint** | ❌ Uses dynamic routing | ❌ Uses dynamic routing | No change needed |

## Implementation Location

All fixes needed in: **`plexus-derive/src/codegen/core.rs`**

Specifically in the `generate_activation_impl()` function which generates the `Activation` trait implementation.

## Testing Validation

After implementing the fix, validate with:

```bash
# Start fixed substrate
plexus-substrate --port 4444

# Test schema endpoint
synapse -P 4444 substrate bash schema

# Should return complete schema with:
# - methods: ["execute", "schema"]
# - Full JSON schemas for params/returns
# - Proper hashes for all methods
```

## References

- Old macro source: `plexus-macros/src/codegen/activation.rs:244-313`
- New macro source: `plexus-derive/src/codegen/core.rs`
- Dynamic routing: `plexus-substrate/src/main.rs:249-251`
- DynamicHub routing: `plexus-core/src/plexus/plexus.rs`
