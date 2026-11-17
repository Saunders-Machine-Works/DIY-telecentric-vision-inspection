# DESIGN_GUIDELINES.md — Telecentric comparator: design and UX guideline (DRAFT v2)

Status: v2, revised after metrology review (5/10) and UX review (6/10). No application code yet.
Derived from `DESIGN_REFERENCE.md` (Keyence IM / IM-X, Mitutoyo M3) and `PROJECT_BACKGROUND.md` (hardware, measurement core,
build-1 lessons). Section 9 lists what changed from v1.

## 0. Context and honest capability statement

- Shop-floor optical comparator for Saunders Machine Works. Operator places a small machined part (gage pins, threaded gages,
  turned parts) on a backlit stage and wants a trusted number in place of calipers/micrometers for routine QC.
- Hardware: Basler acA2440-20gm (2448×2048 mono, GigE), Opto Engineering TC12056 bi-telecentric 0.114× (~30 µm/px, ~74×62 mm
  field), Advanced Illumination CX0404-455C1 blue (455 nm) backlight driven by a SignaTech DCS-100E, PT-series chessboard
  reticle. Parts are **dark silhouettes on a bright field**; the image is **mono**.
- **Capability limit (stated in the app's About/Help and on every report):** achievable accuracy in build 1 was ~±0.0002 in on
  a certified pin. This system **does not replace GO/NOGO gaging or lead/pitch-diameter gage verification** for tight-tolerance
  thread gages (W/X class). Features whose tolerance is too tight for the measured uncertainty are shown as *Indicative*
  (§4.4), never PASS/FAIL.
- Primary units: **inches**, mm secondary. Desktop app (PySide6 in build 1). Mouse + keyboard primary; touch/glove use is
  supported by target sizing (§3.3).
- Binding build-1 UX lessons (PROJECT_BACKGROUND §9): fewer reliable workflows beat convenient automations; tools appear as
  **editable, unconfirmed suggestions** before measuring; explicit **Start over** distinct from cancelling a pending edit;
  report **average and worst-case**; bound overlays to the viewport; no live repaint during drags.
- Measurement is **snapshot-based**: average ~8 frames on an explicit trigger; the live view is for placement only.

## 1. Principles

1. **The number is the product.** The result value is the largest, highest-contrast element when a result exists.
2. **The image is evidence.** Never tint, crop or decorate the silhouette. Overlays are removable and never hide the measured edge.
3. **State is never color-only.** Every state has a word, an icon *shape* and a color (§4.2).
4. **A number must carry its uncertainty.** Results show expanded uncertainty U (k=2); conformance uses a stated decision rule (§4.4).
5. **Say when a measurement can't be trusted**, with a plain reason and a next action. Never a plausible-looking wrong number.
6. **Suggest, then confirm.** Every tool is an editable proposal until confirmed.
7. **Few modes.** Run, Set up, Calibrate. Nothing else gets a top-level tab.
8. **Every result is reproducible**: it records what produced it (§4.6).

## 2. Information architecture

Top-level tabs: **Run | Set up | Calibrate**. A status strip is always visible.

### 2.1 Layout (all modes, 1920×1080 reference)
- **Canvas** (≥ 70 % width): black surround, uniform scaling only. Wheel zooms about the cursor, middle-drag pans, **Fit** and
  **1:1** buttons. Scale (µm/px) and zoom shown bottom-left of the canvas.
- **Right column** (340 px fixed): mode panel. **Measure** is a 48 px-high primary button pinned at the top of the column in Run.
- **Top bar** (44 px): mode tabs · **Program** picker (name, version, ▾ menu: New / Open / Save / Save as / Duplicate / Delete)
  · **Start over** (secondary).
- **Status strip** (bottom, 28 px), left → right: camera (connected, fps) · **backlight** (controller setpoint + *image-measured*
  background level, §6.1) · **calibration** (date, µm/px, state ✓ valid / ▲ stale / ✖ none) · bias set in use · units toggle (in | mm).
- **Banner area**: a single 40 px banner above the canvas. Priority when several apply: camera lost > no calibration > backlight
  fault > stale calibration > stale result > info. Others collapse into a "+n more" chip.

### 2.2 Run mode
Empty-program state: "No program loaded — Open one, or go to Set up to make one." (Measure disabled.)

Flow: place part → **Measure** (button or F5) → *Capturing 3/8* shown in the column and as a thin progress bar on the canvas →
canvas freezes to the snapshot with a **“Snapshot 10:42:13” badge** (top-left of canvas, so a frozen frame is never read as live)
→ rows fill. Buttons in the column header after a capture: **Remeasure** (same tools, new capture), **Repeat ×N** (§4.5),
**New part** (return to live, clears results). Measure is not cancellable (≈300 ms); Esc during capture is ignored.

**Start over (Run)**: discards the current snapshot and results and returns to live; the program is untouched. Instant, with a
**Undo** toast (persistence rule in §7.4).

**Verdict banner** (48 px tall, 24 px word, state color fill with dark text, icon left). Aggregation: any FAIL → **FAIL**; else
any CHECK → **CHECK**; else all PASS → **PASS**; no tolerances set → no verdict ("Measured only"). If any feature is INDICATIVE the
banner reads **“PASS · n features not judged (Indicative)”** (never a plain PASS). Informational features (calculated pitch diameter,
angles) do not affect the verdict unless the operator opts in per feature.

Results list: sorted by feature order; a toggle **“Errors first”** sorts worst-first. Selecting a row highlights its image
callout; clicking a callout selects its row (hover highlight on pointer devices, tap selects on touch).

Run header fields (all optional, remembered): **Lot**, **Serial** (auto-increment option), **Operator**, **Part ID**.
Bottom of column: Save · Print · Export CSV. Save/export failure shows an inline error with the path and reason.

### 2.3 Set up mode
- **Tool palette** (icon + text label): Edge pair (two-edge gap), Line, Circle, **Thread**.
- Selecting a tool shows a one-line instruction above the canvas.
- **Thread tool flow**:
  1. Drag a box over **one side** of the thread (build-1 lesson; far side found automatically as the *nearest* strong edge).
  2. The box is a **dashed proposal** with 32 px hit-area corner handles (10 px visual, zoom independent) and a live
     “Pitches in box: n” count. The scan axis is a thin line.
  3. Properties panel: **Size / Pitch / Class** preset row (UNC/UNF/metric), outputs as checkboxes (Major, Minor, Pitch diameter
     (calculated), TPI, Worst tooth). The app *suggests* a size from measured major + TPI; the operator confirms and picks the class.
  4. **Confirm** measures; the box turns solid. **Cancel** drops the pending box only. **Start over** (Set up) clears all tools in
     the working program with an Undo toast.
- Editing a confirmed tool returns it to *pending* and marks its results **stale** (§6).
- **Tolerance model** per feature: **LSL** and **USL**, either optional (external minor is max-only; class 2A/6g have an
  allowance and are unilateral). A preset fills LSL/USL from the class table; the operator may override. A feature may show a
  nominal for reference without implying symmetry. Display a deviation-from-nominal value in the row (§4).
- **Program management**: Save writes a versioned program (tools, tolerances, preset, bias-set reference, fixture/orientation
  setting). Deleting a program is a modal confirm (“Delete ‘SL-A15535’? This can’t be undone.”). Unsaved changes show a • on the
  Program name.

### 2.4 Calibrate mode
Steps: 1 Backlight/image check → 2 Place reticle → 3 Capture → 4 Review → 5 Verify → 6 Save. Each shows ✓ / current / ✖ + reason.
- **Reticle entry**: pitch (default 0.600 mm), reticle model, and **certificate ID**. Stored with the calibration.
- **Pass criteria** (editable by an admin): the criterion is a **length error after the field model is applied**: scale contribution
  ≤ 0.00005 in over 1 in (≈ 0.005 %) across the calibrated region (default; validate on real data). Scan-line pitch spread is shown
  as a diagnostic, plus ≥ 20 periods per scan line, ≥ 5 scan lines, harmonic check passed (§4.3).
- **Review** shows pitch per scan line, mean, spread (% and µm/px), and the field-dependence map when several captures exist.
- **Verify**: re-measure a certified reference *not used to derive the correction* and show error vs certificate (background §6.2 step 4).
  Calibration cannot be saved as “valid” until Verify is recorded (or explicitly waived with a reason).
- **Check standard (daily)**: a short guided measurement of a certified pin. Result error vs certificate is logged. Drift beyond
  the configured limit marks calibration **stale** (Run allowed but every row becomes CHECK; see §6).
- **Bias corrections** (per edge type and per fixture/orientation) are listed with: gage IDs used, date, configuration they were
  derived on. They are selected by an **explicit orientation/fixture setting saved in the program**, never assumed. Editing
  requires an unlock + reason entry.
- Calibration validity also records and locks **exposure time, gain and backlight level**. A change in any of them marks it stale.
- Records the **calibrated field region**. Edges measured outside it are flagged (§6).

## 3. Visual system

### 3.1 Color tokens (dark UI). Contrast ratios computed against stated backgrounds.

| Token | Hex | Notes / contrast |
|---|---|---|
| `bg-canvas` | `#000000` | image surround |
| `bg-app` | `#14171B` | window |
| `bg-panel` | `#1C2026` | right column, dialogs |
| `bg-row` | `#101215` | result rows |
| `row-hover` | `#181C21` | |
| `row-selected` | `#16283F` | with 3 px `accent` left bar |
| `border` | `#2D333B` | decorative dividers only |
| `border-control` | `#6B7480` | input/checkbox/button outlines: 3.45:1 on panel, 3.96:1 on row (WCAG 1.4.11) |
| `text-primary` | `#E8EBEF` | |
| `text-secondary` | `#9AA4AF` | 7.42:1 on row |
| `text-disabled` | `#5F6873` | disabled only |
| `value-neutral` | `#E8EBEF` | value colour when **no tolerance is set** (never green) |
| `accent` | `#3D8BFF` | 5.66:1 on row; focus ring 2 px (4.93:1 on panel) |
| `pass` | `#2FD65C` | 9.73:1 on row |
| `fail` | `#FF4D4D` | 5.74:1 on row |
| `warn` | `#FFB020` | 10.26:1 on row |
| `indicative` | `#9AA4AF` | dashed outline, no fill |

State chips: **filled** with `bg-row`-coloured text (`#101215` on pass 9.73, fail 5.74, warn 10.26) — never white on fill.
Pass vs warn have near-equal luminance, so **shape and word carry the state**: ● PASS, ▲ CHECK, ✖ FAIL, ◇ INDICATIVE.

**Image overlays** (drawn over a bright backlit field *and* a black silhouette, so colour alone cannot be legible):
- Every stroke and glyph gets a **1 px `#000000` casing** (2 px for text). Cores: edge `#00E5FF`, tool `#FFD400`, dimension text
  `#FFFFFF`. Black casing on `#E8E8E8` is ~17:1; the cores stay readable on the silhouette.
- Dimension text ≥ 13 px on screen. Lines 1.5 px at any zoom.
- **Edge-point dots are off by default**; shown only for the selected/expanded feature, decimated to ≤ 200 points on screen.
  Accepted edge points: solid `#00E5FF` core with black casing. Rejected points: **hollow circle**, `fail` core, black casing (shape, not colour, carries the difference).
- The tolerance zone is shaded 15 % state colour on the selected feature only.
- The image itself is never tinted.

### 3.2 Typography
- **Segoe UI** throughout, with `font-feature-settings` / Qt `QFont` set to tabular figures; verify in Qt and fall back to
  **Consolas** for numeric readouts if Segoe UI’s figures are not tabular in the build.
- Result value 32 px semibold, right-aligned on the decimal point; unit 14 px secondary; feature name 14 px; ID/metadata 12 px;
  floor 12 px.

### 3.3 Spacing and targets
- 8 px grid, 4 px radius, 1 px borders, no bevels or gradients.
- Primary actions (Measure, Confirm, Remeasure) 48 px high; secondary 40 px. Canvas handles: 32 px hit area, 10 px visual.

### 3.4 Iconography
1.5 px stroke, 20 px icons, always labelled in toolbars. Tool icons depict the geometry created.

## 4. Result display and decision logic

### 4.1 Result row (340 px column)
Heights: **non-thread feature 104 px** (lines 1–3 + bar + 8 px padding top/bottom, no worst line); **thread feature 124 px** (adds line 4).
There is no separate "collapsed" size; expansion (below) adds a detail block under the row. For **major/minor diameter** the large
value on line 2 is labelled **“avg (info)”** and coloured `value-neutral`; the verdict-driving min/max tooth values on line 4 carry
their own state icons and colours (principle 1: the biggest number must not imply a verdict it doesn't drive).

```
x:  0        24                                    256        340
y0  ┌ #  Major diameter (worst-of)            [▲ CHECK] ┐   row 1  (28 px)  index/name left, chip right (92×24)
y1  │     0.8675 ±0.0002   in                              │   row 2  (36 px)  value 32px, U 14px secondary, unit 14px
y2  │     22.034 mm  ·  −0.0015 vs max (USL 0.8690)        │   row 3  (20 px)  secondary: mm (3 dp), deviation from the named reference
y3  │     min 0.8641 ±0.0009 (t4) · max 0.8688 ±0.0009 (t7) ▲│   row 4  (20 px)  thread features only; own state icon
y4  └ [LSL ─────●──────────── USL] limits 0.8610 / 0.8690   │   bar    (12 px)  limits from LSL/USL; shapes not colours
```
- **Row state = the worse of average and worst-tooth states** (FAIL > CHECK > INDICATIVE > PASS).
- Value in `value-neutral` when no tolerance; otherwise coloured by state *and* accompanied by chip and bar marker.
- **Tolerance bar**: track = LSL…USL (an open-ended side drawn as an arrow if one limit is absent); marker shape = state
  (● ▲ ✖ ◇); an out-of-band value pins to the track end with an arrow. The U range is drawn as a thin whisker on the marker.
- Deviation is shown against a **named reference** (“vs max (USL)”, “vs min (LSL)”, or “vs basic size” when a basic size is set); never “nominal” for a limit.
- Decimals: round U **up** to 1–2 significant figures and show the value to the **same decimal place as U** (inch default max 4 dp, mm max 3 dp).
  Example: 0.8675 ±0.0002. If U is 0.0008, the value shows as 0.8675 → 0.868 ±0.001 only if the operator’s precision setting allows; otherwise 0.8675 ±0.0008.
- Worked state check for the mockup: avg 0.8675 ±0.0002 (limits 0.8610/0.8690) is PASS; min-tooth 0.8641 ±0.0009 is PASS; max-tooth 0.8688 ±0.0009
  spans 0.8679–0.8697 and crosses USL, so it is **CHECK** (▲) and drives the row state. The mockup shows exactly that.
  Rounding is for display only; conformance uses unrounded values (§4.4). If a displayed value equals a limit and the verdict is
  FAIL, the row shows the unrounded value in the expanded view and a “rounded” tag.
- **Expanded row** shows: raw value, **bias applied and its source**, tilt applied, edges kept / rejected, polarity, min_strength,
  smoothing σ, strength histogram (§4.7), per-tooth chart (crest/root radius vs axial position with limits), fit residual, U budget
  breakdown (scale, edge repeatability, bias residual, tilt).
- A **CHECK row with no usable value** shows “—”; with a low-confidence value it shows the value in `text-secondary` plus “unverified”.

### 4.2 States
| State | Icon | Colour | Meaning |
|---|---|---|---|
| PASS | ● | `pass` | value ± U entirely inside limits |
| FAIL | ✖ | `fail` | value ± U entirely outside a limit |
| CHECK | ▲ | `warn` | within uncertainty of a limit, **or** a trust problem (§6), **or** no value |
| INDICATIVE | ◇ | `indicative` | tolerance too tight for U (TUR < 4); number shown, no conformance |
| (none) | — | `value-neutral` | no tolerance defined |

### 4.3 Thread outputs and what drives the verdict
- **Major and minor diameter**: verdict from **every tooth**: the **minimum tooth is tested against LSL and the maximum tooth against USL** (a one-sided feature tests only its own limit). Both extremes are shown with tooth indices; the average is shown alongside.
- **Pitch diameter** is **calculated** from the measured (crest-truncated) major with the 60° basic form
  (“calculated, 60° basic form — ASME B1.1 / ISO 68-1”), is not independent of major, and is **informational by default**
  (excluded from the verdict unless opted in). It is **not equivalent** to the flank-based pitch diameter the reference products report.
- **Pitch / TPI**: shows *expected (from preset) vs detected*. If detected is within 5 % of 2× or 0.5× the expected pitch the
  feature is CHECK: “Pitch harmonic suspected (detected 2× expected).”
- Flank/thread angle: diagnostics only, no verdict.

### 4.4 Decision rule and measurement capability
- Default decision rule: **ISO 14253-1 default rule — guarded acceptance and guarded rejection with guard band w = U**: PASS only if value ± U
  is entirely inside the limits; FAIL only if entirely outside; otherwise CHECK “Within uncertainty of limit.” This exact name is printed on
  each report. (Simple acceptance, i.e. no guard band, is a different rule and is not the default.) Changing the rule is an admin action.
- U is an expanded uncertainty (k=2) combined from a stored budget: scale calibration, **reticle certificate uncertainty**, **residual of the
  field-dependent scale model**, edge repeatability (Repeat ×N history), **placement/reseat reproducibility**, bias residual (from Verify),
  tilt correction, pixel quantisation, and **part temperature** (default assumption ±5 °C; steel ≈ 11.5 ppm/°C ≈ 0.00006 in per inch).
  Until Repeat/reseat history exists the app uses a **conservative default U** (initial value ±0.0005 in, to be replaced after the first
  qualification study) and says so on the row (“default U”). **Worst-tooth values carry their own, larger U** (single-tooth).
- **Test uncertainty ratio** (two-limit features): if (USL−LSL) / (2U) < 4 the feature is **INDICATIVE** (“Tolerance too tight for this
  system”). **One-limit features have no TUR**: only the guard band applies, unless an admin sets a reference span.

### 4.5 Repeatability
**Repeat ×N** (default 10) has two modes. **Untouched** re-captures without moving the part and measures *instrument repeatability*
only. **Reseat** prompts the operator to remove and replace the part between captures (optionally across operators) and measures
*reproducibility*; only this mode may be labelled a “GR&R-style study”. Both show mean, range, σ per feature and flag **clusters**: two discrete
clusters raise CHECK with “Bimodal result — suspect a selection bug, not noise” (background §5.4). Untouched results feed the edge-repeatability
term of U; Reseat results feed the placement term.

### 4.6 Traceability (stored with every saved result and exported)
Timestamp · program name + version · Lot / Serial / Part ID / Operator · software version · calibration record ID · reticle
certificate ID · bias set ID + orientation setting · exposure/gain/backlight level · decision rule · for each feature: raw value,
bias applied, corrected value, U, state · the **averaged snapshot image** (lossless) and overlay geometry.

### 4.7 Edge transparency
Expanded rows (and a “Details” drawer) display polarity (dark→bright), min_strength, σ, edges kept/rejected, and a histogram of
real edge strengths so a threshold that silently halves a pitch is visible (background §5.2).

## 5. On-image overlays (rules)
- Image coordinates; clamped to the visible viewport (build-1 lesson); callouts ≥ 12 px from the silhouette.
- Default: selected feature in full, others as small `[n]` tags matching row indices; a toggle shows all.
- Pending tools dashed, confirmed solid. Overlay styling per §3.1.

## 6. Trust and error states

| Situation | Where / how |
|---|---|
| No camera at launch | full-canvas empty state: “No camera found.” + Retry + Diagnostics |
| Camera reconnecting | banner; silent retries first (transient GigE) |
| Never calibrated | banner ✖; Measure disabled; the live image may show pixel coordinates only, values read “— in” |
| Calibration stale (age, drift, exposure/gain/backlight changed) | banner ▲; every row CHECK “Calibration stale”; one-click to Calibrate |
| Backlight fault | banner; Measure disabled when the image-measured background is outside limits |
| Stale result (tool edited, or program changed after snapshot) | values switch to `text-secondary` (no opacity dimming), chips stay full strength, plus a “Stale — Remeasure” chip |
| Snapshot frozen | “Snapshot hh:mm:ss” badge on canvas |
| Edges outside calibrated field | CHECK “Edge outside calibrated region” |
| Part touches image border | CHECK |
| Too few scan lines found an edge | CHECK “Only 3 of 12 scan lines found an edge” (**primary** tilt/coverage guard) |
| Tilt above backstop | CHECK; the **applied tilt** is always shown in the expanded row. Tilt itself is normal and corrected (background §5.4) |
| Save/export failure | inline error with path + reason |

Banner placement: the 40 px **status banner** sits above the canvas (canvas column); the 48 px **verdict banner** is the first item in the right column under Measure.

### 6.1 Illumination check (image-based, not controller-based)
The DCS-100E reports only its own setpoint. “Backlight check” analyses the **image**: background gray-level percentiles (5th/50th),
saturated-pixel fraction near each tool’s edge, and uniformity across each tool’s ROI. Defaults (to be tuned on real captures):
median background 150–230 of 255, saturated fraction < 0.5 %, ROI 5th/95th percentile ratio ≥ 0.85. The reference backlight is
visibly non-uniform (`segment_02/frames/0011_00m21s_t.png`), so uniformity is checked per ROI, not globally.

## 7. Interaction rules
1. No live-frame repaint during drags.
2. Shortcuts use `Qt.WindowShortcut` and are **suppressed while a text field has focus**: **F5** Measure, **F6** Remeasure,
   **Esc** cancel pending edit, **Ctrl+Z** undo, **+/−** zoom, **F** fit. Action buttons are `NoFocus` so Space cannot trigger
   “Start over”; Space is reserved for pan.
3. Tab order: Program picker → mode panel top to bottom → status strip. Focus ring 2 px `accent`.
4. Start over (Run and Set up) = instant; the **Undo toast persists (min 10 s) until the next Measure or destructive action**, and Ctrl+Z also works. Deleting a program = modal confirm. Editing a confirmed tool returns it
   to pending.
5. Units toggle changes display only.
6. **Numeric input** accepts `0.875`, `.875`, `7/8`, `1-1/4`, `1 1/4`, `22.2mm`, and thread designations (`1/4-20 UNC-2A`,
   `M12x1.75-6g`); the parsed decimal is echoed beside the field; ambiguous input is rejected inline with the reason.

## 8. Out of scope
Master-library / auto-alignment workflow, robot or pass-fail output, data logging beyond CSV export and saved result records,
multi-user permissions.

## 9. Changes from v1
(v2 scores: metrology 7/10, UX 7/10. A further round fixed their remaining spec bugs: decision-rule name, TUR for one-limit features, expanded U budget,
reseat mode, calibration criterion as a length error, row heights, stale/never-calibrated states, undo persistence, overlay point styling. Those fixes were **not** re-scored.)
- Added uncertainty, decision rule with guard band, TUR / Indicative state, capability disclaimer (metrology 1, 2, 4).
- Replaced ± tolerance with LSL/USL (metrology 3; UX 4).
- Defined verdict source per thread output, relabelled calculated PD, added pitch-harmonic check (metrology 5, 6).
- Reframed tilt: coverage is the primary guard, angle a backstop (metrology 7).
- Calibration traceability, check standard, verify step, bias selection by saved orientation, field region, exposure/gain lock (metrology 8).
- Image-based illumination check (metrology 9). Edge transparency (10). Repeat ×N (11). Traceability record + Lot/Serial/Operator (12).
- Overlay casing, edge-dot policy, text size (UX 1). State shapes and chip text colours, new tokens, contrast figures (UX 2, 3).
- Pixel grid for the result row, worse-of state, deviation field, own worst-tooth line (UX 4).
- Program management, Start over semantics, button placement, banner priority, new states, verdict aggregation (UX 5–7).
- Keyboard conflict fixes, target sizes, input parsing, concrete font (UX 8–10). Calibration criteria (UX 11).
