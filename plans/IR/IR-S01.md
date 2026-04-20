---
id: IR-S01
title: "Spike: synapse deprecation rendering viability"
status: Pending
type: spike
blocked_by: []
unlocks: [IR-6]
severity: High
target_repo: synapse
---

## Question

Can synapse render a per-method deprecation marker next to its name without rewriting its tree-rendering pipeline?

## Setup

1. In a fork/branch of synapse (`/Users/shmendez/dev/controlflow/hypermemetic/synapse/`), add a minimal optional `deprecation` field (tagged union: `None` / `Some { since: String, removed_in: String, message: String }`) to the method schema type synapse consumes.
2. Create a synthetic fixture schema in the synapse test corpus — one activation with one method, that method's `deprecation` is `Some {since: "0.5", removed_in: "0.7", message: "use move_doc"}`.
3. Patch synapse's rendering so that when a method entry has `deprecation = Some(...)`, the method's tree node renders with a `⚠` prefix and the deprecation message is appended to the help text.
4. Run synapse against the fixture schema (no real substrate required — mock or static fixture).

## Pass condition

Running `synapse <fixture>` shows the deprecated method with exactly:
- A `⚠` prefix (or other visibly distinct marker) on the method's line in the tree listing.
- The exact substring `DEPRECATED since 0.5, removed in 0.7 — use move_doc` in the method's help text.

Binary: both markers present → PASS. Either missing → FAIL.

## Fail → next

The patch required rewriting synapse's core rendering pipeline (not an incremental hook). IR-6's scope grows — treat as structurally non-trivial, budget accordingly. Open a replanning trigger on IR-6 to account for the larger change.

## Fail → fallback

If the rendering pipeline genuinely resists incremental extension, a fallback: surface deprecation via a separate `synapse --warnings` subcommand that lists all deprecated surfaces it encounters, rather than inline in the tree. Worse UX but ships.

## Time budget

Two focused hours. If the spike exceeds this, stop and report regardless of pass/fail state — the budget overrun itself is signal.

## Out of scope

- Visual design beyond the marker.
- Source of the `deprecation` field in a real substrate schema (handled by IR-2/IR-4/IR-5).
- Any synapse changes unrelated to rendering deprecation.

## Completion

Spike delivers: a single commit to a throwaway branch in synapse with the patch and fixture, pass/fail result, time spent, and one-paragraph description of what the patch touched. No merge to main. Report lands in IR-6's Context section as a reference before IR-6 is promoted to Ready.
