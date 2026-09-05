# Releasing

Both crates share one version via `[workspace.package]` in the root
`Cargo.toml`. A release is one commit, one `vX.Y.Z` tag, one crates.io publish
per crate, and one GitHub release with binaries attached.

Each crate's `CHANGELOG.md` is generated from git history, so a commit's
subject line is what users will read. Commits that touch
`crates/busybar-anim` appear in that crate's changelog; commits that touch
anything else appear in `busyshow`'s.

## One-time setup

```console
cargo install cargo-release git-cliff
cargo login            # a crates.io token that can publish both crates
```

## Process

1. Dry run. This runs `just precommit` (fmt, check, clippy, test) first, then
   prints what the release would do, including both changelogs, without doing
   it:

   ```console
   just release patch          # or minor, major
   ```

2. Release for real:

   ```console
   just release patch --execute
   ```

3. Wait for the `publish-release` workflow on GitHub. Nothing to do by hand.
