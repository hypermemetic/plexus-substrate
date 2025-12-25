//! Plexus - the central routing layer for activations
//!
//! Plexus IS an activation that also serves as the registry for other activations.
//! It implements the "caller-wraps" streaming architecture.

use super::{
    context::PlexusContext,
    method_enum::MethodEnumSchema,
    schema::Schema,
    streaming::{wrap_stream, PlexusStream},
    types::{PlexusStreamItem, StreamMetadata},
};
use crate::types::Handle;
use async_stream::stream;
use async_trait::async_trait;
use futures::{Stream, StreamExt};
use jsonrpsee::core::{server::Methods, SubscriptionResult};
use jsonrpsee::{proc_macros::rpc, PendingSubscriptionSink, RpcModule};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::pin::Pin;
use std::collections::HashMap;
use std::sync::Arc;

// ============================================================================
// Error Types
// ============================================================================

#[derive(Debug, Clone)]
pub enum PlexusError {
    ActivationNotFound(String),
    MethodNotFound { activation: String, method: String },
    InvalidParams(String),
    ExecutionError(String),
    HandleNotSupported(String),
}

impl std::fmt::Display for PlexusError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlexusError::ActivationNotFound(name) => write!(f, "Activation not found: {}", name),
            PlexusError::MethodNotFound { activation, method } => {
                write!(f, "Method not found: {}.{}", activation, method)
            }
            PlexusError::InvalidParams(msg) => write!(f, "Invalid params: {}", msg),
            PlexusError::ExecutionError(msg) => write!(f, "Execution error: {}", msg),
            PlexusError::HandleNotSupported(activation) => {
                write!(f, "Handle resolution not supported by activation: {}", activation)
            }
        }
    }
}

impl std::error::Error for PlexusError {}

// ============================================================================
// Schema Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActivationInfo {
    pub namespace: String,
    pub version: String,
    pub description: String,
    pub methods: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MethodSchemaInfo {
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<schemars::Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<schemars::Schema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationFullSchema {
    pub namespace: String,
    pub version: String,
    pub description: String,
    pub methods: Vec<MethodSchemaInfo>,
}

// ============================================================================
// Activation Trait
// ============================================================================

#[async_trait]
pub trait Activation: Send + Sync + 'static {
    type Methods: MethodEnumSchema;

    fn namespace(&self) -> &str;
    fn version(&self) -> &str;
    fn description(&self) -> &str { "No description available" }
    fn methods(&self) -> Vec<&str>;
    fn method_help(&self, _method: &str) -> Option<String> { None }

    async fn call(&self, method: &str, params: Value) -> Result<PlexusStream, PlexusError>;
    async fn resolve_handle(&self, _handle: &Handle) -> Result<PlexusStream, PlexusError> {
        Err(PlexusError::HandleNotSupported(self.namespace().to_string()))
    }

    fn into_rpc_methods(self) -> Methods where Self: Sized;

    fn full_schema(&self) -> ActivationFullSchema {
        ActivationFullSchema {
            namespace: self.namespace().to_string(),
            version: self.version().to_string(),
            description: self.description().to_string(),
            methods: self.methods().iter().map(|name| {
                MethodSchemaInfo {
                    name: name.to_string(),
                    description: self.method_help(name).unwrap_or_default(),
                    params: None,
                    returns: None,
                }
            }).collect(),
        }
    }
}

// ============================================================================
// Internal Type-Erased Activation
// ============================================================================

#[async_trait]
trait ActivationObject: Send + Sync + 'static {
    fn namespace(&self) -> &str;
    fn version(&self) -> &str;
    fn description(&self) -> &str;
    fn methods(&self) -> Vec<&str>;
    fn method_help(&self, method: &str) -> Option<String>;
    async fn call(&self, method: &str, params: Value) -> Result<PlexusStream, PlexusError>;
    async fn resolve_handle(&self, handle: &Handle) -> Result<PlexusStream, PlexusError>;
    fn full_schema(&self) -> ActivationFullSchema;
    fn schema(&self) -> Schema;
}

struct ActivationWrapper<A: Activation> {
    inner: A,
}

#[async_trait]
impl<A: Activation> ActivationObject for ActivationWrapper<A> {
    fn namespace(&self) -> &str { self.inner.namespace() }
    fn version(&self) -> &str { self.inner.version() }
    fn description(&self) -> &str { self.inner.description() }
    fn methods(&self) -> Vec<&str> { self.inner.methods() }
    fn method_help(&self, method: &str) -> Option<String> { self.inner.method_help(method) }

    async fn call(&self, method: &str, params: Value) -> Result<PlexusStream, PlexusError> {
        self.inner.call(method, params).await
    }

    async fn resolve_handle(&self, handle: &Handle) -> Result<PlexusStream, PlexusError> {
        self.inner.resolve_handle(handle).await
    }

    fn full_schema(&self) -> ActivationFullSchema { self.inner.full_schema() }

    fn schema(&self) -> Schema {
        let schema = schemars::schema_for!(A::Methods);
        serde_json::from_value(serde_json::to_value(schema).expect("serialize"))
            .expect("parse schema")
    }
}

// ============================================================================
// Plexus Event Types
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HashEvent {
    Hash { value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum ListActivationsEvent {
    Activation {
        namespace: String,
        version: String,
        description: String,
        methods: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum SchemaEvent {
    Schema {
        activations: Vec<ActivationInfo>,
        total_methods: usize,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CallParams {
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

// ============================================================================
// Plexus Method Enum
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "method", rename_all = "snake_case")]
pub enum PlexusMethod {
    Call(CallParams),
    Hash,
    ListActivations,
    Schema,
}

impl PlexusMethod {
    pub fn description(method: &str) -> Option<&'static str> {
        match method {
            "call" => Some("Route a call to a registered activation"),
            "hash" => Some("Get plexus configuration hash"),
            "list_activations" => Some("List all registered activations"),
            "schema" => Some("Get full plexus schema"),
            _ => None,
        }
    }
}

impl MethodEnumSchema for PlexusMethod {
    fn method_names() -> &'static [&'static str] {
        &["call", "hash", "list_activations", "schema"]
    }

    fn schema_with_consts() -> Value {
        serde_json::to_value(schemars::schema_for!(PlexusMethod)).unwrap()
    }
}

// ============================================================================
// Plexus RPC Interface
// ============================================================================

#[rpc(server, namespace = "plexus")]
pub trait PlexusRpc {
    #[subscription(name = "call", unsubscribe = "unsubscribe_call", item = Value)]
    async fn rpc_call(&self, method: String, params: Value) -> SubscriptionResult;

    #[subscription(name = "hash", unsubscribe = "unsubscribe_hash", item = Value)]
    async fn rpc_hash(&self) -> SubscriptionResult;

    #[subscription(name = "list_activations", unsubscribe = "unsubscribe_list_activations", item = Value)]
    async fn rpc_list_activations(&self) -> SubscriptionResult;

    #[subscription(name = "schema", unsubscribe = "unsubscribe_schema", item = Value)]
    async fn rpc_schema(&self) -> SubscriptionResult;
}

// ============================================================================
// Plexus
// ============================================================================

struct PlexusInner {
    activations: HashMap<String, Arc<dyn ActivationObject>>,
    pending_rpc: std::sync::Mutex<Vec<Box<dyn FnOnce() -> Methods + Send>>>,
}

/// Plexus - the central hub that IS an activation and routes to other activations
#[derive(Clone)]
pub struct Plexus {
    inner: Arc<PlexusInner>,
}

impl Plexus {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(PlexusInner {
                activations: HashMap::new(),
                pending_rpc: std::sync::Mutex::new(Vec::new()),
            }),
        }
    }

    /// Register an activation
    pub fn register<A: Activation + Clone>(mut self, activation: A) -> Self {
        let namespace = activation.namespace().to_string();
        let activation_for_rpc = activation.clone();

        let inner = Arc::get_mut(&mut self.inner)
            .expect("Cannot register: Plexus has multiple references");

        inner.activations.insert(namespace, Arc::new(ActivationWrapper { inner: activation }));
        inner.pending_rpc.lock().unwrap()
            .push(Box::new(move || activation_for_rpc.into_rpc_methods()));
        self
    }

    /// List all methods across all activations
    pub fn list_methods(&self) -> Vec<String> {
        let mut methods = Vec::new();

        // Include plexus's own methods
        for m in Activation::methods(self) {
            methods.push(format!("plexus.{}", m));
        }

        // Include registered activation methods
        for (ns, act) in &self.inner.activations {
            for m in act.methods() {
                methods.push(format!("{}.{}", ns, m));
            }
        }
        methods.sort();
        methods
    }

    /// List all activations (including plexus itself)
    pub fn list_activations(&self) -> Vec<ActivationInfo> {
        let mut activations = Vec::new();

        // Include plexus itself
        activations.push(ActivationInfo {
            namespace: Activation::namespace(self).to_string(),
            version: Activation::version(self).to_string(),
            description: Activation::description(self).to_string(),
            methods: Activation::methods(self).iter().map(|s| s.to_string()).collect(),
        });

        // Include registered activations
        for a in self.inner.activations.values() {
            activations.push(ActivationInfo {
                namespace: a.namespace().to_string(),
                version: a.version().to_string(),
                description: a.description().to_string(),
                methods: a.methods().iter().map(|s| s.to_string()).collect(),
            });
        }

        activations
    }

    /// Compute hash for cache invalidation
    pub fn compute_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut strings: Vec<String> = Vec::new();

        // Include plexus itself
        strings.push(format!(
            "{}:{}:{}",
            Activation::namespace(self),
            Activation::version(self),
            Activation::methods(self).join(",")
        ));

        // Include registered activations
        for a in self.inner.activations.values() {
            strings.push(format!("{}:{}:{}", a.namespace(), a.version(), a.methods().join(",")));
        }
        strings.sort();

        let mut hasher = DefaultHasher::new();
        strings.join(";").hash(&mut hasher);
        format!("{:016x}", hasher.finish())
    }

    /// Route a call to the appropriate activation
    pub async fn route(&self, method: &str, params: Value) -> Result<PlexusStream, PlexusError> {
        let (namespace, method_name) = self.parse_method(method)?;

        // Handle plexus's own methods
        if namespace == "plexus" {
            return Activation::call(self, method_name, params).await;
        }

        let activation = self.inner.activations.get(namespace)
            .ok_or_else(|| PlexusError::ActivationNotFound(namespace.to_string()))?;

        activation.call(method_name, params).await
    }

    /// Resolve a handle
    pub async fn resolve_handle(&self, handle: &Handle) -> Result<PlexusStream, PlexusError> {
        let activation = self.inner.activations.get(&handle.plugin)
            .ok_or_else(|| PlexusError::ActivationNotFound(handle.plugin.clone()))?;
        activation.resolve_handle(handle).await
    }

    /// Get activation schema
    pub fn get_activation_schema(&self, namespace: &str) -> Option<Schema> {
        self.inner.activations.get(namespace).map(|a| a.schema())
    }

    /// Get full schemas for all activations (including plexus itself)
    pub fn list_full_schemas(&self) -> Vec<ActivationFullSchema> {
        let mut schemas = Vec::new();

        // Include plexus itself
        schemas.push(Activation::full_schema(self));

        // Include registered activations
        for a in self.inner.activations.values() {
            schemas.push(a.full_schema());
        }

        schemas
    }

    /// Get help for a method
    pub fn get_method_help(&self, method: &str) -> Option<String> {
        let (namespace, method_name) = self.parse_method(method).ok()?;
        let activation = self.inner.activations.get(namespace)?;
        activation.method_help(method_name)
    }

    fn parse_method<'a>(&self, method: &'a str) -> Result<(&'a str, &'a str), PlexusError> {
        let parts: Vec<&str> = method.splitn(2, '.').collect();
        if parts.len() != 2 {
            return Err(PlexusError::InvalidParams(format!("Invalid method format: {}", method)));
        }
        Ok((parts[0], parts[1]))
    }

    fn error_stream(&self, message: String) -> PlexusStream {
        let metadata = StreamMetadata::new(vec!["plexus".into()], self.compute_hash());
        Box::pin(futures::stream::once(async move {
            PlexusStreamItem::Error { metadata, message, code: None, recoverable: false }
        }))
    }

    // Stream helpers
    fn hash_stream(&self) -> Pin<Box<dyn Stream<Item = HashEvent> + Send + 'static>> {
        let hash = self.compute_hash();
        Box::pin(stream! { yield HashEvent::Hash { value: hash }; })
    }

    fn list_activations_stream(&self) -> Pin<Box<dyn Stream<Item = ListActivationsEvent> + Send + 'static>> {
        let activations = self.list_activations();
        Box::pin(stream! {
            for a in activations {
                yield ListActivationsEvent::Activation {
                    namespace: a.namespace,
                    version: a.version,
                    description: a.description,
                    methods: a.methods,
                };
            }
        })
    }

    fn schema_stream(&self) -> Pin<Box<dyn Stream<Item = SchemaEvent> + Send + 'static>> {
        let activations = self.list_activations();
        let total = self.list_methods().len();
        Box::pin(stream! {
            yield SchemaEvent::Schema { activations, total_methods: total };
        })
    }

    /// Convert to RPC module
    pub fn into_rpc_module(self) -> Result<RpcModule<()>, jsonrpsee::core::RegisterMethodError> {
        let mut module = RpcModule::new(());

        PlexusContext::init(self.compute_hash());

        // Add plexus's own RPC methods
        let plexus_methods: Methods = self.clone().into_rpc().into();
        module.merge(plexus_methods)?;

        // Add all registered activation RPC methods
        let pending = std::mem::take(&mut *self.inner.pending_rpc.lock().unwrap());
        for factory in pending {
            module.merge(factory())?;
        }

        Ok(module)
    }
}

impl Default for Plexus {
    fn default() -> Self { Self::new() }
}

// ============================================================================
// Plexus RPC Implementation
// ============================================================================

#[async_trait]
impl PlexusRpcServer for Plexus {
    async fn rpc_call(&self, pending: PendingSubscriptionSink, method: String, params: Value) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let stream_result = self.route(&method, params).await;

        tokio::spawn(async move {
            match stream_result {
                Ok(mut stream) => {
                    while let Some(item) = stream.next().await {
                        if let Ok(raw) = serde_json::value::to_raw_value(&item) {
                            if sink.send(raw).await.is_err() { break; }
                        }
                    }
                }
                Err(e) => {
                    let error = PlexusStreamItem::Error {
                        metadata: StreamMetadata::new(vec!["plexus".into()], PlexusContext::hash()),
                        message: e.to_string(),
                        code: None,
                        recoverable: false,
                    };
                    if let Ok(raw) = serde_json::value::to_raw_value(&error) {
                        let _ = sink.send(raw).await;
                    }
                }
            }
            let done = PlexusStreamItem::Done {
                metadata: StreamMetadata::new(vec!["plexus".into()], PlexusContext::hash()),
            };
            if let Ok(raw) = serde_json::value::to_raw_value(&done) {
                let _ = sink.send(raw).await;
            }
        });
        Ok(())
    }

    async fn rpc_hash(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let stream = self.hash_stream();
        let wrapped = wrap_stream(stream, "plexus.hash", vec!["plexus".into()]);

        tokio::spawn(async move {
            let mut stream = wrapped;
            while let Some(item) = stream.next().await {
                if let Ok(raw) = serde_json::value::to_raw_value(&item) {
                    if sink.send(raw).await.is_err() { break; }
                }
            }
            let done = PlexusStreamItem::Done {
                metadata: StreamMetadata::new(vec!["plexus".into()], PlexusContext::hash()),
            };
            if let Ok(raw) = serde_json::value::to_raw_value(&done) {
                let _ = sink.send(raw).await;
            }
        });
        Ok(())
    }

    async fn rpc_list_activations(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let stream = self.list_activations_stream();
        let wrapped = wrap_stream(stream, "plexus.list_activations", vec!["plexus".into()]);

        tokio::spawn(async move {
            let mut stream = wrapped;
            while let Some(item) = stream.next().await {
                if let Ok(raw) = serde_json::value::to_raw_value(&item) {
                    if sink.send(raw).await.is_err() { break; }
                }
            }
            let done = PlexusStreamItem::Done {
                metadata: StreamMetadata::new(vec!["plexus".into()], PlexusContext::hash()),
            };
            if let Ok(raw) = serde_json::value::to_raw_value(&done) {
                let _ = sink.send(raw).await;
            }
        });
        Ok(())
    }

    async fn rpc_schema(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
        let sink = pending.accept().await?;
        let stream = self.schema_stream();
        let wrapped = wrap_stream(stream, "plexus.schema", vec!["plexus".into()]);

        tokio::spawn(async move {
            let mut stream = wrapped;
            while let Some(item) = stream.next().await {
                if let Ok(raw) = serde_json::value::to_raw_value(&item) {
                    if sink.send(raw).await.is_err() { break; }
                }
            }
            let done = PlexusStreamItem::Done {
                metadata: StreamMetadata::new(vec!["plexus".into()], PlexusContext::hash()),
            };
            if let Ok(raw) = serde_json::value::to_raw_value(&done) {
                let _ = sink.send(raw).await;
            }
        });
        Ok(())
    }
}

// ============================================================================
// Plexus as Activation
// ============================================================================

#[async_trait]
impl Activation for Plexus {
    type Methods = PlexusMethod;

    fn namespace(&self) -> &str { "plexus" }
    fn version(&self) -> &str { "1.0.0" }
    fn description(&self) -> &str { "Central routing and introspection" }

    fn methods(&self) -> Vec<&str> {
        vec!["call", "hash", "list_activations", "schema"]
    }

    fn method_help(&self, method: &str) -> Option<String> {
        PlexusMethod::description(method).map(|s| s.to_string())
    }

    async fn call(&self, method: &str, params: Value) -> Result<PlexusStream, PlexusError> {
        match method {
            "call" => {
                let p: CallParams = serde_json::from_value(params)
                    .map_err(|e| PlexusError::InvalidParams(e.to_string()))?;
                self.route(&p.method, p.params).await
            }
            "hash" => Ok(wrap_stream(self.hash_stream(), "plexus.hash", vec!["plexus".into()])),
            "list_activations" => Ok(wrap_stream(self.list_activations_stream(), "plexus.list_activations", vec!["plexus".into()])),
            "schema" => Ok(wrap_stream(self.schema_stream(), "plexus.schema", vec!["plexus".into()])),
            _ => Err(PlexusError::MethodNotFound {
                activation: "plexus".to_string(),
                method: method.to_string(),
            }),
        }
    }

    fn into_rpc_methods(self) -> Methods {
        self.into_rpc().into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plexus_implements_activation() {
        fn assert_activation<T: Activation>() {}
        assert_activation::<Plexus>();
    }

    #[test]
    fn plexus_methods() {
        let plexus = Plexus::new();
        let methods = plexus.methods();
        assert!(methods.contains(&"call"));
        assert!(methods.contains(&"hash"));
        assert!(methods.contains(&"list_activations"));
        assert!(methods.contains(&"schema"));
    }

    #[test]
    fn plexus_hash_stable() {
        let p1 = Plexus::new();
        let p2 = Plexus::new();
        assert_eq!(p1.compute_hash(), p2.compute_hash());
    }
}
