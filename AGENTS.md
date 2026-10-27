# larp — agent instructions

## Project
Rust package: library crate `larp-memory` (lib target `larp_memory`) + CLI binary `larp`. Format spec is frozen in
SPEC.md — do NOT change the on-disk format without updating SPEC.md and
bumping the format version.

## Rules
- One phase at a time. Never start the next phase until the current phase's
  acceptance criteria pass.
- After every change: `cargo test` must pass, `cargo clippy` must be clean.
- Every new feature ships with a test. No test, no merge.
- Keep functions small and dependency-free where possible. No new crates
  without asking.
- Never silently rewrite memory records — corrections are new records that
  supersede old ones (see SPEC.md).
- If cargo insists a changed binary is fresh (stale fingerprints, seen
  after the package rename), `cargo clean -p larp-memory` and rebuild.
- When unsure about a design choice, stop and ask instead of guessing.

## Commands
- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy -- -D warnings`
- Bench: `cargo bench`
