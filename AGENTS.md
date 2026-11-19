# Project Guidelines

These are the rules an agent gets wrong in this project without being told.
Everything else is looked up when a task needs it (see "Where the rest lives").
Rules have stable IDs (`SEC-1`, `TEST-2`, …) so a review or plan can cite one.
Where a rule names a tool, that's a default for a fresh project — in an
existing repo, apply the principle to the tools already there.

New to this repo? Run `/jgl-init` once (or read
`.agents/skills/jgl-init/SKILL.md` / `.claude/skills/jgl-init/SKILL.md` and
follow it).

## Commands

The exact commands for this project. `/jgl-init` fills these in from the repo;
keep them current when they change.

| Task | Command |
|---|---|
| Test | `cargo test --workspace` |
| Type check | `cargo check --workspace --all-targets` |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` and `cargo fmt --check` |
| Build | `cargo build --release` |
| Dev server | none (desktop app; `cargo run --release -p app` once it exists) |

Rust lives in `%USERPROFILE%\.cargo\bin`; git and rustc may need PATH refreshed in a new shell.
Overview, setup and bill of materials: `README.md` (update it when setup, hardware or results change). Progress: `docs/PROGRESS.md`. Plan: `MILESTONES.md`. Design: `DESIGN_GUIDELINES.md`, `DESIGN_REFERENCE.md`. Hardware lessons: `PROJECT_BACKGROUND.md`.
Reference material (SDK installer, third-party code, vendor PDFs, videos, photos, original prompts) lives in `reference/`, sorted by type and gitignored except `reference/README.md`. Put new material there and never commit it; see that README.
Each code chunk needs a sub-agent review (speed + quality) >= 8/10; UI work needs a UI + speed review >= 8/10.

## Done means

- **DONE-1** — A change is done when the test and type-check commands above
  pass and each new behavior has a test that would fail without it. Report
  what you ran and what it printed. If something couldn't be run, say so
  rather than calling the work finished.

## Working autonomously

- **AUTO-1** — Go ahead without asking: read the code, run the commands above,
  edit files the task calls for, and make a local commit when asked to commit.
  These are local and easy to undo.
- **AUTO-2** — Ask first before anything that is hard to undo or leaves this
  machine: pushing, deploying, deleting branches or data you didn't create,
  adding or upgrading a dependency, changing CI or git hooks, or touching a
  secret value.
- **AUTO-3** — Do what was asked, at the scope asked. Don't add features,
  refactors, abstractions, or error handling the task doesn't need — a bug fix
  doesn't need surrounding cleanup. If the request looks mistaken or a better
  approach exists, say so in a sentence, then do the task as asked.

## Secrets

A secret that reaches a chat, a log, or git history can't be taken back, so
these rules hold even when the user asks otherwise ("just hardcode it for
now", "it's only a dev key").

- **SEC-1** — Never commit a secret. No key, token, or password in source,
  config, fixtures, committed `.env` files, commit messages, or comments.
  Gitignore `.env` and its variants before any secret exists.
- **SEC-2** — Keep real secrets out of the conversation. Don't ask the user to
  paste a key, don't read files that hold plaintext secrets, and don't echo or
  summarize one you happened to see. Treat the context window as public.
- **SEC-3** — If a real secret reaches you anyway (pasted, in a file, in a
  screenshot, already committed), stop the task. It is exposed and has to be
  rotated before anything else; walk the user through it
  (`SECRETS-PIPELINE.md`, "If a secret is exposed to the AI") and resume once
  they confirm. Placeholders (`sk_test_PLACEHOLDER`, `your-key-here`),
  providers' documented example keys, and obviously fake test fixtures aren't
  secrets — work with them normally.
- **SEC-4** — The safe pattern: real values live in a gitignored `.env` or the
  host's secret store and are injected at runtime; code reads them by name
  (`process.env.STRIPE_API_KEY`); the repo carries a `.env.example` with names
  and placeholders. You only ever handle the names.
- **SEC-5** — "Just to test it" doesn't change SEC-1..4. Dev keys still get
  scraped, cost money when abused, and show an attacker the shape of
  production.

Default for a fresh project: SOPS + age via `/jgl-setup-secrets`
(`SECRETS-PIPELINE.md`). Doppler, 1Password, or git-crypt are equally fine.

## Code structure

- **STRUCT-1** — Optimize for cohesion, not line count: everything one concept
  needs in one place, named after the concept, understandable without reading
  past its direct imports.
- **STRUCT-2** — One concept per file, where a concept can be a whole feature.
  Don't split cohesive logic to hit a line count; one function per file
  scatters a feature for humans and agents alike.
- **STRUCT-3** — ~500 lines is a soft ceiling, not a target. Multiple
  responsibilities is the smell, not length.
- **STRUCT-6** — Put the seam where the next change will land, and nowhere
  else. Add an interface when the second case actually arrives, not before
  (DUP-3). `/jgl-setup-harness` installs the structural check that enforces
  the dependency rules mechanically.

## Testing

Every feature, fix, or behavior change ships with tests that exercise it.

- **TEST-1** — Plans include the test work. A plan for a feature, fix, or
  behavior change names the tests to write or update and where they live. If a
  plan leaves them out, point that out before it's approved.
- **TEST-2** — Assert real values. `expect(total).toBe(42.5)`, not
  `expect(total).toBeDefined()`. If the real assertion is hard to write,
  that's a design problem to surface, not a reason to weaken the test.
- **TEST-3** — Each test exists to catch a specific bug. Fewer tests with tight
  assertions beat more coverage from filler.

## Git

- **GIT-1** — No AI attribution in commits: no `Co-Authored-By` naming an AI
  tool or model, no "Generated with" line, no 🤖. If your tool adds one by
  default, remove it. The `commit-msg` hook rejects them.
- **GIT-2** — Commit and push are separate steps; push only when the user
  asks, and only the working branch. Never push to `main`; changes reach it
  only through a merged PR.
- **GIT-3** — Messages say what changed and why, not "update" or "fix".
- **GIT-4** — No work happens on `main`: no edits, commits, or merges there.
  All work goes on a feature branch or the user's own branch (e.g.
  `alex-dev`) and reaches `main` through a PR. Before editing anything, check
  the branch; if it's `main`, stop and ask which branch to use (offer the
  user's own branch or a new feature branch) before changing files.
- **GIT-5** — Past roughly 10 changed files or 400 added lines since the last
  commit, checkpoint: commit, update `docs/PROGRESS.md` (done, in flight, next
  step, open questions), and continue in a fresh session that starts from that
  file and `git log`. Long sessions drift from these rules with no signal that
  it's happening (`ARCHITECTURE.md`, "Session length").

## Where the rest lives

Read these when the task calls for them, not up front:

- Writing or reviewing tests → `PRACTICES.md`, "Testing craft"
- Adding or upgrading a dependency → `PRACTICES.md`, "Dependencies"
- Planning a refactor → `PRACTICES.md`, "Refactoring"
- Setting up coverage, CI, or the pre-push hook → `PRACTICES.md`, then
  `TESTING-PIPELINE.md` for the Vitest + Playwright stack
- Handling secrets or a leaked key → `SECRETS-PIPELINE.md`
- Changing which checks gate a push, or why a check doesn't →
  `ARCHITECTURE.md`
- Skills: `/jgl-init`, `/jgl-scope`, `/jgl-check`, `/jgl-status`,
  `/jgl-commit`, `/jgl-push`, `/jgl-review-code`, `/jgl-review-testing`,
  `/jgl-setup-testing`, `/jgl-setup-secrets`, `/jgl-setup-harness`
