# Substrate Wire Protocol & Security Reference

**Date:** 2026-05-02
**Audience:** Engineers building clients against substrate (gamma, synapse-cc-generated TS/Rust clients, agents using `synapse <backend>`).
**Scope:** Documents what substrate actually puts on the wire, what auth it enforces today, and which security posture decisions are deferred to the deployment environment. This is a *reference* — for structural critique see [`16670380887168786687_substrate-technical-debt-audit.md`](./16670380887168786687_substrate-technical-debt-audit.md).

---

## TL;DR

| Question | Answer |
|---|---|
| Transport | WebSocket on `:4444`, MCP HTTP on `:4444/mcp`, optional `--stdio` |
| Wire framing | JSON-RPC 2.0 — one notification per stream item, no batching |
| Synapse `--json` framing | **NDJSON** — `LBS.putStrLn $ encode item` per item, line-flushed (`synapse/app/Main.hs:1016`) |
| Auth (today) | Optional admission key via `--api-key` / `PLEXUS_API_KEY`, sent on `X-Plexus-API-Key`. SessionValidator (when wired) consumes Cookie OR `Authorization: Bearer`. **No login activation, no built-in JWT issuer** |
| Origin allowlist on WS upgrade | **Not enforced** by substrate. CORS handled generically by `tower-http` |
| Rate limiting / connection caps | **None**. Unbounded streams, unbounded subscriptions, no idle timeout |
| Multi-tenancy | **Not implemented**. Activations may accept `owner_id` params but it's application-level, not framework-enforced |
| Bidir reference activation | `interactive` (`src/activations/interactive/activation.rs`) — wizard / select / confirm / delete |
| `_info` exposure | Open by default — full schema readable without auth |

If you are taking substrate to public internet, read the ["Security posture"](#security-posture) section before exposing port 4444.

---

## What substrate is

`plexus-substrate` is a Plexus RPC server that registers ~17 activations under one `DynamicHub` and serves them on three transport surfaces simultaneously:

- WebSocket at `ws://127.0.0.1:4444` (primary)
- MCP HTTP at `http://127.0.0.1:4444/mcp` (Model Context Protocol)
- stdio JSON-RPC when launched with `--stdio`

The hub routing layer is `plexus-core::DynamicHub`; the per-transport adapters live in `plexus-transport`. Substrate itself owns the activation registrations (`builder.rs:145-175`) and a thin `main.rs` for argument parsing and lifecycle.

A representative invocation:

```text
synapse substrate health.status
```

routes through:

```
Client (synapse CLI)
  → WS upgrade, JSON-RPC envelope
  → DynamicHub.route()
  → Health::call()
  → Stream of PlexusStreamItem ── JSON-RPC notifications ──>
  → Client renders / encodes
```

---

## Wire protocol

### Method invocation envelope

Every method call uses standard JSON-RPC 2.0 over the chosen transport. The hub's outer call is `<backend>.call`, with the method path and params nested inside:

```json
{
  "jsonrpc": "2.0",
  "id": 7,
  "method": "substrate.call",
  "params": {
    "method": "echo.once",
    "params": { "message": "hi", "count": 3 }
  }
}
```

The server immediately responds with a subscription handle (numeric or UUID — random per request):

```json
{ "jsonrpc": "2.0", "id": 7, "result": { "subscription": "<sub-id>" } }
```

Subsequent stream items arrive as **notifications** (no `id`), each carrying the subscription identifier:

```json
{
  "jsonrpc": "2.0",
  "method": "subscription",
  "params": {
    "subscription": "<sub-id>",
    "result": { /* PlexusStreamItem */ }
  }
}
```

**One stream item per notification. No batching.** A stream of N items yields N notifications.

### `PlexusStreamItem` variants

The `result` field above is one of:

```json
// Data (the most common)
{ "type": "data",
  "content": { /* domain payload */ },
  "content_type": "application/json" }

// Progress (incremental status)
{ "type": "progress",
  "message": "step 3 of 10",
  "fraction": 0.3 }

// Error (recoverable or fatal — see flag)
{ "type": "error",
  "message": "...",
  "recoverable": false }

// Request (bidirectional — server asks client for input mid-stream)
{ "type": "request",
  "request_id": "<uuid>",
  "request_data": { "type": "confirm", "message": "Drop the table?", "default": false },
  "timeout_ms": 30000 }

// Done (terminal — always last)
{ "type": "done",
  "metadata": { "path": ["health"], "context_hash": "..." } }
```

Domain types (e.g. `EchoEvent`, `WizardEvent`) are wrapped into `PlexusStreamItem::Data` at the dispatch boundary by `wrap_stream(...)` (see `health/activation.rs:70-98` for the canonical pattern). The activation's domain code never constructs `PlexusStreamItem` directly.

### Termination

`Done` is the only terminal item. `Error { recoverable: false }` may be followed by `Done` to close the stream cleanly, but is not always — clients should treat fatal `Error` as terminal and tolerate a missing `Done` in that case.

### Synapse `--json` output framing

When a client calls a method via `synapse <backend> <method> --json`, the CLI serializes each `PlexusStreamItem` it receives and writes it to stdout as **one line per item**:

```haskell
-- synapse/app/Main.hs:1016
printResult True _ _ item = LBS.putStrLn $ encode item
```

`LBS.putStrLn` emits `<encoded-json>\n` per item — i.e. **NDJSON**. Agents can consume with line-buffered reads:

```bash
synapse substrate echo.once --message hi --count 3 --json | while read -r line; do
    echo "got: $line"
done
```

Each `line` is one complete JSON object representing one `PlexusStreamItem`.

Without `--json`, synapse renders via Mustache templates (or pretty-printed JSON fallback) — that output is for humans, not agents.

---

## Authentication

### Header layout (AUTHZ-BEARER-1)

Substrate's authentication surface separates two unrelated concerns onto two unrelated headers, per [AUTHZ-BEARER-1](../../../plans/AUTHZ/AUTHZ-BEARER-1.md):

| Header | Carries | Consumed by | Configurable? |
|---|---|---|---|
| `X-Plexus-API-Key` (default) | The static deployment-admission key (the value of `--api-key` / `PLEXUS_API_KEY`) | The transport's static admission gate (layer 0); never reaches `SessionValidator` | Yes — `TransportServerBuilder::with_api_key_header(HeaderName)` overrides the name |
| `Authorization: Bearer <token>` | A user-identity token (JWT or opaque session token) | `SessionValidator::validate(token)` when one is wired (when not, ignored except for the deprecated v1 compat shim) | No — fixed by HTTP convention |
| `Cookie: <...>` | Browser-issued session credentials (e.g. `access_token=<jwt>`) | `SessionValidator::validate(cookie_value)` when one is wired | No — fixed by HTTP convention |

The two layers compose: when `api_key` is set, the static admission gate fires *first* (independent of `SessionValidator`). After it passes (or when `api_key` is unset), `SessionValidator` (when wired) tries the `Cookie` input first; on no-`Some`, it falls back to `Authorization: Bearer`. Cookie wins when both inputs produce a result.

### What substrate enforces today

A single optional **admission key** via the `--api-key` flag or `PLEXUS_API_KEY` environment variable (`main.rs:26-30`). When set, the same key is required on:

- WebSocket upgrade — `X-Plexus-API-Key: <key>` (per AUTHZ-BEARER-1)
- MCP HTTP requests — `Authorization: Bearer <key>` *(temporary inconsistency: the MCP and REST gateways have not yet been aligned to the new header layout; a follow-up ticket addresses this)*
- stdio (no-op — local pipe)

When unset, substrate logs a warning and accepts unauthenticated connections:

```
WARN  Authentication: DISABLED — set --api-key or PLEXUS_API_KEY to require an admission key (sent on the X-Plexus-API-Key header)
```

When set:

```
INFO  Authentication: static admission key configured (header: X-Plexus-API-Key); Authorization: Bearer is reserved for SessionValidator (AUTHZ-BEARER-1)
```

### v1 compat shim (deprecated, single release window)

For deployments that may have wired `Authorization: Bearer <api_key>` against the pre-AUTHZ-BEARER-1 transport, the WebSocket middleware accepts that wire shape one more time when:

- `api_key` is configured, AND
- the configured `X-Plexus-API-Key` header is absent, AND
- `Authorization: Bearer <value>` carries a value matching the configured `api_key` exactly, AND
- no `SessionValidator` is wired.

A `tracing::warn!("deprecated: Bearer-as-api-key compatibility shim fired; migrate to X-Plexus-API-Key header")` is emitted on every fire so operators see the deprecation. The compat shim is OFF whenever `SessionValidator` is configured to prevent header-conflation re-entry. AUTHZ-BEARER-2 (separate, future ticket) removes the compat shim after one release.

### What substrate does *not* enforce today

- **No login activation, no built-in JWT issuer.** Substrate consumes identity tokens via `SessionValidator` but does not mint them. An external IdP (Keycloak, Auth0, your own) issues the tokens; an activation crate supplies the `SessionValidator` impl.
- **No `SessionValidator` is wired by default.** `SqliteSessionManager` exists in the codebase (`lib.rs:16`) but it is a session *store* for MCP, not an authenticator. Backends (e.g. plexus-trak via `TrakAuth`) wire their own validator.
- **No Origin allowlist on WS upgrade.** Cross-origin browser clients are accepted unconditionally. The `ValidOrigin` extractor exists in `plexus-transport` but substrate's `main.rs` does not configure it.
- **No `from_cookie` / `from_header` / `from_auth_context` extractors used in any activation.** Auth context is not threaded into business logic today.

### Practical implication

If you are exposing substrate publicly, the admission key is your *only* line of defense from the substrate codebase itself. CSRF protection, origin validation, rate limiting, and per-user identity must be supplied by deployment infrastructure (reverse proxy, gateway, IdP) and a backend-supplied `SessionValidator`.

---

## Bidirectional methods

### How they work

A method declared with `#[plexus_macros::method(bidirectional, streaming)]` receives a `ctx: &Arc<StandardBidirChannel>` parameter (injected by the macro). The activation calls `ctx.confirm(...)`, `ctx.prompt(...)`, `ctx.select(...)` mid-execution; each call emits a `PlexusStreamItem::Request` to the client and awaits a matching response before continuing.

### Canonical examples

`src/activations/interactive/activation.rs`:

| Method | Bidir interactions | Use case |
|---|---|---|
| `interactive.wizard` | `ctx.prompt()` → name; `ctx.select()` → template; `ctx.confirm()` → final yes/no | Multi-step setup |
| `interactive.delete` | `ctx.confirm()` → "Delete \<paths\>?" | Destructive confirmation |
| `interactive.confirm` | single `ctx.confirm()` | Simple gate |

### Standard request shapes

`StandardBidirChannel` defines a fixed vocabulary the server emits to the client (from `tests/bidirectional_integration.rs:16-20`):

```json
// Confirm
{ "type": "confirm", "message": "Are you sure?", "default": false }

// Prompt
{ "type": "prompt", "message": "Enter project name:" }

// Select
{ "type": "select", "message": "Pick template:", "options": ["a", "b", "c"] }
```

The client must respond by calling back into a method (typically `<backend>.respond` or per-channel equivalent) with the matching `request_id` and a typed payload. Activations that detect non-bidir transports yield `Error { message: "Interactive mode required" }` and finish without prompting (`interactive/activation.rs:49-128`).

### What client UIs need to handle

A generic bidir UI (e.g., for gamma) must:

1. Listen for `PlexusStreamItem::Request` notifications on any subscription
2. Render `request_data` as a form/dialog using its `type` discriminator
3. Send the response back, threading the `request_id`
4. Resume waiting for further stream items
5. Honor `timeout_ms` — drop the prompt if the user doesn't answer in time

Forms need not be rich: confirm/prompt/select cover almost all current usage.

---

## Children & dynamic routing

### Hub activations

The `solar` activation is the canonical hierarchical example (`src/activations/solar/`):

```
solar
├── mercury
├── venus
├── earth
│   └── moon
├── jupiter
│   ├── io
│   ├── europa
│   ├── ganymede
│   └── callisto
├── ...
```

Method calls like `substrate.solar.jupiter.io.info()` route through nested `ChildRouter` impls generated by the activation macro. `DynamicHub::plugin_children()` exposes the structure for discovery.

### `raw_ctx` propagation

`ChildRouter::router_call` accepts `raw_ctx: Option<&RawRequestContext>` (the post-2026-Q1 signature change in plexus-core). Substrate's activations do *not* currently use `raw_ctx` for anything — it is propagated by the macro and ignored downstream. See `src/activations/solar/celestial.rs` for the signature.

This is the lever that future auth-context-aware routing will pull on. As of today, no activation reads from it.

---

## Security posture

### Current state (development-grade)

| Concern | State |
|---|---|
| Bearer token | Optional, off by default |
| Origin validation | Not enforced |
| Cookie / session auth | Not implemented |
| `_info` schema gating | Open — anyone who can connect can enumerate every method |
| Rate limiting | None |
| Connection caps | None |
| Stream backpressure | None — `DynamicHub` uses unbounded channels (see audit) |
| Subscription ID predictability | Random; not reviewed for cryptographic strength |
| Multi-tenancy | Not enforced. `owner_id` params exist on some activations but a malicious client can pass any value |
| Audit logging | None |
| Handle integrity | Handles are not cryptographically signed; clients can forge them |
| TLS | Not configured by substrate; expected from deployment |

### What public hosting requires

Before exposing substrate on the public internet, at minimum:

1. **Set `PLEXUS_API_KEY`.** Reject unauthenticated connections.
2. **Deploy behind a reverse proxy** (nginx, Caddy, Cloudflare) that provides:
   - TLS termination
   - Rate limiting (per-IP, per-token)
   - Connection caps and idle timeouts
   - Origin validation (until substrate enforces it natively)
   - Maximum WS message size limits
3. **Strip any client-supplied `from_header` values** that activations rely on for trust. Set them authoritatively at the proxy.
4. **Audit each enabled activation for `owner_id`-style fields.** If you have multiple users, every `WHERE owner_id = ?` clause is application-enforced and must be covered by tests.
5. **Disable activations you do not need.** Each one is attack surface (`builder.rs:145-175`).

### Known gaps (operational)

From observed `TODO` comments and audit findings:

- `orcha/orchestrator.rs` and `orcha/activation.rs` hardcode `/workspace` and `auto_approve: true` — these need session-bound substitutes before multi-user deployment
- `claudecode_loopback` has a hardcoded 300s timeout with no override
- `cone/activation.rs` has an incomplete bash integration TODO
- No subscription cleanup on client disconnect — `ClaudeCodeStorage.streams` leaks across reconnects (per audit)

For the full structural picture see [`16670380887168786687_substrate-technical-debt-audit.md`](./16670380887168786687_substrate-technical-debt-audit.md).

---

## Versioning of the wire protocol

`PlexusStreamItem` is serialized without an explicit version tag. Adding a new variant is silently breaking for older clients — there is no schema-negotiation handshake on connection. Plan client deployments to track substrate versions, or wait for a versioning primitive to land (currently absent).

---

## Quick reference — the JSON shapes a client must understand

Minimum a client must parse to be useful:

- The `subscription` response to a method call (extract the id)
- `subscription` notifications matching that id
- The `result` envelope's `type` discriminator (`data` / `progress` / `error` / `request` / `done`)
- For `data`: the `content` payload's domain shape (varies per method)
- For `request`: the `request_data.type` discriminator (`confirm` / `prompt` / `select`)
- For `error`: the `recoverable` flag

Anything else is optional convenience.
