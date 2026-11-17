# DESIGN_REFERENCE.md — Commercial vision-metrology UI references

Source: `reference/videos/2026-11-07 10-58-46.mp4` (344 s, 1920×1080 @ 60 fps), a screen recording of a browser playing
YouTube videos back to back. Split into `reference/videos/segments/segment_01..05.mp4` (transition/browser-chrome gaps dropped).
Frame citations are relative to `reference/videos/segments/`, e.g. `segment_02/frames/0011_00m21s_t.png`.
Filename suffix `_s` = scene-change frame, `_t` = timed sample (every 2 s). Timestamps are segment-relative.

**Read this first — what the stills can and cannot tell us**

- Only **segment 02** is a clean, direct screen capture (Keyence IM-X software, full 1920×1080 UI). Everything else is
  a **camera filming a monitor** (segments 01, 03, 04) and then re-compressed by YouTube. Hex values from those frames
  are tinted by the monitor, camera white balance and glare. They are indicative only.
- Segment 02's capture was itself heavily compressed (it is a clip from a promo video). Fine text is readable at
  full size but is not pixel-exact.
- Still frames lose all timing. Motion/animation notes are inference, flagged as such.
- Transcripts were produced by Whisper `small.en` on CPU. Brand names and the presenter's name were misheard
  ("Kiont's", "Mark Sacco"); on-screen text says **Mark Sawko** and **Keyence IM 7020**, and I use those.
- Segment 05 is a repeat of segment 03 (same M3 video, slower pacing). It adds nothing new and is not cited below
  unless noted.

---

## 1. Per-segment summary

| Seg | Source | Software shown | Notable |
|---|---|---|---|
| 01 (0:00–1:17) | Kremin Inc. "Verifying Gauge Accuracy with the Keyence" quick tip | **Keyence IM-7020** vision system, touchscreen on the head of the machine | The only step-by-step *authoring* workflow: pick Wide-field vs High-precision, center part, autofocus, run **Thread** tool by drawing one line down the thread axis, tick Major/Minor/Pitch, OK. Result overlay labelled on the part. Presenter checks 0.3344" against the gage's stamped PD .3344 |
| 02 (1:17–1:51) | "Our New Keyence Thread Inspection Machine", Rolled Threads Unlimited | **Keyence IM-X Series** PC software, **Run** screen, direct capture | The best single reference for a production *run* screen: huge numeric readouts with OK badges, three parts in one field of view, dimension callouts drawn on the silhouette, Next / Paused controls. Also shows 3 gage pins on a glass stage with a green indicator behind them (see hardware frames) |
| 03 (1:52–2:41) | Mitutoyo "Thread Measuring Made Easy with M3" | **Mitutoyo M3** on a QuickImage, touch UI | Best *part interaction*: "touch thread, touch the thread edge, done". Result card with O/R/P/L rows, tolerance table that auto-detects M12×1.75 and only needs a class (6g), and a **Deviation view** with out-of-tolerance value in red |
| 04 (2:45–4:30) | Kremin Inc. "Keyence IM Series Vision System" review | Keyence IM touchscreen: Home menu, Program screen, Run screen | Home menu with large tiled launcher; results panel in close-up; reviewer's list of caveats (small viewing window, black-part contrast, cost, scratched glass stage). Mostly talking head; limited UI frames |
| 05 (4:32–5:44) | Same Mitutoyo video as 03 | M3 | Duplicate of 03, slower. Used only for cross-checking |

---

## 2. Screens / views inventory

### 2.1 Keyence IM-X — Run screen (the reference for a production readout) — segment 02
Frames: `segment_02/frames/0011_00m21s_t.png` (clearest), `0012_00m23s_t.png`, `0013_00m25s_t.png`, `0015_00m29s_t.png`.

Layout regions (1920×1080 capture):
- **Title bar**: "KEYENCE IM-X Series", standard Windows chrome.
- **Left rail (~255 px)**: "Elements Tree" with Category selector and a Group1 list; green on/off pill. Light gray `#E0E0E0`.
- **Top toolbar** over the image: hamburger "Menu(M)", file/print icons, zoom "82%", fit/zoom icons, delete/undo/redo,
  measurement-axis icon. Light, flat, icon-only, with a small dropdown caret on some.
- **Image canvas (center, ~1300 px wide)**: pure black field with the backlit scene as a light-gray disc cluster
  (the stage glass illuminated in overlapping circles) and parts as **black silhouettes**. Three parts shown at once.
  The first part carries dimension annotations; the other two have none yet (consistent with run-in-sequence).
- **Right results column (~320 px)**: top blue **"Run"** header + "To Main Menu"; program name dropdown; Lot Number /
  Serial Counter / Name text inputs; "Display Result" with up/down/Delete and a "Re-measurements" button; then a
  **scrolling list of result rows**; below it two checkboxes ("Zoom display", "Display results in the descending order
  of errors"); then bottom action row.
- **Bottom of right column**: big **Next** button (gray) and **Paused** button (dark teal with green/cyan text);
  below that a text row "Delete Result | Print | Save Report".
- **Bottom toolbar** under the canvas: layer/3D/camera/scissors/image icons on the left, a run/stop/measure group on the right
  (green play, red stop square, teal toggles).

**Result row anatomy** (rows 1–9 visible in `0011_00m21s_t.png`): row number at left → small feature ID (e.g.
`MAJORD001`, `PITCHDIA001`, `MINORD001`, `LN-LN002`) in small white text → large green value with small "inch" unit →
thin **tolerance slider track** with a tick marker under the ID line (shows where the value sits in the band) → at
the right a dark-blue block with large green **OK**. Rows are black with white/green text; the OK block is a deep navy.
The rows are tall (~62 px) and the value text is ~30 px — readable from arm's length.

**On-image annotations**: dimension lines in green text on the silhouette, dashed red leaders, short yellow edge-probe
segments at each measured edge, yellow circles for circle features, small `[n]` index labels matching result-row
numbers. Text is rotated 90° along vertical dimensions. See `0011_00m21s_t.png`; at this scale the annotation text
on the silhouette is dense and partly overlapping on the first part — **this is a clutter problem worth avoiding**.

### 2.2 Keyence IM — thread measurement in close-up — segments 01, 04
Frames: `segment_04/frames/0019_00m32s_s.png` (readable), `0014_00m21s_s.png`, `0035_01m22s_s.png`,
`segment_01/frames/0024_01m01s_s.png`.
- Image background is a blue-gray (this unit uses a different illumination than segment 02; looks diffuse/blue-tinted),
  part black. Dimension text green; probe lines yellow/red; coordinate axes drawn as a **green Y axis and red X axis with arrowheads** from a green × origin marker.
- Results list on the right in black rows, a green value, a tiny white feature label (16a, 16b, 8a, 8b, 11) and the same
  tolerance slider and navy OK badge. Bottom: "Measure" (gray) and a large "OK" (blue with green text), plus
  "Delete Result | Print | Save Report".

### 2.3 Keyence IM — thread tool authoring — segment 01
Frames: `segment_01/frames/0021_00m53s_s.png` (thread tool open, light-blue line drawn down the thread axis),
`0023_00m57s_s.png` (options panel), `0020_00m48s_s.png` (close-up of silhouette with panel at right),
`0019_00m45s_s.png` (Program screen: icon palette column on the right of the image).
- The thread tool opens a **side panel with numbered procedure text** ("1. Click starting and ending points…
  2. Adjust the width by dragging. Click [Apply]. Click [OK]") and a **checkbox list** of measurements: Major diameter,
  Minor diameter, Pitch, Thread angle, Pitch diameter, Partial cumulative pitch deviation, plus tolerance settings.
- On the image: a thin cyan/blue axis line down the shaft, a translucent teal **band** (see `0024_01m01s_s.png`: green-teal
  box with `MAJORD001`, `PITCHDIA001`, `MINORD001` rotated labels at the box edge) showing the edge search width. The
  box is draggable (per narration: "expand the area to include the entirety of the thread").
- Small text in this panel is **illegible** at the camera's resolution beyond the headings quoted above.

### 2.4 Keyence IM — Home and Program palettes — segments 01, 04
Frames: `segment_04/frames/0038_01m30s_s.png` and `0037_01m27s_s.png` (home), `segment_01/frames/0009_00m22s_s.png`
(home, softer), `0011_00m26s_s.png` (capture-setup dialog: "1. Select camera: Wide field / High precision; 2. specify a
capture size; 3. specify a capture position / interval"), `0013_00m30s_s.png` (floating **Z-Control** palette).
- **Home**: dark blue gradient background, six large tiles in a 2-col layout: Run, Program, Statistics/Analysis,
  Help (small circle), Single Measurement, Automatic Measurement. Each tile has a flat orange/white icon at left and
  one label. A thin orange divider with "IM series" runs across the middle. Footer: Combine multi-Programs / Security
  settings / Optional Settings / power icon. **Whole screen is designed for finger touch.**
- **Capture setup dialog**: a blue segmented toggle labelled Wide field | High precision, a square preview diagram
  showing the capture area inside the stage, and a bottom thumbnail strip of capture layouts. Mixed light-gray dialog on
  the dark app background.
- **Z-Control palette** (`0013_00m30s_s.png`): a narrow floating panel with a green numeric readout (e.g. `1.21039`),
  a unit line, a vertical list of stored Z-heights, up/down arrows, and an **"AF"** autofocus button in blue.

### 2.5 Mitutoyo M3 — live image with a thread tool — segment 03
Frames: `segment_03/frames/0008_00m11s_t.png` (empty live view), `0011_00m17s_t.png` (yellow points panel appears),
`0012_00m19s_t.png` (thread edge traced in orange-red), `0013_00m21s_t.png` (point count 1652), `0014_00m23s_t.png`
(result panel on left), `0015_00m25s_t.png` (touching the screen).
- **Image background is a saturated green** with the part black: this is the backlight color of that machine, and the
  M3 UI treats it as just image content. Visible overlay: **orange-red edge trace** along the crest/root profile of the
  thread (`0012_00m19s_t.png`).
- **Left panel** stack: `X / Y / Q` live coordinate readout (`98.63884`, `44.97630`, `0°00'00"`) → on touch it becomes a
  **yellow "Points" counter** (`0` → `1652`) → on completion the panel shows `O / R / P / L` values (`11.80492`, `9.64750`,
  `10.66901`, `1.74660`) in white on dark blue. Counter turns yellow at the active tool.
- **Right strip**: "No features" text when empty, then a `Thread 1` item in a vivid blue highlight bar.
- **Bottom right**: **Done** and **Delete** as large, flat text buttons; a tool-icon strip across the bottom.
- Top toolbar: home, video, crosshair, table icons, "Norm" mode selector, units "mm", help "?".
- Small text in the live-view left panel (secondary rows under the O/R/P/L values: `c f LSBF Ref 1T Pts 1429 of 1652`)
  is too small to read confidently.

### 2.6 Mitutoyo M3 — Feature Detail / tolerance and deviation — segment 03
Frames: `segment_03/frames/0017_00m29s_t.png` (empty tolerance card: "Apply tolerances using the tolerance buttons below"),
`0020_00m35s_t.png` (tolerance table filled: `- Tol` / `+ Tol` columns), `0021_00m37s_t.png` (Actual view),
`0024_00m43s_t.png` (Deviation view).
- Gray-blue backdrop with a **single large raised card** containing: a title toggle pair at the top (`Thread 1`,
  `Tolerance / d / a / n`), Size (M12), Pitch (1.750000), Class (6g) selectors, then a 4-row table **O R P L** with
  a leading letter chip on each row and numeric text fields in white with beveled borders.
- **View switcher at the bottom**: four letter buttons **O R P L** and a thread icon, alongside Done / Delete.
- **Actual** (`0021_00m37s_t.png`): O/R/P/L rows with big black numerals (e.g. `11.80492`, `9.64750`), a thread
  profile schematic at the left, and small secondary diagnostic lines (`c`, `f`, `r`, `LSBF`, `VED`, `Pts 1429 of 1652`).
- **Deviation** (`0024_00m43s_t.png`): same layout, values replaced by deviation from nominal. **Out-of-tolerance
  value is red** (`-0.19434` on P); others black. In-tolerance and out-of-tolerance are therefore distinguished by a single text-color change — and no other cue was visible (no badge, no band), so I would not copy that choice.
- Hardware observation: this UI is **Windows-style skeuomorphic** (bevels, gradients). It is explicitly the least modern of the three.

### 2.7 Keyence IM — Statistics/Analysis, Program list, other
Not shown. Statistics/Analysis exists only as a Home tile. Not covered by the footage.

### 2.8 Hardware context frames (not UI)
- `segment_02/frames/0001_00m01s_t.png`–`0009_00m17s_t.png`: Keyence IM-X with three gage pins on a glass stage; the
  stage shows a green indicator patch under the pins. Colorful "Run" indicator is a **blue LED strip on the head**
  (`0004`–`0008`).
- `segment_04/frames/0028_01m03s_s.png`: a small pin on the stage glass; `0043_01m36s_s.png` shows the joystick/dial
  control cluster labelled MEASURE / PRINT / OK on the IM-series base (`0009_00m12s_s.png`).

---

## 3. Component catalog

| Component | Where | Visual characteristics |
|---|---|---|
| **Result row** | Keyence Run, `segment_02/…/0011_00m21s_t.png` | Full-width black card, ~62 px tall. Index number, small ID text, big green value + small unit, tolerance slider with tick, navy OK block. The value is by far the largest element |
| **OK badge** | Keyence | Navy `#002848`-ish block at row end, huge green "OK", same height as row. No NG frame visible in this footage (**NG appearance unknown**) |
| **Tolerance slider** | Keyence rows | Hairline white track with 2–3 tiny tick marks and a white dot; illegible detail at this resolution — meaning of ticks (nominal / limits / value) is inferred |
| **Primary action button** | Keyence Run: `Run` header blue `#4080D8`; M3: `Done` | Keyence uses a blue header "tab" for mode, gray `Next`, teal `Paused`. M3 uses flat gray text buttons |
| **Mode toggle** | Keyence capture dialog: Wide field / High precision | Segmented control, active segment blue |
| **Text input** | Keyence Lot Number / Serial Counter / Name | White field, thin blue-gray border, label to the left |
| **Floating palette** | Keyence Z-Control (`segment_01/…/0013_00m30s_s.png`) | Narrow, light, with a green LCD-style readout and an AF button |
| **Icon toolbar** | Keyence top/bottom, M3 top/bottom | Small flat icons, no labels; Keyence is monochrome gray with colored accents; M3 is monochrome with a few blue accents |
| **Point counter** | M3, `0011_00m17s_t.png` | Yellow panel, "Points" label, counts up while the thread edge is traced |
| **Result card** | M3, `0021_00m37s_t.png` | Raised light card on gray-blue, letter chips, large numerals, small gray subscripts |
| **Feature list item** | M3 right strip | Highlighted "Thread 1" in vivid blue |
| **Dialog / procedure panel** | Keyence thread tool, `segment_01/…/0023_00m57s_s.png` | Docked right panel with numbered instructions, checkbox list, OK/Apply/Cancel |
| **Dimension callout on image** | Keyence | Green text, red dashed leader, rotated for vertical dims, `[n]` index tag matching the results list |
| **Edge-probe indicator** | Keyence | Yellow short segments at each measured edge and circles for features |
| **Edge trace** | M3 `0012_00m19s_t.png` | 2-3 px orange-red line hugging the silhouette over the whole thread |
| **Status cue** | Keyence Paused/Run/Next; M3 "No features" | Words in a colored block; no iconography |
| **Home tile** | Keyence Home | Dark tile, line icon left, label right, ~150×55 px at 1080p |

---

## 4. Visual style

### 4.1 Palette (sampled)
Sampled with a script on 1920×1080 frames, quantized to 8-level bins; "(direct)" = clean capture, "(photo)" = camera
photo of a monitor, indicative only.

| Role | Hex | Frame / region | Source |
|---|---|---|---|
| Image canvas, empty / outside stage | `#000000` | `segment_02/…/0011_00m21s_t.png` (300,120) | direct |
| Part silhouette | `#000000`–`#080808` | same (930,570) | direct |
| Backlight field (disc) | `#A0A0A0` | same (700,780) | direct |
| Result row background | `#101010` | same (1640,440) | direct |
| **Result value green** | `#27CE30` (brightest sample `#1EFF1D`) | same, row 1 value | direct |
| OK block background | `#002848` (deep navy) | same (1850,330) | direct |
| "Run" header blue | `#367FDC` / `#4080D8` | same (1610,55) | direct |
| "Paused" button bg / text | `#404850` / `#05BDC3` | same (1775,970) | direct |
| "Next" button | `#787878` | same | direct |
| Left rail / panel gray | `#E0E0E0` | same | direct |
| Toolbar / form light | `#F8F8F8` | same | direct |
| On-image dimension text | `#22A21E` (dim) – `#1EFF1D` (bright) | same (910,280) | direct |
| Red dashed leaders | `#860608` | same | direct (dark red is a compression artifact; visually reads `#E02020`-ish) |
| Keyence image backdrop (other unit) | `#8088A8` | `segment_04/…/0019_00m32s_s.png` | photo |
| OK badge, other unit | `#001860` | same | photo |
| Keyence Home background | `#283050` | `segment_04/…/0038_01m30s_s.png` | photo |
| M3 backlight green | `#60C860` | `segment_03/…/0012_00m19s_t.png` | photo |
| M3 points-counter yellow | `#909830` (photo dim; visually `#E8E040`-ish) | `segment_03/…/0012_00m19s_t.png` | photo |
| M3 thread-highlight blue | `#0050F0` | `segment_03/…/0020_00m35s_t.png` | photo |
| M3 tolerance card | `#90B8E8` | same | photo |

**Observations:** the three products converge on the same palette logic — **neutral gray/black chrome, saturated
green for good values, red for failures, blue for mode/selection, yellow for "work in progress"**. Keyence puts its
readouts on a *black* strip with *green* numerals (high contrast), M3 puts dark numerals on a *light* card.

### 4.2 Typography
- Keyence Run results use a **proportional sans** (looks Segoe UI/Tahoma-like) at roughly 30 px for the value, ~12 px for the ID and ~11 px for unit text at 1080p. Values appear right-aligned with a fixed 4 decimals, so `0.8675` / `0.8249` / `10.3785` line up on the decimal within compression error (rows 1, 2, 7 in `0011_00m21s_t.png`); I first read this as misaligned, and a reviewer corrected me. The real weakness is the tiny ID and unit text.
- M3 uses a large light-weight sans for the numerals (~50 px on the camera frame), small gray subscripts.
- No monospace anywhere that I could see. Weight hierarchy: value heavy, label light.
- Sizes quoted are measured from the frame, not specified by the vendor.

### 4.3 Density and spacing
- Keyence Run: dense. Nine+ result rows plus a form plus action row in 320 px width. Row padding is tight; text is large relative to the row.
- M3: sparse. One card, four rows, large touch targets.
- Both put the **image first** (70%+ of width) and the numbers in a fixed side column.

### 4.4 Borders / shadows
- Keyence: flat panels, 1 px light borders on inputs, no shadows. M3: bevels and gradients, raised card with drop shadow.

### 4.5 Iconography
- Keyence: small monochrome pictograms, thin strokes, accent color on active tool. The Home screen uses flat orange/white icons on dark tiles.
- M3: monochrome silhouette icons, mixed stroke weight (looks dated).

---

## 5. Interactions and workflows (inferred from frames + narration)

### 5.1 Keyence IM: one thread, one line (segment 01 + transcript)
1. Home → **Program** (`0009_00m22s_s.png`).
2. Capture setup dialog: choose **Wide field** or **High precision** (`0011_00m26s_s.png`). Presenter picks High precision.
3. Center the part; use **AF** in the Z-Control palette for focus ("sometimes it works well, sometimes not, but at
   least usually gets you pretty close", transcript 00:31–00:37) and fine-tune.
4. Select the **Thread** tool; draw a single line down the part axis, "staying inside the first thread and ending before the last thread" (00:47–00:53).
5. Drag to widen the band so it spans the whole thread; Apply (`0021_00m53s_s.png`, `0024_01m01s_s.png`).
6. Tick Major / Minor / Pitch diameter; OK (`0023_00m57s_s.png`).
7. Results appear as overlay labels + list; compare pitch diameter (0.3344") to the gage's stamp.

### 5.2 Keyence IM-X: batch run (segment 02)
1. Run screen with program name + lot/serial inputs (`0011_00m21s_t.png`).
2. Multiple parts are in the field of view; each is measured in the same pass.
3. Per-feature OK/NG. A **Next** button advances to the next part/serial and **Paused** indicates run state.
4. Delete Result / Print / Save Report as end-of-run actions.

### 5.3 M3: one-touch thread (segment 03, transcript 00:15–00:23)
1. Tap **Thread** tool.
2. Touch the thread edge **once** (finger on the crest line at `segment_05/frames/0018_00m29s_t.png`; the repeat segment has the clearest hand shot).
3. Software traces the edge automatically (points counter climbs to 1652), then **Done**.
4. Results card appears with O/R/P/L.
5. Switch to **Tolerance** → software has already identified **M12 × 1.75**, user adds class (6g) → tolerances fill in.
6. Switch to **Deviation** → out-of-tolerance value turns red.

### 5.4 Pitfalls the reviewers named (segment 04, transcript)
- Viewing window is small → parts must fit (`0027_00m59s_s.png`, `0028_01m03s_s.png`).
- Contrast: "color of the part… make sure you know what color the parts are" (`0032_01m12s_s.png`).
- Autofocus is "sometimes it works, sometimes not".
- Stage glass scratches (`0039`–`0042`, `01m31s`).

---

## 6. Motion / animation notes — UNCERTAIN (stills lose timing)

- Segment 03 `0011_00m17s_t.png` → `0013_00m21s_t.png`: the yellow Points counter appears and counts up; the traced edge is
  drawn in orange-red. I cannot tell whether the trace animates progressively or appears at once.
- Segment 02 `0017_00m31s_s.png`, `0018_00m32s_s.png`: transitions at the very end are the editor's, not the software's.
- Segment 04 circle-wipe / white-flash frames (`0003`, `0008`, `0017`, `0018`, `0025`, `0039`–`0042`) are the **video editor's** transitions. Ignore them.
- Keyence "Paused" teal button vs run state suggests a color change on state; no frame pair proves it.

---

## 7. Candidates for the telecentric-comparator app

(Hardware: Basler acA2440-20gm 2448×2048 mono, bi-telecentric 0.114×, blue 455 nm backlight, DCS-100E controller, PT-series chessboard reticle.
Outputs inches primarily, per `PROJECT_BACKGROUND.md` §9.)

1. **Black canvas, silhouette as content, UI chrome recedes.** (Keyence Run). Our image is a *mono* camera with a blue
   backlight, so parts are black on bright; Keyence's black outer field with a gray backlit disc is the right reference.
   Do not colorize the image the way M3 inherits a green backlight.
2. **Right-hand results column with one big number per row.** (`segment_02/…/0011_00m21s_t.png`). Large green value,
   small unit, ID above, state block at right, tolerance slider under the ID. Add what Keyence lacks: **tabular figures**
   and a **second (mm) line** in smaller type, per `PROJECT_BACKGROUND.md` §9.
3. **Index tags that connect an on-image callout to a result row.** (`[n]` in Keyence). Keep it; avoid Keyence's dense overlapping labels (limit to
   the selected feature, show the rest on hover).
4. **Thread workflow = one axis line + draggable band + checkbox of outputs.** (`segment_01/…/0021`, `0023`, `0024`). This matches the
   background doc's lesson that a placed tool should be an **editable, unconfirmed suggestion** before measuring.
5. **Live "Points" counter during trace.** (M3 `0011`–`0013`). A cheap, honest progress cue during the 8-frame averaged capture.
6. **Pre-filled tolerance from detected thread size; user only picks class.** (M3 `0018`–`0020`). Good fit for gage-pin / thread-gage work.
7. **Three-way view switch: Actual / Tolerance / Deviation** (M3 `0021`, `0017`, `0024`). Deviation is the useful QC view; add a non-color cue.
8. **Mode tabs: Run vs Program.** (Keyence). We have two real activities: set up a part, then run it.
9. **Persistent, explicit "Remeasure" and "Delete result".** (Keyence Run). Matches the background doc's "start over" lesson.
10. **Worst-value readout next to the average** — *not* shown in any reference; it is a headline requirement of ours, so it has to be invented.

Do-not-copy list:
- Keyence's overlapping on-image text on the first part (`0011_00m21s_t.png`, left part).
- M3's color-only fail cue (red text, `0024_00m43s_t.png`).
- M3's skeuomorphic bevels (`0020_00m35s_t.png`).
- Keyence's tiny ID/unit text in the result rows.
- Keyence's mid-gray field (`#A0A0A0`) and green/red overlay colors: our backlight is brighter, so overlays need a black casing.
- Keyence's tiny unlabeled icon toolbar for an operator on a shop floor.
- Autofocus: our lens is telecentric on a fixed stage; no equivalent needed.

---

## 8. Open questions / unclear

1. **NG appearance on Keyence** is never shown (all rows are OK). Unknown: color, flashing, sound, whether the whole row inverts.
2. **Tolerance slider meaning**: tick marks are too small to confirm they show limits vs nominal vs measured value.
3. **Thread tool diameter result semantics** on Keyence: whether the readout is averaged across teeth or the worst tooth. Not stated.
4. **Statistics/Analysis, Help, Single/Automatic Measurement** screens are never shown.
5. **Color of selection/hover states** in all three products: not observed.
6. **Small text in the Keyence thread panel** (`segment_01/…/0023_00m57s_s.png`) beyond the headings quoted is illegible.
7. **Exact fonts**: guessed from glyph shapes only.
8. **Whether the Keyence yellow-circle/red-dashed annotation scheme is configurable.**
9. **Units shown by default on M3** are mm; the unit chip in the top toolbar is "mm"; inches option not shown.
10. **Segment 02's 0:23–0:33 frames** show the same screen with different values (0.8680 → 0.8673) across consecutive
    parts; I cannot tell whether the display refreshes live per part or after pressing Next.
11. **Segment 05 duplicate**: I did not read its transcript sentence-by-sentence against 03; it looks the same but I only compared the first minute.
12. **YouTube-compressed text**: any number quoted in this file is read from a compressed frame; verify before using it as a spec.
