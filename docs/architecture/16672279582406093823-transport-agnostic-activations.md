# Transport-Agnostic Activations

**Date**: 2026-03-25
**Status**: Architectural Design - Core Principle
**Supersedes**: Portions of `16672310277095318271-protocol-methods-and-projections.md`

## Core Principle

**Activations are transport-agnostic.** They implement a clean `Activation` interface that knows nothing about JSON-RPC, REST, gRPC, or any specific protocol. Transports are **adapters** that wrap activations and expose them through their protocol.

## The Problem With Current Architecture

### What We Had (Wrong)

```rust
// Activation knows about jsonrpsee!
#[activation(namespace = "bash", plexus)]
impl Bash {
    async fn execute(&self, ...) -> impl Stream<Item = BashEvent> { ... }
}

// Generates jsonrpsee-specific code IN the activation
#[jsonrpsee::proc_macros::rpc(server, namespace = "bash")]
pub trait BashRpc {
    #[subscription(...)]
    async fn execute(&self, ...) -> SubscriptionResult;
}

impl BashRpcServer for Bash { ... }
```

**Problems:**
- ❌ Activation is coupled to jsonrpsee
- ❌ Can't use the same activation over REST/gRPC without modifications
- ❌ Transport logic mixed with business logic
- ❌ Hard to test (need to mock jsonrpsee types)

### What We Need (Right)

```rust
// Activation is pure business logic
#[activation(namespace = "bash", version = "1.0.0")]
impl Bash {
    async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
        self.executor.execute(&command).await
    }
}

// Implements transport-agnostic Activation trait
impl Activation for Bash {
    async fn call(&self, method: &str, params: Value)
        -> Result<PlexusStream, PlexusError>;

    async fn schema(&self, method: Option<String>)
        -> Result<PlexusStream, PlexusError>;

    async fn _protocol(&self)
        -> Result<PlexusStream, PlexusError>;

    async fn _info(&self)
        -> Result<PlexusStream, PlexusError>;
}

// JSON-RPC transport WRAPS the activation (separate generated code)
pub struct BashJsonRpcAdapter {
    inner: Arc<Bash>,
}

impl BashRpcServer for BashJsonRpcAdapter {
    async fn execute(&self, sink: PendingSubscriptionSink, command: String)
        -> SubscriptionResult
    {
        // Adapt activation to jsonrpsee
        let stream = self.inner.call("execute", json!({"command": command}))?;
        jsonrpsee_adapter::forward_stream(stream, sink).await
    }
}
```

**Benefits:**
- ✅ Activation has zero transport dependencies
- ✅ Same activation works with any transport
- ✅ Transport logic separated into adapters
- ✅ Easy to test (pure domain logic)

---

## The Activation Interface

### Core Trait

```rust
// In plexus-core/src/plexus/activation.rs
#[async_trait]
pub trait Activation: Send + Sync {
    /// Activation namespace (e.g., "bash", "solar")
    fn namespace(&self) -> &str;

    /// Activation version (semver)
    fn version(&self) -> &str;

    /// Plugin ID (stable UUID)
    fn plugin_id(&self) -> Uuid;

    /// Is this a hub (has children) or leaf?
    fn is_hub(&self) -> bool;

    /// Protocol methods (what the activation supports)
    /// - Meta-methods: _protocol, _info
    /// - Protocol methods: call, schema
    /// - User methods: execute, info, etc.
    fn methods(&self) -> Vec<&str>;

    /// Get help text for a method
    fn method_help(&self, method: &str) -> Option<String>;

    /// Get full schema for this activation or a specific method
    async fn schema(&self, method: Option<String>)
        -> Result<PlexusStream, PlexusError>;

    /// Route a method call (handles local methods and children)
    async fn call(&self, method: &str, params: Value)
        -> Result<PlexusStream, PlexusError>;

    /// Get protocol information (meta-method)
    async fn _protocol(&self)
        -> Result<PlexusStream, PlexusError>;

    /// Get activation information (meta-method)
    async fn _info(&self)
        -> Result<PlexusStream, PlexusError>;
}
```

### Key Types (Transport-Agnostic)

```rust
/// A stream of items wrapped with Plexus metadata
pub struct PlexusStream {
    inner: Pin<Box<dyn Stream<Item = PlexusStreamItem> + Send>>,
}

/// Items in a Plexus stream
#[derive(Serialize, Deserialize)]
pub enum PlexusStreamItem {
    /// User data (the actual event/result)
    Data {
        content: Value,
        content_type: String,
        provenance: Vec<String>,
    },

    /// Stream completed successfully
    Done {
        provenance: Vec<String>,
    },

    /// Error occurred
    Error {
        error: PlexusError,
        provenance: Vec<String>,
    },
}

/// Errors that can occur in Plexus
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlexusError {
    MethodNotFound { activation: String, method: String },
    ActivationNotFound { name: String },
    InvalidParams { message: String },
    Internal { message: String },
    // ...
}
```

**Important**: These types know NOTHING about JSON-RPC, REST, gRPC, etc. They're pure domain types.

---

## The Transport Layer

### What is a Transport?

A **transport** is an adapter that:
1. Wraps an `Activation`
2. Exposes it through a specific protocol (JSON-RPC, REST, gRPC)
3. Handles protocol-specific concerns (subscriptions, HTTP methods, error codes)

### Transport Architecture

```
┌─────────────────────────────────────────┐
│         Application Layer               │
│  (User methods: execute, info, etc.)    │
└──────────────┬──────────────────────────┘
               │ implements
               ↓
┌─────────────────────────────────────────┐
│      Activation Trait (Core)            │
│  (call, schema, _protocol, _info)       │
└──────────────┬──────────────────────────┘
               │ wrapped by
         ┌─────┴─────┬─────────┬──────────┐
         ↓           ↓         ↓          ↓
    ┌────────┐  ┌────────┐  ┌──────┐  ┌─────┐
    │JSON-RPC│  │  REST  │  │ gRPC │  │ MCP │
    │Adapter │  │Adapter │  │Adapter│ │Adapt│
    └────────┘  └────────┘  └──────┘  └─────┘
```

### Example: JSON-RPC Transport

```rust
// Generated by #[transport(jsonrpc)] or similar
pub struct BashJsonRpcTransport {
    activation: Arc<Bash>,
}

impl BashJsonRpcTransport {
    pub fn new(activation: Bash) -> Self {
        Self {
            activation: Arc::new(activation),
        }
    }

    /// Convert to jsonrpsee Methods for registration
    pub fn into_rpc_methods(self) -> jsonrpsee::Methods {
        let mut module = RpcModule::new(self);

        // Register user methods
        module.register_subscription(
            "execute",
            "unsubscribe_execute",
            |params, sink, ctx| async move {
                // Parse params
                let command: String = params.one()?;

                // Call activation
                let stream = ctx.activation.call("execute", json!({"command": command})).await?;

                // Adapt stream to jsonrpsee
                forward_plexus_stream_to_jsonrpsee(stream, sink).await
            }
        ).unwrap();

        // Register protocol methods
        module.register_subscription(
            "call",
            "unsubscribe_call",
            |params, sink, ctx| async move {
                let (method, params): (String, Value) = params.parse()?;
                let stream = ctx.activation.call(&method, params).await?;
                forward_plexus_stream_to_jsonrpsee(stream, sink).await
            }
        ).unwrap();

        module.register_subscription(
            "schema",
            "unsubscribe_schema",
            |params, sink, ctx| async move {
                let method: Option<String> = params.optional()?;
                let stream = ctx.activation.schema(method).await?;
                forward_plexus_stream_to_jsonrpsee(stream, sink).await
            }
        ).unwrap();

        // Register meta-methods
        module.register_subscription("_protocol", "unsubscribe__protocol", ...).unwrap();
        module.register_subscription("_info", "unsubscribe__info", ...).unwrap();

        module.into()
    }
}

/// Adapter function: PlexusStream → jsonrpsee subscription
async fn forward_plexus_stream_to_jsonrpsee(
    mut stream: PlexusStream,
    sink: SubscriptionSink,
) -> Result<(), jsonrpsee::types::Error> {
    while let Some(item) = stream.next().await {
        match item {
            PlexusStreamItem::Data { content, .. } => {
                let raw = serde_json::value::to_raw_value(&content)?;
                sink.send(raw).await?;
            }
            PlexusStreamItem::Done { .. } => {
                break;
            }
            PlexusStreamItem::Error { error, .. } => {
                return Err(plexus_error_to_jsonrpsee(error));
            }
        }
    }
    Ok(())
}

/// Adapter function: PlexusError → jsonrpsee Error
fn plexus_error_to_jsonrpsee(error: PlexusError) -> jsonrpsee::types::Error {
    match error {
        PlexusError::MethodNotFound { .. } => {
            ErrorObject::owned(-32601, "Method not found", Some(error))
        }
        PlexusError::InvalidParams { .. } => {
            ErrorObject::owned(-32602, "Invalid params", Some(error))
        }
        PlexusError::Internal { .. } => {
            ErrorObject::owned(-32603, "Internal error", Some(error))
        }
        // ...
    }
}
```

### Example: REST Transport

```rust
pub struct BashRestTransport {
    activation: Arc<Bash>,
}

impl BashRestTransport {
    /// Convert to Axum router
    pub fn into_router(self) -> axum::Router {
        axum::Router::new()
            // User methods
            .route("/execute", post(execute_handler))

            // Protocol methods
            .route("/call", post(call_handler))
            .route("/schema", get(schema_handler))

            // Meta-methods
            .route("/_protocol", get(protocol_handler))
            .route("/_info", get(info_handler))

            .with_state(self)
    }
}

async fn execute_handler(
    State(transport): State<BashRestTransport>,
    Json(params): Json<ExecuteParams>,
) -> Result<impl IntoResponse, RestError> {
    // Call activation
    let stream = transport.activation.call("execute", serde_json::to_value(params)?).await?;

    // Adapt to SSE (Server-Sent Events) for streaming
    let sse_stream = stream.map(|item| {
        match item {
            PlexusStreamItem::Data { content, .. } => {
                Event::default().json_data(content)
            }
            PlexusStreamItem::Done { .. } => {
                Event::default().data("[DONE]")
            }
            PlexusStreamItem::Error { error, .. } => {
                Event::default().json_data(error)
            }
        }
    });

    Ok(Sse::new(sse_stream))
}
```

### Example: gRPC Transport

```rust
// Generated from .proto
pub struct BashGrpcService {
    activation: Arc<Bash>,
}

#[tonic::async_trait]
impl bash_proto::bash_server::Bash for BashGrpcService {
    type ExecuteStream = ReceiverStream<Result<ExecuteResponse, Status>>;

    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<Self::ExecuteStream>, Status> {
        let params = request.into_inner();

        // Call activation
        let stream = self.activation
            .call("execute", serde_json::to_value(params).unwrap())
            .await
            .map_err(plexus_error_to_grpc_status)?;

        // Adapt to gRPC stream
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            while let Some(item) = stream.next().await {
                match item {
                    PlexusStreamItem::Data { content, .. } => {
                        let response: ExecuteResponse = serde_json::from_value(content).unwrap();
                        tx.send(Ok(response)).await.unwrap();
                    }
                    PlexusStreamItem::Error { error, .. } => {
                        tx.send(Err(plexus_error_to_grpc_status(error))).await.unwrap();
                        break;
                    }
                    PlexusStreamItem::Done { .. } => break,
                }
            }
        });

        Ok(Response::new(ReceiverStream::new(rx)))
    }
}
```

---

## The Macro System

### What the Macro Generates

The `#[activation]` macro generates **ONLY** the transport-agnostic code:

```rust
#[activation(namespace = "bash", version = "1.0.0")]
impl Bash {
    async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
        self.executor.execute(&command).await
    }
}

// Macro generates:
impl Bash {
    pub const NAMESPACE: &'static str = "bash";
    pub const PLUGIN_ID: Uuid = uuid!("...");

    pub fn namespace(&self) -> &str { "bash" }
    pub fn version(&self) -> &str { "1.0.0" }
    pub fn plugin_id(&self) -> Uuid { Self::PLUGIN_ID }

    // Helper to get PluginSchema
    pub fn plugin_schema(&self) -> PluginSchema { ... }
}

#[async_trait]
impl Activation for Bash {
    fn namespace(&self) -> &str { "bash" }
    fn version(&self) -> &str { "1.0.0" }
    fn plugin_id(&self) -> Uuid { Self::PLUGIN_ID }
    fn is_hub(&self) -> bool { false }

    fn methods(&self) -> Vec<&str> {
        vec!["execute", "call", "schema", "_protocol", "_info"]
    }

    fn method_help(&self, method: &str) -> Option<String> {
        match method {
            "execute" => Some("Execute a bash command...".into()),
            "call" => Some("Route to sub-methods".into()),
            "schema" => Some("Get schema information".into()),
            "_protocol" => Some("Get protocol information".into()),
            "_info" => Some("Get activation information".into()),
            _ => None,
        }
    }

    async fn call(&self, method: &str, params: Value)
        -> Result<PlexusStream, PlexusError>
    {
        match method {
            "execute" => {
                // Parse params
                #[derive(Deserialize)]
                struct Params { command: String }
                let p: Params = serde_json::from_value(params)?;

                // Call user method
                let stream = Bash::execute(self, p.command).await;

                // Wrap in PlexusStream
                let wrapped = stream.map(|event| PlexusStreamItem::Data {
                    content: serde_json::to_value(event).unwrap(),
                    content_type: "bash.execute".into(),
                    provenance: vec!["bash".into()],
                });

                Ok(PlexusStream::new(wrapped))
            }
            _ => Err(PlexusError::MethodNotFound {
                activation: "bash".into(),
                method: method.into(),
            })
        }
    }

    async fn schema(&self, method: Option<String>)
        -> Result<PlexusStream, PlexusError>
    {
        let schema = self.plugin_schema();
        let result = if let Some(name) = method {
            schema.methods.iter()
                .find(|m| m.name == name)
                .map(|m| SchemaResult::Method(m.clone()))
                .ok_or(PlexusError::MethodNotFound { ... })?
        } else {
            SchemaResult::Plugin(schema)
        };

        Ok(PlexusStream::once(result))
    }

    async fn _protocol(&self) -> Result<PlexusStream, PlexusError> {
        let info = ProtocolInfo {
            version: "2.0".into(),
            meta_methods: vec!["_protocol".into(), "_info".into()],
            protocol_methods: vec![
                ProtocolMethod {
                    name: "call".into(),
                    description: "Route to sub-methods".into(),
                },
                ProtocolMethod {
                    name: "schema".into(),
                    description: "Get schema information".into(),
                },
            ],
            user_methods: vec!["execute".into()],
        };

        Ok(PlexusStream::once(info))
    }

    async fn _info(&self) -> Result<PlexusStream, PlexusError> {
        let info = ActivationInfo {
            backend: "substrate".into(),
            backend_version: env!("CARGO_PKG_VERSION").into(),
            namespace: self.namespace().into(),
            version: self.version().into(),
            is_hub: self.is_hub(),
            children: None,
            plugin_id: self.plugin_id().to_string(),
            hash: "2de7d5478ddd4205".into(),
        };

        Ok(PlexusStream::once(info))
    }
}
```

**Key point**: NO transport-specific code. No jsonrpsee, no REST, no gRPC.

### Transport Registration

Transports are registered **separately** from the activation:

```rust
// In substrate/main.rs or a transport config
async fn main() {
    // Create activation (pure business logic)
    let bash = Bash::new();

    // Wrap in transports as needed
    let jsonrpc_transport = BashJsonRpcTransport::new(bash.clone());
    let rest_transport = BashRestTransport::new(bash.clone());
    let grpc_transport = BashGrpcService::new(bash.clone());

    // Register with respective servers
    let jsonrpc_methods = jsonrpc_transport.into_rpc_methods();
    let rest_router = rest_transport.into_router();
    let grpc_service = BashGrpcServer::new(grpc_transport);

    // Serve all transports
    tokio::join!(
        serve_jsonrpc(jsonrpc_methods, 4444),
        serve_rest(rest_router, 8080),
        serve_grpc(grpc_service, 50051),
    );
}
```

---

## Benefits

### 1. True Separation of Concerns

```rust
// Business logic (no transport dependencies)
impl Bash {
    async fn execute(&self, command: String) -> impl Stream<Item = BashEvent> {
        self.executor.execute(&command).await
    }
}

// Transport adapter (separate concern)
impl BashRpcServer for BashJsonRpcTransport {
    async fn execute(&self, sink: PendingSubscriptionSink, command: String)
        -> SubscriptionResult
    {
        // Adapt activation to jsonrpsee
    }
}
```

### 2. Multiple Transports Simultaneously

```rust
// Same activation, multiple protocols
let bash = Bash::new();

// Available over JSON-RPC
jsonrpc_server.register(BashJsonRpcTransport::new(bash.clone()));

// AND over REST
rest_server.register(BashRestTransport::new(bash.clone()));

// AND over gRPC
grpc_server.register(BashGrpcService::new(bash));
```

### 3. Easy Testing

```rust
#[tokio::test]
async fn test_bash_execute() {
    let bash = Bash::new();

    // Test activation directly (no transport mocking!)
    let stream = bash.call("execute", json!({"command": "echo hi"})).await.unwrap();

    let items: Vec<_> = stream.collect().await;
    assert!(matches!(items[0], PlexusStreamItem::Data { .. }));
}
```

### 4. Protocol-Agnostic Activation

```rust
// This activation works with ANY transport
// No jsonrpsee dependency
// No REST dependency
// No gRPC dependency
// Pure domain logic

[dependencies]
plexus-core = "0.3"
futures = "0.3"
serde = { version = "1.0", features = ["derive"] }
# That's it!
```

### 5. Transport Evolution

```rust
// Add new transport without touching activations
pub struct BashMqttTransport { ... }
pub struct BashWebSocketTransport { ... }
pub struct BashGraphQLTransport { ... }

// Activations don't need to change!
```

---

## Implementation Strategy

### Phase 1: Define Core Interfaces (plexus-core)

- [ ] Define `Activation` trait (transport-agnostic)
- [ ] Define `PlexusStream` and `PlexusStreamItem` types
- [ ] Define `PlexusError` enum
- [ ] Define protocol types (`ProtocolInfo`, `ActivationInfo`, etc.)

### Phase 2: Update Macro (plexus-derive)

- [ ] Generate only `Activation` trait impl
- [ ] Remove all transport-specific code generation
- [ ] Generate `call`, `schema`, `_protocol`, `_info` implementations
- [ ] Ensure zero transport dependencies in generated code

### Phase 3: Create Transport Adapters

- [ ] Create `plexus-transport-jsonrpc` crate
- [ ] Implement `forward_plexus_stream_to_jsonrpsee`
- [ ] Implement error mapping functions
- [ ] Generate transport wrapper structs

### Phase 4: Update Substrate

- [ ] Remove direct activation RPC registration
- [ ] Wrap activations in JSON-RPC transport
- [ ] Register transport methods with jsonrpsee
- [ ] Test end-to-end

### Phase 5: Add More Transports (Optional)

- [ ] REST transport (`plexus-transport-rest`)
- [ ] gRPC transport (`plexus-transport-grpc`)
- [ ] MCP transport (`plexus-transport-mcp`)

---

## File Structure

```
plexus-core/
  src/plexus/
    activation.rs         # Activation trait
    stream.rs             # PlexusStream types
    error.rs              # PlexusError
    protocol.rs           # Protocol types

plexus-derive/
  src/
    codegen/
      activation.rs       # Generate Activation impl (transport-agnostic)

plexus-transport-jsonrpc/    # New crate!
  src/
    adapter.rs            # PlexusStream → jsonrpsee adapter
    error.rs              # PlexusError → jsonrpsee error
    transport.rs          # JsonRpcTransport wrapper

plexus-transport-rest/       # New crate!
  src/
    adapter.rs            # PlexusStream → SSE/HTTP adapter
    transport.rs          # RestTransport wrapper

plexus-substrate/
  src/
    main.rs               # Wrap activations in transports
```

---

## Summary

The core architectural principle:

> **Activations implement a clean, transport-agnostic `Activation` interface. Transports are adapters that wrap activations and expose them through specific protocols.**

This achieves:
- ✅ Complete separation of concerns
- ✅ Multiple transports for the same activation
- ✅ Easy testing (no transport mocking)
- ✅ Transport evolution without activation changes
- ✅ Clean dependency graph

The activation knows its domain. The transport knows its protocol. They're composed, not entangled.
