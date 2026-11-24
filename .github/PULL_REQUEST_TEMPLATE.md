<!-- Thank you for contributing to .larp! -->

## What this changes

<!-- One or two sentences. Link the issue it closes, if any: "Closes #N". -->

## Checklist

- [ ] `cargo test` passes (`cargo test --locked --all-targets`)
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] `cargo fmt --check` is clean
- [ ] New behavior has a test (see `CONTRIBUTING.md`: no test, no merge)
- [ ] If the hot path changed (append, search, recall): `examples/bench.rs`
      was run and the numbers are in the PR description
- [ ] If the on-disk format changed: `SPEC.md` / `docs/FORMAT.md` updated
      **and** the format version bumped (format changes are rare by design)

## Notes for reviewers

<!-- Anything surprising, any trade-off you made, anything you want a second
pair of eyes on. Delete this section if there is nothing to say. -->
