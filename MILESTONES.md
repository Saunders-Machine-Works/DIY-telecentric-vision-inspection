# Thread Metrology (Rust) — Milestone Plan (DRAFT v0)

Status: the original plan, written 2026-11-07 before any code. Kept as the roadmap; section 0 and the
open questions are historical. Current state: `docs/PROGRESS.md`. Overview and setup: `README.md`.
Sources: `reference/prompts/Rust prompt.md`, `PROJECT_BACKGROUND.md`, `DESIGN_GUIDELINES.md`, `DESIGN_REFERENCE.md`, `reference/documents/` (the `reference/` files are local only; see `reference/README.md`).

## 0. Findings about this machine (checked 2026-11-07)

| Item | State |
|---|---|
| Rust (`rustc`, `cargo`) | **Not installed** |
| MSVC linker (`link.exe`) | **Not on PATH** (may exist in a VS Build Tools install; unchecked) |
| Basler Pylon SDK | **Not found** under `C:\Program Files\Basler` |
| git | **Not installed** |
| Hardware list | `reference/prompts/Rust prompt.md` line 1 says "[Pasted text #2 +3 lines]", so the pasted hardware list never reached the file. I'm assuming the usual list: TC12056 lens, CX0404-455C1 backlight, DCS-100E controller, PT reticle. Please confirm or paste it. |

Each of these needs your OK first (AUTO-2: installs and dependencies).

## 1. Architecture (carried from PROJECT_BACKGROUND §3, §4)

Cargo workspace, measurement core independent of UI and camera:

```
crates/
  metrology-core/   subpixel edges, periodic edges, line/circle fit, thread tool, calibration math.
                    Pure Rust, no I/O. Tested against synthetic images and scipy reference vectors.
  camera/           trait Camera { open, grab, close } + PylonCamera (FFI) + FileCamera + MockCamera
  app/              GUI + CLI entry. Thin; state machine only.
tools/              scripts that drive camera + core headless (calibration, captures, overlay PNGs)
tests/fixtures/     synthetic generators + scipy-generated golden vectors
```

Principles kept: snapshot-based (average 8 frames, measure on trigger), one typed `MeasurementResult`,
scale calibration separate from per-edge-type bias, bi-telecentric so no perspective correction,
re-aim the scan axis for tilt before applying `cos(theta)`, worst tooth beside average, expected-pitch
harmonic check, nearest-edge (not strongest) for the opposite wall, probe at several axial positions.

## 2. Candidate dependencies (each needs your approval — AUTO-2)

| Need | Proposal | Why / alternative |
|---|---|---|
| Camera | Basler Pylon SDK via thin `bindgen` FFI (own crate) | Same driver stack as the first build, GigE retry behaviour known. Alternative: `pylon-cxx` crate. |
| GUI | `eframe`/`egui` | Fast start, single binary, immediate-mode overlays suit image tools. Alternatives: `iced`, `slint`. |
| Images | `image` (PNG in/out only) | Everything else hand-written in core. |
| Math | none at first (hand-rolled 3x3/LM) | Add `nalgebra` only if the geometric fit needs it. |
| Test | built-in `cargo test` + `proptest` | proptest for randomized scipy-parity checks. |

## 3. Phases and milestones

Every milestone ends with: tests green, `cargo clippy`/`fmt` clean, and two sub-agent reviews
(code speed+quality, UI+speed where UI exists), each **>= 8/10**, iterating until met.

### Phase I — See the camera, prove accuracy on the 1/2-13 GO gage (PD 0.45")

| # | Milestone | Done when | Tests |
|---|---|---|---|
| I-0 | Toolchain + repo | Rust, MSVC, Pylon installed (with your OK); workspace builds; Commands table in AGENTS.md filled | `cargo test` runs on an empty workspace |
| I-1 | Camera bring-up | Enumerate, open, grab 8-frame average, GigE first-grab retry; `FileCamera` replays PNGs | Mock: retry succeeds on 2nd try, fails cleanly after N; real: manual smoke script |
| I-2 | Capture diagnostics | Histogram, backlight uniformity, saturation check; save PNG | Synthetic frames hit each warning threshold exactly once |
| I-3 | Subpixel edge core | B-spline sampling + Gaussian + gradient + parabola peak, polarity and strength floor | Parity vs scipy golden vectors to ~1e-9; synthetic step edge at known subpixel offsets recovers within 0.02 px |
| I-4 | Periodic edges + scale calibration | Reticle pitch via scan lines, harmonic check, um/px with spread | Synthetic checkerboard 2x/0.5x harmonic regression; real reticle result stored |
| I-5 | Accuracy test on 1/2-13 GO | Scripted untouched repeats; report mean, range, bias vs 0.45" PD, with expanded uncertainty | Report is generated; acceptance numbers set after you choose the target |

Phase I exit gate: written accuracy report (repeatability on untouched captures and on reseats, bias to 0.4500").
If the bias is large, ask whether a bug is being calibrated out (PROJECT_BACKGROUND §6.2) before applying any correction.

### Phase II — Measure major, minor, PD, pitch on any thread #10 to 1/2" and M5 to M12

| # | Milestone | Done when | Tests |
|---|---|---|---|
| II-1 | Synthetic thread generator | 60-degree V-thread silhouettes with blur, noise, tilt, known truth | Generator self-checks (pitch, major, minor recovered by analytic read) |
| II-2 | Thread tool core | Silhouette trace, crest/root detection, both sides, nearest-wall opposite edge, multi-position probe | Box-size independence; bimodal-selection regression; chip splice regression |
| II-3 | Tilt handling | Re-aim axis, Theil-Sen on peaks/troughs, residual `cos` correction, detrend both sides to shared axial point | Sweep 0-10 degrees x box heights; error bounded |
| II-4 | Thread identification | Match (major, pitch) to a #10..1/2" and M5..M12 table, UN/UNC/UNF/metric coarse/fine; report ambiguity instead of guessing | Table tests for every size; near-miss pairs flagged |
| II-5 | Results model + tolerance | `MeasurementResult`, LSL/USL, avg and worst tooth, PD calculated and labelled as calculated, decision rule per guideline | Real-value assertions for each state (PASS/FAIL/Indicative/not judged) |
| II-6 | GUI v1 | Run screen per DESIGN_GUIDELINES: image canvas, results list, overlay lines for major/minor/PD/pitch, suggest-then-confirm tool, Start over vs Cancel | Headless state-machine tests (pending -> confirmed -> measured -> cleared) |
| II-7 | Speed pass | Cold start, measure latency, drag responsiveness measured and logged | Benchmarks with budgets: start < 1 s, measure < 1 s after capture (targets for you to confirm) |
| II-8 | Real-part validation | Set of known parts, repeat and reseat, compare to reference values | Every real-hardware bug becomes a synthetic regression test |

### Phase III — Auto-detect, tracking, defects

| # | Milestone | Done when | Tests |
|---|---|---|---|
| III-1 | Thread presence detection | Periodic-structure detector in the field of view, with reject for non-threaded parts | Synthetic and recorded negatives |
| III-2 | Tracking | Follow a moving part; continuous measure with per-frame confidence, lock only on a stable pose | Synthetic motion sequences, dropped frames |
| III-3 | Form defects | Per-tooth deviation, chip, flank, crest damage; thresholds from real captures | Spliced-defect regressions |
| III-4 | Review and hardening | Full-suite review, logging/traceability record | Suite review by sub-agent |

## 4. Open questions for you

1. OK to install the Rust toolchain (rustup, MSVC Build Tools if missing), Basler Pylon SDK and git?
2. OK with `eframe/egui` and the Pylon FFI approach above?
3. Accuracy targets: what number defines success in I-5 (e.g. PD within 0.0005", repeatability range under 0.0003")? PROJECT_BACKGROUND cites 0.0002" on a pin and 0.0002" range for untouched repeats.
4. Speed budgets for start and measure (my proposal: 1 s start, 1 s measure).
5. The missing hardware list from `reference/prompts/Rust prompt.md` line 1.
6. Should the code live in `C:\Users\John\Desktop\ClaudeTheadVision\` itself or a new repo folder, and may I `git init` it?
7. Unit default: inches primary, mm secondary (per background). Confirm.
