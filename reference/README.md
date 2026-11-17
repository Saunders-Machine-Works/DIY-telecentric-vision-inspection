# reference/

Material the project was built from or checked against. None of it is part of the product, and
none of it is in git: `.gitignore` ignores everything in this folder except this README. Some of
it can't be republished (vendor PDFs, third-party code, a commercial SDK), and the rest is either
large or private.

A fresh clone has only this file. Ask the project owner for a copy of the folder if you need it.

| Folder | What's in it | Used by |
|---|---|---|
| `software/` | Basler pylon 7.5 SDK installer (the `camera` crate grabs frames through pylon); two third-party repos studied for ideas, Loupe (Apache-2.0) and Measurific (MIT) | `docs/OTHER_SOFTWARE_REVIEW.md` |
| `videos/` | The screen recording of commercial vision-measurement software, plus `segments/`: the split clips, extracted frames, audio and transcripts | `DESIGN_REFERENCE.md` (frame paths are relative to `videos/segments/`) |
| `documents/` | Vendor brochures and specs (Keyence IM-7000, Mitutoyo Quick Vision), a metrology reference guide, and the pin-gage calibration certificate | `MILESTONES.md` |
| `images/` | Photos of the rig, tunnel and masks; the rig layout sketch (`layout.jpg` / `.pptx`); a raw debug frame from the camera (`probe.pgm`) | design discussions only; no code reads them |
| `prompts/` | The original AI prompts the project started from (`Rust prompt.md`, `Scrape Video.md`) | `MILESTONES.md` |
| `project-kit/` | The original copy of the agent/process kit whose files now live at the repo root | nothing; kept to compare against |
| `archive/` | `pre-public-history.bundle`: git history from before the repo went public (`git clone pre-public-history.bundle old-history`) | — |

Not stored here: `captures/` (calibration files and frames the app reads at run time),
`measurements.csv` and `diag_*.png` (app and example output). They stay at the repo root, also
gitignored, because the code uses those paths.

## Rules

- Put new reference material in the matching subfolder (make a new one if none fits) and add a
  row to the table above. Never commit it.
- Third-party code in `software/` is for reading only: don't run, install or build it. An idea
  re-implemented in our code needs no licence notice; a close port of their code needs the
  MIT / Apache-2.0 notices.
