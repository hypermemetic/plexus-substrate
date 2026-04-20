---
id: CHILD-10
title: "Explicit plugin_children override as a child macro extension"
status: Pending
type: implementation
blocked_by: []
unlocks: []
severity: Medium
target_repo: plexus-macros
---

## Problem

CHILD-8 synthesizes `plugin_children()` from static `#[plexus_macros::child]` methods. When an author needs a different source (e.g., Solar computes summaries from runtime config), today's escape hatch is to hand-write `fn plugin_children(&self) -> Vec<ChildSummary>` on the impl and rely on the macro's "already defined" detection. That works but it leaks macro-contract knowledge: the author has to know that a free-standing method with this specific name silently overrides synthesis. There's no declarative signal to the macro, and a typo in the method name silently falls back to synthesis without warning.

The override should be explicit: a macro attribute that names the method (or provides the output directly), so the macro knows exactly what's happening and can validate signatures.

## Context

Current behavior (post-CHILD-8):
- If impl defines `fn plugin_children(&self) -> Vec<ChildSummary>` (exact name and signature) → macro skips synthesis, emits `self.plugin_children()` in the schema constructor.
- Otherwise → macro synthesizes from static `#[child]` methods.

The rename-typo risk is real: `fn plugin_childrn` (typo) silently falls back to synthesis with empty hashes, which passes type-check but produces wrong schemas at runtime.

## Required behavior

Introduce an explicit child-summary override. Three shapes supported:

**Shape 1: attribute arg on the activation attr pointing at a method.**

```rust
#[plexus_macros::activation(namespace = "solar", summaries = "body_summaries")]
impl Solar {
    fn body_summaries(&self) -> Vec<ChildSummary> { /* ... */ }
}
```

The macro looks up `body_summaries` in the same impl block, validates its signature (`fn(&self) -> Vec<ChildSummary>` or async variant), and emits `self.body_summaries()` as the summary source in `plugin_schema_body`.

**Shape 2: attribute on the method directly.**

```rust
#[plexus_macros::activation(namespace = "solar")]
impl Solar {
    #[plexus_macros::child_summaries]
    fn body_summaries(&self) -> Vec<ChildSummary> { /* ... */ }
}
```

Macro discovers the annotated method automatically. Exactly one `#[child_summaries]` method permitted per impl.

**Shape 3: inline argument on the `#[child]` attribute.**

```rust
#[plexus_macros::activation(namespace = "solar")]
impl Solar {
    #[plexus_macros::child(list = "body_names", summaries = "body_summaries")]
    async fn body(&self, name: &str) -> Option<CelestialBodyActivation> { /* ... */ }

    fn body_summaries(&self) -> Vec<ChildSummary> { /* ... */ }
}
```

Shape 3 is the most local (lives on the dynamic child method it describes). Prefer Shape 3 when the override is semantically tied to a specific dynamic `#[child]` gate. Fall back to Shape 1 or 2 when the override describes the whole activation's child set.

**Pin in this ticket: support all three shapes.** Each is a different authorial convenience; they don't conflict. Present all three in one fixture and they compose the same concrete `plugin_schema`.

Macro-expansion rules:

| Condition | Outcome |
|---|---|
| No `summaries` referenced, no hand-written `fn plugin_children` | Macro synthesizes from static `#[child]` methods (unchanged CHILD-8 behavior). |
| `summaries = "name"` on activation attr or `#[child(summaries = "name")]` | Macro validates the named method exists in the same impl with signature returning `Vec<ChildSummary>` (sync or async), emits `self.name()` as the summary source. Synthesis skipped. |
| `#[plexus_macros::child_summaries]` on a method | Discovered automatically; same effect as Shape 1. |
| Hand-written `fn plugin_children(&self) -> Vec<ChildSummary>` AND any of the explicit overrides | Compile error — forces the author to pick one canonical source. |
| Referenced method doesn't exist, or signature wrong | Clear compile error. |

Existing hand-written `fn plugin_children` continues to suppress synthesis (backward compat — all current Solar-style overrides keep working). A lint-level warning is emitted suggesting the explicit override shapes, but it does not error.

## Risks

| Risk | Mitigation |
|---|---|
| Three shapes is three things to document | Rustdoc on `#[plexus_macros::child]` enumerates them and the "when to use which" guidance. |
| Shape 3 (`summaries = "name"` on `#[child]`) creates the illusion that the referenced method is per-child instead of whole-impl | Doc: the referenced method produces the summaries for the ENTIRE impl, not just that `#[child]`. It's just placed near the child it most naturally describes. |
| The deprecation warning on hand-written `fn plugin_children` breaks existing test expectations | Ensure the warning is suppressible via `#[allow(...)]` and doesn't flip CI red. |

## What must NOT change

- Existing Solar-style `fn plugin_children(&self)` hand-written overrides continue to work (with a lint warning nudging toward the explicit form).
- CHILD-8's synthesis path continues to apply when no override is present.
- All CHILD-3 / CHILD-4 / CHILD-7 / CHILD-8 fixtures pass.
- Substrate `cargo build --workspace` passes with zero edits.

## Acceptance criteria

1. `cargo build -p plexus-macros` and `cargo test -p plexus-macros` succeed.
2. Fixtures for each of Shapes 1, 2, 3 produce identical generated `plugin_schema` given identical summary methods.
3. A fixture combining an explicit override with `fn plugin_children` fails to compile with a clear error.
4. A fixture referencing a nonexistent method (`summaries = "typo"`) fails to compile with a clear error.
5. A lint warning fires on activations still using the hand-written `fn plugin_children` pattern. Warning text suggests the explicit attribute.
6. CHILD-7's Solar (currently using hand-written `fn plugin_children`) continues to compile; the lint warning can be silenced or migrated to Shape 3.
7. Substrate workspace builds clean.

## Completion

PR against plexus-macros. CI green. Status flipped to Complete in the same commit. Optionally, a follow-up substrate PR migrates Solar from hand-written `fn plugin_children` to Shape 3 (`#[child(summaries = "body_summaries")]`) — out of scope for this ticket.
