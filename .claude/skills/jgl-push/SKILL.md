---
name: jgl-push
description: Pushes the working branch to the remote, refusing any push to main, and never forcing or skipping hooks. Use when the user says "push", "/jgl-push", or "send it up".
disable-model-invocation: true
---

# Push

Push committed work to the remote. Pushing can trigger CI and deploys and
makes work public on shared repos, which is why it's a separate step from
`/jgl-commit` (GIT-2).

## Before pushing

Show the user what's going out: the current branch and the commits ahead of
the remote (`git log @{u}..HEAD --oneline`, or against `origin/<default>` when
there's no upstream). If nothing is ahead, say so and stop.

## Branch rules

- **Feature branch or the user's own branch** (e.g. `alex-dev`; anything but
  `main`, `master`, `production`, `release`): push. Use
  `git push -u origin <branch>` the first time.
- **`main`, `master`, or a production branch:** don't push. No work happens
  on `main` (GIT-4); it changes only through a merged PR. Tell the user and
  offer to move the commits to a working branch instead:

  ```
  You have 3 commits on main. Work doesn't go on main in this repo.
  Move them to a branch and open a PR:
      git switch -c <branch>   # or switch to your own branch, e.g. alex-dev
      git push -u origin <branch>
  ```

  Don't reset `main` to drop the commits without the user's yes.

## Pushing

Let the pre-push hook run — it re-runs the type check and tests and stops a
red push. Don't use `--no-verify` to get past it.

If the push is rejected as non-fast-forward, don't force it. Tell the user the
remote has commits they don't have, suggest `git pull --rebase`, and let them
decide. Use `--force` / `--force-with-lease` only when the user explicitly
asks and has confirmed what it overwrites. Push only the branch that's
checked out or the one the user named — if unsure which, ask.

## Report

```
Pushed 3 commits to origin/feature/login-form.
https://github.com/<owner>/<repo>/tree/feature/login-form
```

Derive the URL from `git remote get-url origin` when you can, so the user can
open the PR or watch CI.
