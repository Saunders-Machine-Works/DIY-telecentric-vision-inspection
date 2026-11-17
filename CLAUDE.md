@AGENTS.md

## Claude Code notes

- Skills live in `.claude/skills/`. `/jgl-check`, `/jgl-review-code` and
  `/jgl-review-testing` run in a forked context and hand back a report, so
  their tool output doesn't fill this session.
- `/jgl-init`, `/jgl-scope`, `/jgl-commit`, `/jgl-push` and the `/jgl-setup-*`
  skills run only when the user types them.
- If `/jgl-init` installed the agent guards, `.claude/settings.json` blocks
  reading secret files and force or `--no-verify` pushes. A blocked call means
  the guard is working; tell the user rather than finding a way around it.
- Reference material lives in `reference/` (`software/`, `videos/`, `documents/`,
  `images/`, `prompts/`, `project-kit/`, `archive/`). Only `reference/README.md`
  is tracked. Put new reference files in the matching subfolder, add a row to
  that README, and never `git add -f` anything under `reference/`. Third-party
  code in `reference/software/` is read-only: don't run, install or build it.
