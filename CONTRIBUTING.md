# Contributing

Thanks for helping. Fork the repo, make your change on a branch in your fork,
and open a pull request against `main`. Nobody works on `main` directly: all
work happens on a feature branch or your own branch (e.g. `alex-dev`), and
reaches `main` only through a reviewed PR. Direct pushes to `main` are blocked.

## Setup

Follow [README.md, "Setup"](README.md#setup). You need the Basler pylon SDK to
build the `camera` and `app` crates and to run their tests. `metrology-core`
builds and tests without it, so you can work on measurement code with no camera.

Once per clone, turn on the repo's git hooks:

```
git config core.hooksPath .githooks
```

## Before you open a PR

Run these and make sure they pass:

```
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

CI runs fmt, clippy and the `metrology-core` tests on every PR. CI has no
pylon SDK, so it can't run the `camera` and `app` tests. If you change either
crate, run the full test suite locally and say so in the PR.

## What a good PR looks like

- **One change per PR.** A bug fix doesn't need nearby cleanup in the same PR.
- **Tests come with the change.** Each new behavior or fix needs a test that
  would fail without it. Assert real values, not just that something exists.
- **Commit messages say what changed and why.** "Fix edge refinement bias at
  low sigma" is useful; "fix" or "update" isn't.
- **No AI attribution in commits.** Remove `Co-Authored-By` trailers naming
  an AI tool, "Generated with" lines and 🤖. The `commit-msg` hook and CI both
  reject them. Using AI tools is fine; `AGENTS.md` holds the rules they need.
- **No secrets.** Don't commit keys, tokens or passwords anywhere.
- **Don't commit reference material.** Vendor PDFs, SDKs, videos and
  third-party code stay in the gitignored `reference/` folder (see
  `reference/README.md`).
- **Hardware changes.** If you change a part in `hardware/`, include both the
  STEP and STL files and say what you changed and why. If you've printed and
  tested it, say that too.

## License

By contributing, you agree that your contributions are licensed under the
[MIT License](LICENSE).
