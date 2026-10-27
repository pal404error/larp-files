# Contributing to .larp

## Quick start

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Every change needs a test, and clippy + fmt must be clean — CI enforces
both, so run them before pushing.

## Rules that matter

- The on-disk format is frozen in `docs/FORMAT.md`. Do not change it
  without updating the spec and bumping the format version.
- Never silently rewrite memory records. Corrections supersede; history is
  preserved.
- Keep the core dependency-light. A new crate needs justification.
- Benchmarks live in `examples/bench.rs`. If you touch the hot path
  (append, search, recall), run it and report the numbers in your PR.

## Good first issues

- Recall ranking experiments (better decay curves, type weighting).
- The shared-dictionary / block-compression follow-ups in
  `docs/BENCHMARKS.md`.
- More MCP client setup guides in the README.
- Fuzzing with cargo-fuzz on top of the fuzz-lite tests.

## Release process

Maintainers: bump the version in `Cargo.toml`, update `CHANGELOG.md`, tag
`vX.Y.Z`, push the tag — the Release workflow builds binaries and creates
the GitHub release automatically.
