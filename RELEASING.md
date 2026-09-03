# Releasing

Releases are cut by hand; the list below is the whole procedure.

1. **Land the changelog.** Every behaviour, API or output change should already
   be under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md) — added by the
   PR that made it, not reconstructed here. Confirm nothing since the last
   release is missing:

   ```sh
   git log --oneline v$PREV..HEAD
   ```

2. **Pick the version.** Translated XPath is part of the contract: a change to
   the expression a selector produces is a minor bump before 1.0 and a major one
   after, even when the new expression selects the same nodes. Pre-1.0, a
   breaking API change is a minor bump.

3. **Rename the section.** `## [Unreleased]` becomes `## [X.Y.Z] - YYYY-MM-DD`,
   with a fresh empty `## [Unreleased]` above it, and update the compare links
   at the bottom of the file.

4. **Check `version` in `Cargo.toml`.** The `semver` CI job compares the public
   API against the released baseline on every change, so a breaking change is
   normally bumped by the PR that makes it and the version is already right
   here. Bump it if it is not, then `cargo check` so `Cargo.lock` follows.

5. **Check the release.**

   ```sh
   cargo fmt --all --check
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all-features
   cargo semver-checks check-release        # cargo install cargo-semver-checks
   cargo deny check licenses bans sources   # cargo install cargo-deny
   cargo package --list                     # src/, Cargo.toml, LICENSE, README, CHANGELOG
   cargo publish --dry-run
   ```

6. **Commit, tag, push.**

   ```sh
   git commit -am "Release X.Y.Z"
   git tag -a vX.Y.Z -m "vX.Y.Z"
   git push origin master vX.Y.Z
   ```

7. **`cargo publish`,** then open a GitHub release for the tag with the
   changelog section as its body.
