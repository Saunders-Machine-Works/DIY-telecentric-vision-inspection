# Telecentric Vision Measurement System — Background for a Fresh Build

This document exists for one purpose: you (an LLM, or a developer working with one) are about
to build a new implementation of this project from scratch, on **identical hardware** to an
existing, working system. This file is everything the first build learned the hard way —
hardware quirks, algorithm choices that worked and ones that didn't, bugs that looked like one
thing and turned out to be another, and calibration gotchas — so you don't have to rediscover
all of it by trial and error against real parts.

It is not a spec. It's a "here's what we know now" briefing. Read it before writing code, and
re-read the relevant section whenever you hit something that *feels* like it should be simple
and isn't — there's a good chance it's covered below.

## 1. What this system is

A shop-floor optical comparator, built for **Saunders Machine Works (SMW)**, a production
machining shop. An operator places a small machined part (gage pins, threaded gages, turned
parts) in front of a backlit camera and gets dimensional measurements — diameters, thread
major/minor/pitch diameter, TPI — accurate enough to be trusted in place of calipers/micrometers
for routine QC, with subpixel-accurate edge detection as the foundation.

The original project brief (included in the first build's repo as
`vision_measurement_software_spec.md` — ask for it if you don't have it) envisioned something
broader: a general multi-tool "digital caliper" plus a master-library/teach-and-compare
workflow modeled on a Keyence TM-X comparator, eventually feeding pass/fail data to a sorting
robot. **That is not what got built first, and that's a deliberate, working lesson in itself:**
the team built one thing well — a single-purpose thread-gage inspection tool — validated it
against real certified references, and only then worried about generality. A circle/line
"digital caliper" mode and a basic diameter-only readout also exist, but the master-library,
auto-alignment, and robot-integration pieces of the original spec were never built. If you're
starting fresh, **strongly consider the same ordering**: get one real, trusted measurement
working end-to-end on real hardware before generalizing.

## 2. Hardware (yours is the same)

- **Camera:** Basler acA2440-20gm — Sony IMX264, 2/3" sensor, 2448×2048, 3.45µm pixels,
  monochrome, global shutter, **GigE Vision**. Accessed from Python via `pypylon`.
- **Lens:** Opto Engineering TC12056 **bi-telecentric** lens, 0.114x magnification, ~157.8mm
  working distance. This single fact is architecturally important and saves you real
  engineering effort: **fixed magnification, no perspective distortion, regardless of a part's
  height/working-distance variation.** A tilted-in-depth part's silhouette width is *not*
  affected by that tilt (true for a cylinder — this is used directly in the thread tool's tilt
  math, see §5.4). Don't build a homography/perspective-correction pipeline; it's solving a
  problem this lens doesn't have.
- **Illumination:** backlit, collimated, blue LED — parts render as **dark silhouettes on a
  bright, uniform background**. All edge-finding in this project assumes that polarity
  convention.
- **Calibration target:** Opto Engineering PT-series chrome-on-glass chessboard reticle (e.g.
  0.6mm pitch, ~54×42 interior corners on the one used here). This is for system pixel→mm scale,
  not for per-part geometry.
- **Resulting scale:** ≈30µm/pixel at the object plane before subpixel refinement, ≈74×62mm
  field of view.

### GigE gotcha (will bite you immediately)

A `pypylon` grab can **transiently fail right after `Open()`/`StartGrabbing()`** with "buffer
was incompletely grabbed" while the GigE link/ARP is still settling. This is not a sustained
bandwidth problem — it clears within a retry or two. Build retry logic into your camera
`get_frame()` from day one (a handful of retries with a short sleep between), and consider a
short `sleep` right after opening the camera before the first grab. Do **not** treat this as a
reason to add complex error recovery elsewhere; it's purely a "the link just came up" artifact.

### Camera access doesn't require the GUI

`pypylon`'s `InstantCamera` can be driven directly from a plain script — open, grab frames,
close — with no GUI involved. This matters a lot for calibration and debugging: you can script
multi-capture calibration runs, dump frames to disk, and inspect overlays as PNGs, all without
a human dragging boxes in a window. Build your camera interface so this is easy from the start
(a thin `Camera` protocol with `open()/get_frame()/close()`, no GUI coupling).

## 3. Core architectural principles that held up

These came from the original spec and proved correct in practice — keep them:

1. **System calibration (pixel→mm scale) is separate from any per-part correction.** Don't let
   "we found a small bias on this part type" leak into the lens/camera scale factor. They are
   different kinds of correction with different lifetimes (see §6).
2. **The measurement engine is UI-independent.** Every tool is a plain function:
   `(image, operator-supplied geometry, calibration) -> (typed results, fit-geometry-for-overlay)`.
   The GUI only handles placement/display. This paid off directly — most of the actual
   validation and bug-fixing in this project happened by calling measurement functions straight
   from a script against saved/synthetic images, never touching Qt.
3. **One typed result object for everything** (`MeasurementResult`: feature name, tool type,
   value, unit, tolerance, pass/fail, timestamp, metadata dict). Every tool returns a list of
   these. This made it trivial to add new derived quantities (e.g. "worst tooth" alongside
   "average") without touching the UI contract.
4. **Snapshot-based measurement, not per-frame fitting.** Early on it's tempting to re-run the
   full measurement every live frame. In practice: average a short burst of frames (this project
   uses 8) into one image at the moment of "measure," and only re-run on an explicit trigger
   (placing a tool, hitting Remeasure). This trades a brief (~250–400ms) blocking capture for
   materially lower sensor noise and removes the cost of re-fitting dozens of times a second for
   no benefit (the part isn't moving while being measured).

## 4. Suggested project layout

What worked here (Python 3.11+, PySide6 for the GUI, OpenCV + NumPy + SciPy for CV/math,
pytest):

```
measurement/   subpixel edge detection + line/circle fitting + thread tool
               (the shared, UI-independent, heavily-tested core)
camera/        Camera protocol + PylonCamera (real) + MockCamera + file-replay camera
calibration/   reticle-based system calibration, spatial model, bias corrections
synthetic.py   synthetic test-image generators (see §8 — do this before touching hardware)
ui/            the actual windows (PySide6), one per workflow
app.py         CLI entry point, picks which window to open
```

Dependencies that mattered: `pypylon` (camera, optional extra — import it lazily so the rest of
the app works without it installed), `opencv-python`, `numpy`, `scipy` (`scipy.ndimage`,
`scipy.signal.find_peaks`, `scipy.optimize.least_squares`), `PySide6`.

## 5. The measurement core — what actually works

### 5.1 Subpixel edge detection

The foundation everything else is built on. **Do not threshold a binarized image and call it a
day** — it won't get you dowel-pin-grade accuracy. The working approach:

1. Sample image intensity along a line segment (perpendicular to wherever you expect an edge)
   using **cubic B-spline interpolation** — `scipy.ndimage.map_coordinates(order=3,
   mode="nearest")` is the reference implementation to match if you reimplement this (e.g. in
   Rust/C++ later). This is *not* the same as naive linear interpolation or Catmull-Rom — it has
   a recursive IIR prefilter step. If you port this to another language, validate against scipy
   at near machine precision (1e-9-ish) on randomized inputs *before* building anything on top
   of it; getting this subtly wrong silently degrades every downstream measurement.
2. Smooth the sampled profile with a small Gaussian (`sigma` on the order of 1px — this is a
   tunable, not a magic constant; too much smoothing costs you real accuracy, too little makes
   you noise-sensitive).
3. Take the gradient of the smoothed profile; the edge is where `|gradient|` peaks.
4. Refine the peak location with a 3-point parabolic fit around the discrete maximum — this is
   what actually gets you sub-pixel (not just sub-sample) precision.
5. Reject with a `min_strength` (gradient magnitude) floor and `polarity` filter (rising vs.
   falling vs. either) so you don't return a plausible-looking but wrong edge from noise.

Realistic achievable accuracy with this pipeline on real hardware: "a few hundredths of a pixel"
on the profile itself, translating (through calibration) to a few ten-thousandths of an inch on
a certified gage pin — this is the number the original spec's acceptance test targeted (1.000"
pin → 1.0000" ± 0.0002") and it's achievable.

### 5.2 Periodic edge detection (checkerboard pitch, thread crests)

A second primitive, built on top of the first: scan a long line, find *every* subpixel edge
along it (not just the first/strongest), then treat the result as a periodic signal. Used for
two unrelated things that turned out to need the same tool:
- Reading the calibration reticle's checkerboard pitch directly (see §6.1 — faster and more
  robust than corner detection for a fine-pitch board).
- Finding a thread's crest/root sawtooth along the shaft axis.

**Tune `min_strength` carefully and re-check it if lighting/scale changes.** On the real
reticle, a `min_strength` of 1.0 (fine for a single clean edge) let weak noise-floor peaks
(strength ~1–1.3) fragment real, much-stronger transitions (strength ~11–37) into spurious
sub-periods — this *silently* read the pitch out at ~2x the true value, no error, just wrong.
5.0 cleanly separated real transitions from noise on that image. Don't assume a threshold that
worked on a clean synthetic test image will work on a real backlit photo; check a strength
histogram on a real capture.

**Harmonics are a real failure mode, not a theoretical one.** Periodic peak detection can lock
onto a sub-structure within one period (e.g. half the true pitch) rather than true period-to-
period spacing. If you know the expected pitch from context (e.g. "every part here is a known
thread spec"), check a detected pitch against the expected value *and its 2x/0.5x harmonics*
before trusting it, not just a raw tolerance band around the expected value.

### 5.3 Line/circle fitting

Algebraic fit for a fast initial guess, then geometric (orthogonal-distance) least-squares
refinement — algebraic circle fits (e.g. Kåsa) minimize an algebraic proxy, not true Euclidean
residuals, and geometric refinement is what actually delivers the accuracy you want. Wrap both
in RANSAC to reject outliers (debris, reflections, a chipped edge) before the final fit, not
after.

A gage pin's diameter is measured as a **two-edge gap** (parallel-line / line-gap tool), not as
a circle fit — you're looking at the pin from the side (silhouette), not end-on, so its
silhouette is two straight edges, not a circle. Don't assume every "diameter" measurement is a
circle-fit problem; check what the part's silhouette actually looks like from your camera angle
first.

### 5.4 Thread measurement — the hard part, and where most of the real bugs were

This is worth the most detail because it's where this project spent the most debugging time,
and every one of these was found against **real parts**, not synthetic images — synthetic
testing catches a lot, but budget real-hardware validation time specifically for this tool.

**The approach:** a V-thread's silhouette, viewed from the side, traces the thread's radial
profile along the shaft axis — a sawtooth between crest (major diameter) and root (minor
diameter). Scan perpendicular to the shaft axis at many points along it, find the strongest
transition at each (silhouette edge), get a traced boundary curve, then run periodic peak
detection on *that* curve to find individual crest/root positions. Do this for **both sides** of
the shaft (the operator boxes one side; the far side is located automatically — see below) and
diameter = distance between corresponding crest (or root) positions on the two sides.

**Pitch diameter is calculated, not directly measured**: true flank-midpoint measurement needs
much more precise flank-angle fitting than crest/root detection gives you. The pragmatic
standard-thread-form approximation (ASME B1.1 / ISO 68-1, for 60° UN/metric threads):

```
pitch_diameter = major_diameter - 0.649519 * pitch      # 0.649519 = 0.75 * sqrt(3) / 2
```

This assumes a textbook thread profile, not actual flank angle or wear — treat it as a
calculated check value, not a gauge-equivalent measurement, and say so in the UI.

**Chip/wear detection needs a "worst tooth" value, not just an average.** A worn or chipped
single-point threading insert typically cuts *one or a few* teeth shallower than the rest — an
average over a dozen good teeth almost completely hides a single bad one. Report both: the
averaged diameter (what a standard tolerance class actually gauges) and the single worst
tooth's value (what actually catches a localized chip). This was a headline requirement, not a
nice-to-have — build it in from the start.

**Finding the opposite side — don't just take "the strongest edge beyond here."** The first
version of opposite-side detection used "find the single strongest transition anywhere past the
box" and it locked onto a fixture clamp block well beyond the shaft (sharper edge than the
shaft's own blurred silhouette) — roughly doubling the reported diameter. Fix: take the
**nearest** sufficiently-strong transition, not the strongest one anywhere. The true opposite
wall is the first real material boundary you cross, full stop.

**That "nearest" probe needs to sample more than one line, or it becomes bimodal.** A single
probe line, at one exact axial position, occasionally saw *two* very close candidates straddling
the strength threshold — the true wall, plus a weaker secondary blip (apparently the thread's
own profile catching blur at that one specific phase). Which one counted as "nearest" flipped
between otherwise-identical, untouched captures of the same physical part — confirmed on real
hardware: 5 untouched recaptures split into two clusters **0.012in apart** before fixing this,
down to a **0.0002in range** after. Fix: probe from several axial positions spread across the
box, take the median. **If you ever see a measurement that looks "noisy" but the noise comes in
two discrete clusters rather than a smooth spread, suspect a discrete selection bug like this
one before blaming camera/lighting noise.**

**A part sitting at a real angle must be measured correctly, not just tolerated.** Operators
reseat parts in a V-block a few degrees off level routinely — this is normal, not an error
condition, and 1 pass at "don't crash, just refuse if tilted" is not good enough. The full,
working approach has two stages:

1. **Re-aim the scan axis itself.** Trace once along the box's own (as-drawn, assumed straight)
   axis, estimate how far the traced profile's true axis drifts from it, then **rotate the scan
   axis by that amount around the box's own center** and re-trace before measuring anything.
   This matters more than it sounds like it should: a search band of fixed half-width, run along
   the box's *original* axis, drifts further from the part's actual crest-to-root line the
   further you get from the box's center on a tilted part. On a *tall* box (many pitches) that
   drift can approach or exceed the band's half-width toward either end, clipping/biasing
   exactly the teeth furthest from center — confirmed on real data: a tall box read major
   diameter several percent low at a tilt a *short* box on the same part, same tilt, measured
   correctly. Re-aiming the axis itself (not just correcting the numbers afterward) fixes this
   because the search band then stays centered on the part along the box's whole length,
   regardless of box size — **this is the single most important lesson in this whole
   document**: don't try to fix a geometric sampling problem with a scalar correction factor
   applied after the fact; fix the sampling geometry itself when you can.
2. **Apply a residual `cos(tilt)` correction** for whatever small tilt remains after re-aiming
   (the first-pass estimate is rarely perfect). The physics: measuring crest-to-crest distance
   *perpendicular to the scan axis* when the part's real axis is tilted by `theta` relative to it
   reads `true_width / cos(theta)`, not `true_width` — the "tilted ruler" effect. This is exact
   for a straight cylinder at in-plane tilt (within the image). **Pitch is corrected the
   opposite direction** (`measured_pitch * cos(theta)`, not divided) because it's a spacing
   *along* the axis, which under-reads rather than over-reads under the same tilt.
3. Tilt estimation itself should fit a line through **peaks only, or troughs only** (whichever
   has more points) — not through the raw traced curve. A short box samples the sawtooth at
   near-arbitrary phase at each end; fitting the raw curve mistakes that edge-truncation effect
   for several degrees of phantom tilt. Peaks (or troughs) all sit on the same constant-radius
   rail by construction, so a line through several of them reflects the true axis regardless of
   where the box happens to start/end mid-tooth. Use a robust slope estimator (Theil-Sen: the
   median of every pairwise slope) with **at least 3 points**, not 2 — 2 points gives you zero
   robustness against one truncated/outlier tooth, the exact failure mode you're trying to avoid.
4. **Detrend before comparing two sides**, and reference both sides' averages to the *same*
   axial point before diffing them. The two sides of a thread are generally out of phase — their
   detected teeth don't sit at matching axial positions — so on a tilted part, each side's own
   average silently bakes in wherever *that side's* teeth happened to average out to. Comparing
   two such raw averages mixes `tilt_slope * (difference in the two sides' mean axial position)`
   into the diameter. Fix: move each side's average to one shared reference axial point (e.g.
   the box's own midpoint) before diffing. This bug was easy to miss because it only shows up as
   "a tall box's result differs a bit from a short box's result" — a symptom that looks like
   general noise unless you specifically test box-size independence.
5. Geometry, not math, is the real ceiling on tilt tolerance: past some angle, the search band
   literally cannot contain the full crest-to-root swing along the box's whole re-aimed length no
   matter how good the correction math is. Let that geometric coverage check (e.g. "did enough
   scan lines find a strong edge") do its job rather than only relying on one explicit angle
   ceiling — both should exist, but the explicit ceiling is a backstop, not the only guard.

**Don't reuse a bias correction across different edge geometries, or across physical setup
changes.** Two separate, hard-won lessons here, both costly if conflated:
- A bias calibrated from a smooth two-edge cylinder's silhouette (gage pin OD) made thread
  measurements *worse*, not better, when applied to thread crest/root edges — a sawtooth edge
  behaves differently than a smooth cylinder's edge. Keep these corrections separate per edge
  *type*, never share one across tool types just because both are "two-edge measurements."
- A bias calibrated once, on one physical setup/orientation, measurably did **not** transfer
  when the same gage was later reoriented (same gage, same software, different seating) — off by
  enough to matter. Any correction of this kind needs to be documented as tied to a specific
  physical configuration and re-derived whenever that configuration changes. Don't assume a
  number from a prior calibration run is still valid just because the part and software are
  unchanged if the *fixture/orientation* changed.

## 6. Calibration — what to measure, and how

Two genuinely different kinds of calibration; keep them conceptually and architecturally
separate (§3.1):

### 6.1 System scale (pixel → mm)

From the PT-series reticle. **Corner-based detection (`cv2.findChessboardCorners`) was too slow
and unreliable on a real fine-pitch reticle** (~54×42 interior corners) — tens of seconds per
candidate grid-size guess, and it failed outright even for size guesses within a couple squares
of correct. The fix that actually worked: measure the checkerboard's period **directly along
several scan lines** (reuse the periodic-edge-detection primitive from §5.2) instead of
detecting individual corners, then average. Also meaningfully more precise, since you're
averaging a subpixel transition fit over dozens of periods along multiple lines rather than
relying on one corner detector's opinion per corner.

A single reticle capture only covers whatever fraction of the sensor the reticle physically
fills in one shot — if the lens isn't *perfectly* telecentric/distortion-free in practice (ours
wasn't, by ~0.3–1% across the frame), consider a **field-dependent correction**: sample local
scale at several points across the field (not just one global average), and fit a low-order
(bilinear, not higher) surface. Bilinear is deliberate — with partial-field coverage, anything
higher-order can extrapolate to nonsense outside the sampled region; bilinear degrades
gracefully instead. Moving the reticle to several positions and stitching samples from all of
them before fitting gets you a correction trustworthy across the whole sensor rather than one
partial-field capture's limited extent.

### 6.2 Per-geometry bias corrections

Separate from system scale. Even with correct scale and good edge detection, a real silhouette
edge was found to sit consistently a bit *inside* the true mechanical surface (blur/diffraction/
threshold behavior at the edge, not a scale problem) — measured against certified Deltronic gage
pins. The size of that bias wasn't quite constant across pin sizes, so a piecewise-linear
interpolation against the raw (pre-correction) reading (sorted `(measured, correction)` points)
beat one flat average — a flat average overcorrected the smallest calibration pin in real use.
Hold flat at the nearest endpoint outside the tested range rather than extrapolating.

**Procedure that worked for finding these corrections** (do this on your own unit — the numbers
below are illustrative only, not transferable to different hardware or even the same hardware in
a different physical configuration):
1. Take **several independent, untouched** captures of a certified reference (don't touch the
   part between captures — you're averaging out camera/measurement noise, not fixture
   repeatability).
2. Average the raw (uncorrected) readings.
3. `bias = true_certified_value - raw_average`.
4. **Verify against fresh captures not used to derive the correction** before trusting it.

On the specific unit this project used: a two-edge/pin bias around 0.05–0.09mm depending on pin
size; a thread-specific major/pitch-diameter bias, after the tilt-correction fixes in §5.4 were
in place, of only ~0.03mm (small, because the underlying measurement had gotten much more
accurate — before those fixes, an *apparent* bias of ~0.4mm looked necessary, which was really
masking the box-size-dependent bug, not a real optical/edge effect. **If a "calibration"
correction you're computing is large, ask whether you're actually calibrating out a bug.**

## 7. Performance lessons

- **Profile before reaching for a faster language.** The instinct when Python feels slow is "port
  it to Rust/C++." Here, profiling found the real cost wasn't language overhead — it was a cubic-
  spline prefilter step whose cost scales with the *size of the image region being scanned*, not
  the number of query points along it, combined with code that re-prefiltered the *same* region
  from scratch on every one of ~1000–2000 per-scan-line calls during auto-detection. Batching
  those calls to share one prefilter of the union region, **done in pure Python/SciPy** (a manual
  `np.pad(..., mode="edge")` + `scipy.ndimage.spline_filter` + `map_coordinates(...,
  prefilter=False)`, plus vectorizing the smoothing/gradient/peak-selection across all scan lines
  at once), got a ~4x and ~2.4x speedup on the two hottest operations with zero change of
  language. A from-scratch build in a fast language gets to skip this lesson; a from-scratch
  build in Python should learn it immediately rather than assume Python is the bottleneck.
- A Rust/PyO3 extension reimplementing the same algorithm was built and validated (bit-exact to
  ~1e-9 against the SciPy reference, further speedups on top of the batching win), but deliberately
  **not** made the default — the pure-Python path was judged "good enough" first, with Rust kept
  as a documented, tested, opt-in upgrade path for later rather than something to maintain and
  debug in parallel before the algorithm itself had stabilized. Consider the same ordering:
  correctness and stability first, in whatever language gets you there fastest to iterate in;
  raw speed later, once you know what you're actually optimizing.
- A live camera-preview timer competing with mouse-drag handling on Qt's single GUI thread made
  box-dragging feel laggy — not because the math was slow, but because decoding/repainting a
  multi-megapixel frame ~30x/second was fighting for the same thread as drag events, for a video
  image the operator isn't even looking at while actively dragging a box. Fix: skip the frame
  grab/repaint entirely while a drag is in progress. If your UI feels laggy during interaction,
  check for this kind of same-thread contention before assuming your algorithm needs optimizing.

## 8. Testing strategy that worked

- **Build synthetic test-image generators before touching real hardware.** A step-edge generator,
  a checkerboard generator, and a (fairly simple, not a true 60° V-groove) thread-silhouette
  generator with known ground truth let the entire edge-detection and thread-measurement core get
  built and validated — including the tilt-correction math — against known-exact geometry, with
  controllable blur and noise, before any camera was involved. This decouples "is my algorithm
  correct" from "is my camera/lighting good," which is otherwise very hard to debug at the same
  time.
- **Every real-hardware bug became a permanent regression test**, usually by directly simulating
  the specific failure pattern found (e.g. "splice a shallower-root rendering into one pitch
  period, at row boundaries that land exactly on crests so the splice itself introduces no
  spurious edge, confirm the worst-tooth reading surfaces it" for the chip-detection feature; "a
  faint single-row artifact at exactly the probe's sample point" for the opposite-side-probe bug).
  This is the highest-leverage testing practice in the whole project — a bug found on real
  hardware and *not* turned into a synthetic regression test will come back.
- **Qt UI logic is testable headless.** `QT_QPA_PLATFORM=offscreen` plus a fixed-frame mock
  `Camera` lets you drive the actual window classes (button clicks, box placement/drag signals,
  keyboard shortcuts) in plain pytest, no display needed. Keep UI classes thin enough that the
  interesting logic (state transitions: pending box → confirmed → measured → cleared) is
  reachable this way rather than buried in Qt signal plumbing you can't unit test.
- **Script the camera directly for calibration work, skip the GUI.** See §2 — a plain Python
  script using the camera class, the measurement functions, and OpenCV to save overlay PNGs for
  visual review was how both the opposite-side-probe bug and the final gage calibration in this
  project were actually done. Don't assume calibration work requires the finished UI to exist.

## 9. UI/UX lessons

- **Fewer, more reliable workflows beat more "convenient" automated ones that still need
  operator review every time anyway.** This project built, then later *removed*, both a one-
  click auto-detect-the-box feature and a toggle between "box one side" / "box the whole shaft."
  Auto-detection's box was always just a starting point the operator still had to review and
  often adjust — it saved little over dragging the box directly, while adding a second thing to
  learn and a second code path to keep correct across the tilt/robustness fixes in §5.4. Simpler
  won. Don't assume more automation is better UX by default for a precision tool where the
  operator needs to trust every result.
- **Show a placed/detected box (or any tool) as an editable, unconfirmed suggestion before
  measuring**, not as an immediate measurement. Letting the operator drag corner handles to
  adjust before confirming caught real cases (a short "NO GO" gage where even a careful auto-box
  undershot the available thread) that a measure-immediately flow would have silently gotten
  wrong.
- **Give the operator an explicit "start over" action** (clear the current box/result and return
  to the initial state) distinct from "cancel this one pending edit" — these are different
  operations and conflating them is confusing once you have a confirmed box with results showing.
- **Dual-unit display, primary unit large/bold/colored, secondary unit smaller underneath** — this
  shop works in inches; showing mm only as a smaller secondary line (not equal-weighted) matched
  how the readout actually gets used.
- **Report both an averaged value and a worst-case/outlier value when the whole point is catching
  localized defects** (see §5.4's worst-tooth discussion) — don't let a UI only show the number
  that's easiest to compute.
- **A results overlay drawn directly on the image needs to bound itself to the visible
  frame/viewport.** A multi-line result readout anchored at a fixed offset from a tool's position,
  with no check against the image's own edges, will run off-screen whenever that tool happens to
  sit in the lower/right part of the frame — which is routine, not an edge case, for a part that
  isn't dead-center. Measure the label's actual rendered size and clamp its position before
  drawing it.

## 10. Known rough edges in the first build (useful to know, not necessarily to copy)

- Pitch diameter is a standard-thread-form *calculation* (§5.4), not a true flank-midpoint
  measurement. If a shop's QC actually requires gauge-equivalent pitch diameter, that's
  meaningfully more work (precise flank-angle fitting) and was explicitly deferred.
- A part-presence-detection + master-library + auto-alignment workflow (the original spec's
  "master-comparison mode") was never built. Everything shipped is manual/ad-hoc placement by
  the operator, once per tool type, with Remeasure for repeat parts in the same fixture position.
- One internal auto-detection helper (locating a periodic run within a scanned silhouette) uses a
  plain per-position rolling-median loop that's measurably a secondary cost center — minor, and
  only affects auto-detection (which, per §9, ended up not being exposed in the final UI anyway).
- No data logging, no pass/fail signal to external systems, no robot integration — all explicitly
  out of scope so far, per the original spec, and still true.

## 11. If you take nothing else from this document

1. The lens is bi-telecentric — don't build perspective correction you don't need.
2. Validate subpixel edge detection against a scipy (or equivalent) reference at near machine
   precision *before* building a measurement pipeline on top of it.
3. When a real part sits at an angle, fix the *scan geometry* (re-aim the axis) before reaching
   for a scalar correction factor — it's more robust and avoids subtle averaging bugs.
4. A "noisy" measurement that's actually bimodal (two discrete clusters, not a smooth spread) is
   a selection bug, not noise — go find the discrete decision point.
5. Keep system-scale calibration and per-geometry/per-setup bias corrections conceptually
   separate, and never assume a bias correction survives a physical setup change.
6. Build synthetic ground-truth test generators first; turn every real-hardware bug into a
   permanent regression test against one.
7. Profile before you optimize, and before you change language.
