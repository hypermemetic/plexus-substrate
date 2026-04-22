---
id: PROT-9
title: "downstream audit: bump plexus-core pins to 0.6 across sibling workspace crates"
status: Pending
type: implementation
blocked_by: [PROT-2, PROT-3, PROT-4]
unlocks: [PROT-10]
severity: Medium
target_repo: multiple
---

## Problem

Every workspace crate that pins `plexus-core`, `plexus-macros`, or `plexus-transport` at 0.5.x hits the dual-version-in-graph failure mode once PROT-2/3/4 ship 0.6.x. HF-0 hit this exact scenario with hyperforge; the version-bump memory (`feedback_version_bumps_as_you_go.md`) codified the audit sweep discipline. PROT-9 applies the sweep for the 0.5 → 0.6 rollover.

Known sibling drift (from earlier surveys):
- plexus-locus: pinned plexus-core 0.3 pre-PROT; 16 real migration errors flagged in HF-AUDIT-2 (not just a pin bump).
- mono-provider, plexus-music-royalty-free, plexus-mono: pinned plexus-core 0.4 per HF-AUDIT-1.
- axon, fidget-spinner, gitvm, hub-codegen, jsexec, plexus-listen, plexus-ir, plexus-rust-codegen, plexus-schemars-compat, plexus-comms, plexus-registry, plexus-derive: various 0.4-0.5 pins per the initial survey.

## Context

This ticket audits every Rust sibling in the workspace, bumps pins where the fix is cheap, and files audit tickets where the fix requires source-level migration (HF-AUDIT-2-style).

## Required behavior

1. **Grep** every `Cargo.toml` under `/Users/shmendez/dev/controlflow/hypermemetic/` for `plexus-core`, `plexus-macros`, `plexus-transport` deps. Build a table: repo / current pin / target pin / estimated difficulty.

2. **Cheap bumps** (pin-only, no source changes): update the Cargo.toml pin, run `cargo build`, verify green. Commit. Patch-bump the sibling's own version. Tag.

3. **Expensive bumps** (source migration needed): file a follow-up ticket per repo in `plans/HF-AUDIT/HF-AUDIT-N.md`. Do NOT attempt the migration in this ticket. Examples (known already):
   - plexus-locus (HF-AUDIT-2 — already filed).
   - Any other discovered during the sweep.

4. **Report** — commit body enumerates:
   - Cheap bumps applied (with commit SHAs and version bumps per crate).
   - Expensive bumps deferred (with ticket IDs).
   - Total delta: N cheap, M deferred.

5. **No-op crates**: if a crate's current pin is `*` or already accepts 0.6 ranges, note it but don't bump the crate's own version.

## Risks

| Risk | Mitigation |
|---|---|
| A "cheap" bump reveals a source-level incompatibility once you actually cargo build. | Demote to "expensive", file audit ticket, move on. |
| Bumping N crates in a single commit is hard to review. | One commit per repo (N commits). Each repo is self-contained. |
| A sibling crate has a `rustflags = ["-D", "dead-code"]` or similar strict lint that trips on macro-generated code from the bumped plexus-macros. | Same as plexus-locus's symptom. Either narrow the rustflags or defer the migration. |
| HF-AUDIT-1 (mono-provider family) and HF-AUDIT-2 (plexus-locus) are already Pending; don't duplicate. | This ticket's report references those by ID rather than filing new ones. |

## What must NOT change

- Any sibling's source code beyond Cargo.toml pins (for cheap bumps).
- Any sibling's public API.
- Any sibling's tagging convention (each uses their own `<crate>-v<version>` pattern).

## Acceptance criteria

1. Every Rust sibling's `Cargo.toml` either (a) pins plexus-* at 0.6+ with a green build, or (b) has a filed HF-AUDIT-N ticket explaining why not.
2. `cargo tree -d` at each bumped sibling shows a single version of each plexus-* crate.
3. Commit body summarizes: N cheap bumps applied, M deferred with ticket IDs, total siblings surveyed.
4. No sibling's build status regresses from whatever it was pre-PROT-9.

## Completion

This ticket is purely tracking — the actual work is per-repo commits. Status flipped to Complete when all siblings are either (a) bumped and green, or (b) have their migration tracked in an audit ticket.
