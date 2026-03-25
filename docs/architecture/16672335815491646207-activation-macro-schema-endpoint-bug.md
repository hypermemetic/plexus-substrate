# Activation Macro Schema Endpoint Bug

**Date**: 2026-03-25
**Status**: CRITICAL BUG - Migration Blocker
**Affects**: plexus-derive v0.1.0, all activations using `#[activation]` macro

## Summary

The new `#[activation]` macro in plexus-derive fails to generate JSON-RPC subscription endpoints for the auto-generated `schema` method, breaking runtime schema introspection. This is a regression from the old `#[hub_methods]` macro which properly exposed schema endpoints.

## Problem Statement

When synapse or clients call `bash.schema` to fetch plugin schemas:
- **OLD substrate** (hub_methods): ✅ Returns complete schema with methods, params, returns, hashes
- **NEW substrate** (#[activation]): ❌ Fails with "RPC error -32603: Internal error"

This breaks the entire Plexus schema introspection system and prevents synapse-cc from generating proper TypeScript clients.

## Root Cause Analysis

### What the Old Macro Did (`#[hub_methods]` in plexus-macros)

The `hub_methods` macro generated schema functionality in **two layers**:

1. **Trait Layer** - `Activation::call()` dispatch:
```rust
// In codegen/activation.rs:264-291
async fn call(&self, method: &str, params: Value) -> Result<PlexusStream> {
    match method {
        #(#dispatch_arms)*
        "schema" => {
            // Auto-generated schema handling
            let method_name: Option<String> = params.get("method")...;
            let plugin_schema = self.plugin_schema();
            let result = if let Some(ref name) = method_name {
                // Return specific method schema
            } else {
                // Return full plugin schema
            };
            Ok(wrap_stream(...))
        }
        _ => { /* route to children */ }
    }
}
```

2. **Method List** - `Activation::methods()`:
```rust
// In codegen/activation.rs:245
fn methods(&self) -> Vec<&str> {
    vec![#(#method_names,)* "schema"]  // ← "schema" added to list
}
```

This made `schema` accessible via the dynamic routing in substrate's main.rs:
```rust
let route_fn = Arc::new(move |method, params| {
    Box::pin(async move { hub.route(&method, params).await })
});
```

### What the New Macro Does (`#[activation]` in plexus-derive)

The `#[activation]` macro **only generates JSON-RPC endpoints for user-defined methods**:

```rust
// In codegen/plexus_jsonrpc.rs:48-105
fn generate_rpc_trait_methods(ast: &ActivationAST, item_impl: &ItemImpl)
    -> syn::Result<Vec<TokenStream>>
{
    let methods = ast.methods.iter().map(|m| {  // ← Only user methods!
        let method_name = syn::Ident::new(m.name.as_str(), ...);
        quote! {
            #[subscription(name = #method_name_str, ...)]
            async fn #method_name(&self, ...) -> SubscriptionResult;
        }
    }).collect();
    Ok(methods)
}
```

The `schema` method exists in the Activation trait but has **no corresponding RPC endpoint**.

When registered with jsonrpsee:
```rust
// substrate logs show only:
[TRACE]   - method: bash_execute
[TRACE]   - method: bash_unsubscribe_execute
// NO bash_schema!
```

## Evidence

### Test Setup
- **Old Substrate**: Commit 0a35dbee, plexus-macros v0.3.10, port 4445
- **New Substrate**: Commit fc624871, plexus-derive v0.1.0, port 4444
- **Test Command**: `synapse -P <port> substrate bash schema`

### Results

**Old substrate (hub_methods):**
```yaml
description: Execute bash commands and stream output
hash: 2de7d5478ddd4205
methods:
  - name: execute
    hash: 3c23da789c6f1b06
    params: { ... }
    returns: { ... }
  - name: schema
    hash: auto_schema
namespace: bash
version: 1.0.0
```

**New substrate (#[activation]):**
```
Fetch error at bash: Protocol error: Subscription error:
  RpcErrorObj {errCode = -32603, errMessage = "Internal error", errData = Nothing}
```

### Substrate Logs Comparison

**Old substrate** - registered methods:
```
[TRACE] bash.into_rpc_methods() converted to Methods with 2 methods
[TRACE]   - method: bash_execute
[TRACE]   - method: bash_unsubscribe_execute
```

**New substrate** - registered methods:
```
[TRACE] bash.into_rpc_methods() converted to Methods with 2 methods
[TRACE]   - method: bash_execute
[TRACE]   - method: bash_unsubscribe_execute
```

Both show only 2 methods! But the old version works because it uses **dynamic routing** via `hub.route()` which calls `Activation::call("schema", ...)`.

The new version has no such fallback - it relies purely on jsonrpsee RPC methods.

## Impact

### Immediate Failures
1. ❌ `synapse substrate <namespace> schema` fails for all migrated activations
2. ❌ synapse-cc cannot generate TypeScript clients (missing type schemas)
3. ❌ Runtime schema introspection broken
4. ❌ Method discovery broken for clients

### Affected Activations
All 8 migrated activations:
- bash, claudecode, claudecode_loopback, changelog, orcha, cone, arbor, lattice

All activations still using hub_methods continue to work:
- chaos, health, interactive, mustache, registry, solar (and nested children)

### Migration Blocker
Cannot complete hub_methods → #[activation] migration until this is fixed. The macro must achieve feature parity with hub_methods.

## Solution Design

### Option 1: Generate Schema RPC Method (Recommended)

Modify `plexus-derive/src/codegen/plexus_jsonrpc.rs` to auto-generate a `schema` subscription:

```rust
fn generate_rpc_trait_methods(ast: &ActivationAST, item_impl: &ItemImpl)
    -> syn::Result<Vec<TokenStream>>
{
    let mut methods = vec![];

    // User-defined methods
    for m in ast.methods.iter() {
        methods.push(quote! {
            #[subscription(name = #method_name_str, ...)]
            async fn #method_name(&self, ...) -> SubscriptionResult;
        });
    }

    // Auto-generate schema method
    methods.push(quote! {
        #[doc = "Get plugin or method schema"]
        #[subscription(name = "schema", unsubscribe = "unsubscribe_schema", item = serde_json::Value)]
        async fn schema(
            &self,
            method: Option<String>
        ) -> jsonrpsee::core::SubscriptionResult;
    });

    Ok(methods)
}
```

And implement it:
```rust
fn generate_rpc_impl_methods(...) -> Vec<TokenStream> {
    // ... existing user methods ...

    // Schema method implementation
    methods.push(quote! {
        async fn schema(
            &self,
            subscription_sink: jsonrpsee::PendingSubscriptionSink,
            method: Option<String>,
        ) -> jsonrpsee::core::SubscriptionResult {
            let sink = subscription_sink.accept().await?;
            let plugin_schema = self.plugin_schema();

            let result = if let Some(name) = method {
                plugin_schema.methods.iter()
                    .find(|m| m.name == name)
                    .map(|m| plexus_core::plexus::SchemaResult::Method(m.clone()))
                    .ok_or_else(|| /* error */)
            } else {
                Ok(plexus_core::plexus::SchemaResult::Plugin(plugin_schema))
            };

            let stream = futures::stream::once(async move { result });
            let wrapped = plexus_core::plexus::wrap_stream(stream, ...);

            tokio::spawn(async move {
                // Stream to sink...
            });

            Ok(())
        }
    });

    Ok(methods)
}
```

**Pros:**
- Direct RPC endpoint, no routing needed
- Consistent with how other methods work
- Easy to discover via jsonrpsee method listing

**Cons:**
- Adds code to every activation
- Slightly increases binary size

### Option 2: Add Dynamic Routing Fallback

Keep schema in Activation::call() only, but add substrate-level routing for missing RPC methods.

**Pros:**
- No macro changes needed
- Centralized schema handling

**Cons:**
- Inconsistent with other methods
- Requires substrate infrastructure changes
- Harder to discover

### Recommendation

**Use Option 1** - Generate the schema RPC method. This achieves true feature parity with hub_methods and makes schema endpoints first-class RPC methods.

## Implementation Checklist

- [ ] Update `plexus-derive/src/codegen/plexus_jsonrpc.rs` to generate schema method
- [ ] Add schema method to RPC trait definition
- [ ] Add schema method implementation
- [ ] Handle `method: Option<String>` parameter for specific method schemas
- [ ] Return `SchemaResult::Plugin` or `SchemaResult::Method`
- [ ] Test with synapse against all migrated activations
- [ ] Verify synapse-cc can generate TypeScript clients
- [ ] Compare generated IR with old substrate (should be identical)
- [ ] Update plexus-derive version and changelog

## Testing

### Manual Test
```bash
# Start substrate with migrated activation
plexus-substrate --port 4444

# Test schema endpoint
synapse -P 4444 substrate bash schema

# Should return:
# - namespace, version, description
# - methods array with execute + schema
# - full JSON schemas for params/returns
```

### Integration Test
```bash
# Generate TypeScript client
cd /tmp/test-client
cat > synapse.config.json <<EOF
{
  "schema": "1.0",
  "language": "typescript",
  "backend": "substrate",
  "url": "ws://127.0.0.1:4444",
  "targets": {
    "client": {
      "generate": ["transport", "rpc", "plugins"],
      "outputDir": "src/plexus"
    }
  }
}
EOF

synapse-cc build

# Should generate:
# - src/plexus/plugins/bash.ts with BashEvent types
# - No missing type errors
```

### Validation
```bash
# Compare old vs new substrate IRs
synapse-cc -P 4445 # old substrate
synapse-cc -P 4444 # new substrate

# Diff the generated ir.json files
diff <(jq 'del(.irHash, .irMetadata)' old/ir.json) \
     <(jq 'del(.irHash, .irMetadata)' new/ir.json)

# Should be identical
```

## Related Issues

- Migration of 8 activations to #[activation] (completed)
- synapse-cc TypeScript client generation
- Runtime schema introspection system
- Method discovery for dynamic clients

## References

- Old macro: `/workspace/hypermemetic/plexus-macros/src/codegen/activation.rs:264-291`
- New macro: `/workspace/hypermemetic/plexus-derive/src/codegen/plexus_jsonrpc.rs`
- Test substrate (old): commit 0a35dbee
- Test substrate (new): commit fc624871
- Comparison report: `/tmp/ir-comparison-report.md`

## Appendix: Hashes from Working Old Substrate

For validation, the old substrate returns these hashes:

```
substrate self_hash: 659af78566893706
bash plugin hash:    2de7d5478ddd4205
bash.execute hash:   3c23da789c6f1b06
```

After fixing the schema endpoint, the new substrate should generate matching hashes (except for the self_hash which reflects overall changes).
