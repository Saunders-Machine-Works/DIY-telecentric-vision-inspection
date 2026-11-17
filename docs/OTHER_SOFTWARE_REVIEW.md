# Review of the two third-party repositories (2026-11-14)

Folder `reference/software/` (local only, not in git) holds **Loupe** (Apache-2.0, Quintin Smits; FastAPI + vanilla JS) and
**Measurific** (MIT, Kyle Bartholomew; Electron/React + Python + C). Both were read as data only:
nothing in them was run or installed. Findings below come from two read-only review passes; the
reviewers did not read every file (Measurific: 586 files, Loupe: 503), so "not found" means "not
found in what was read".

## Headline
- Neither repo has what our core already has: a reticle field-map calibration, pitch diameter
  from flank fits with the helix raise removed, per-tooth defect detection, or thread standards
  tables. Measurific states that pitch diameter, lead and helix are "not inferred".
- Their validation is synthetic only (software-limit pixel bounds); our certified-pin checks are
  stronger. Neither repo has an uncertainty budget.
- What is worth taking: test methodology, an edge-bias study, camera practice, result/tolerance
  schemas, UI structure. Ideas re-implemented in our own code carry no licence obligation; a
  close port of code needs the MIT / Apache-2.0 notices (Apache-2.0: also a statement of changes
  and the NOTICE attribution).

## Acted on already
| Item | Source | Result |
|---|---|---|
| Analytic blurred-edge bias test | Measurific `research/synthetic-esf/THRESHOLD_ACCURACY.md`, Loupe `tests/test_metrology_accuracy.py` | `tests/edge_bias.rs`. Found our 4-point Catmull-Rom crossing left up to 0.031 px (0.9 um) width error at blur sigma 1.0 and 0.075 px (2.3 um) at sigma 0.6 for diameters of arbitrary fractional width. Replaced by a local natural cubic spline: 0.0066 px at sigma 1.0 (matches their published cubic-spline figure), 0.037 px at 0.6. |
| Camera linearity | Measurific `camera-bridge/README.md` | `examples/camera_linearity.rs` (read-only): Gamma off (1.0), LUT off, black level 0, digital shift 0, auto exposure/gain off, PGI off, Mono8. Linear. |
| Curvature bias of a 50 % diameter | same study | -0.0089 px at r = 50 px, -0.0044 at 100, -0.0018 at 250 (sigma 0.6). Our pins are r = 100 to 200 px: under 0.2 um, negligible. |

## To do, in order
1. Done: live A/B on the 0.5009 pin shows the spline change moves the reading by 0.1 um. Open: per-edge local levels (docs/experiments/local_edge_levels.patch) once the illumination is flat.
2. Quality gates in `thread_teeth` / `thread_flank` (Loupe `geometry.py` Huber line fit with
   condition-number and coverage gate; `guided_inspection.py` ambiguity rule: two supported edge
   clusters means flagged, never a silent PASS).
3. Cross-checks of the thread numbers (Measurific `threads.py`): pitch from same-direction flank
   crossings at (P10+P90)/2 with a 0.15 hysteresis band; crest/root envelope diameters;
   cumulative axial-reversal guard (> 2 x median step); two-scale corner test for crest/root
   corners (`corners.py`, exclude about 3 px around corners).
4. Result schema (status / value / method / nominal / deviation / tolerance / pass_fail / witness
   points / unavailable-reason) and `Spec {nominal, upper, lower}` with PASS / WARN / FAIL;
   add an ISO 14253-1 style guard band from our own validated uncertainty (neither repo has one).
5. Camera exposure rule (ROI only, ratio = min(0.9 / P99.9, 0.98 / peak), x0.8 if clipped,
   deadband 0.98 to 1.02) and a live focus score (Sobel, noise-corrected, 200 ms EMA, peak hold).
6. App/UI: central zoom/pan image stage, top toolbar, right inspector with results table; drawing-
   style dimension callouts constant in screen pixels; `[n]` numbering shared by overlay and
   table; verb-first Measure palette; part recipes as versioned serde JSON with migration;
   calibration records as timestamped JSON, never overwritten. Thread-pitch ghost overlays
   (Loupe `reticles/thread-*`: sharp-V zig-zag line pairs; no M5-M12 or #10-1/2 sizes).
7. GigE practice (Loupe `backend/cameras/aravis.py`): jumbo-frame packet size, receive buffer,
   dropped/resent packet counters (pylon on Windows differs; apply the lessons only).

## Not useful for us
Deflectometry, fringe/interferometry, Zernike, Z-stack/roughness, stitching, SQLite run store,
hosted-mode code, Electron packaging, all ML edge/contour models (synthetic-only, 0.07 to 0.17 px
outward on blurred edges per their own README), the PAE sub-pixel detector (their own
investigation shows it invalid at sharp corners and wide blur; 0.2 px targets), Loupe lens and
tilt calibration (weaker than our field map; a telecentric lens needs no perspective correction),
DXF-guided inspection (kept in mind for a later "compare to drawing" feature).

## Cautions found in their documentation
- Loupe default tolerances (warn 0.10 mm, fail 0.25 mm) are far too coarse for us.
- Loupe's edge snapping uses the gradient peak, not a 50 % crossing; the two differ for skewed
  edge profiles. We keep the 50 % crossing.
- Measurific's worst sampled outline error is 0.3 to 1.2 px at sharp corners (unsolved).
- Measurific demosaicing/sRGB warning does not apply to our mono camera (verified above).
