---
id: IR-S02
title: "Spike: synapse-cc codegen extensibility"
status: Pending
type: spike
blocked_by: []
unlocks: [IR-7, IR-9]
severity: High
target_repo: synapse-cc
---

## Question

Can synapse-cc emit a target-language comment above a generated method based on a schema field, for at least one supported target language, without restructuring its codegen core?

## Setup

1. In a fork/branch of synapse-cc (`/Users/shmendez/dev/controlflow/hypermemetic/synapse-cc/`), identify the codegen entry point and the per-target-language emission paths.
2. Pick the target language with the simplest codegen path (whichever that turns out to be).
3. Add a test-only `deprecated: bool` flag on one method's schema in a fixture input.
4. Extend codegen for the chosen language to emit a single-line comment `// SPIKE-DEPRECATED` immediately above the generated method when `deprecated = true`.
5. Run synapse-cc against the fixture; inspect the generated output file.

## Pass condition

`grep -c "// SPIKE-DEPRECATED" <generated-file>` returns exactly 1, located immediately above the method's declaration.

Binary: marker present in correct position → PASS. Missing or in wrong position → FAIL.

## Fail → next

synapse-cc's codegen is not structurally extensible via simple per-field hooks. Open a design ticket (IR-7 would depend on it) for a codegen-hook architecture; IR-9's typed-handle work would share the same prerequisite.

## Fail → fallback

If extensibility requires structural work but a simpler "generate then post-process" approach is viable (run a second pass over generated files to inject comments based on a sidecar schema index), pin that as the implementation path for IR-7. IR-9 still likely needs structural work because typed-handle emission isn't post-processable.

## Time budget

Two focused hours. Over budget → stop and report.

## Out of scope

- Multi-backend validation (that's IR-7's acceptance scope).
- Native language deprecation markers (`@deprecated` JSDoc / `#[deprecated]` Rust). The spike only needs a raw comment.
- Any CLI flag / stderr behavior (IR-7 owns).

## Completion

Spike delivers: one commit to a throwaway branch of synapse-cc with the fixture + patch, pass/fail result, identification of which target language was chosen and why, and one-paragraph description of synapse-cc's codegen architecture as discovered during the spike. Report lands in IR-7 and IR-9's Context sections before either is promoted to Ready.
