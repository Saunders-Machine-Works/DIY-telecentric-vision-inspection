# Progress

## Done
- I-0: toolchain (Rust 1.99, MSVC Build Tools, Git, Pylon 7.5 SDK), workspace, kit, hooks, Commands table.
- I-1: `camera` crate. Hand-written pylon C bindings, persistent stream grabber, 8 pre-registered buffers,
  fresh acquisition per grab/burst, strict frame validation (ROI must equal configured OffsetX/Y), locked
  exposure/gain/trigger, whole-burst retry, u16 accumulator. Reviewer scores 8/10 quality, 8/10 speed.

## Measured on the real camera (acA2440-20gm, release build)
- open 185 ms; single grab 52 ms; 8-frame burst 383 ms (~21 fps = gigabit wire limit at 1500-byte packets).
- Sensor 2464x2056; factory ROI is centered 2448x2048 at offset (8,4). Exposure 3000 us, gain 0.
- Two untouched 8-frame means differ by 0.82 gray levels RMS (single-frame noise about 1.6 levels).
- Lens image circle is smaller than the sensor (dark corners). Peak brightness ~210/255.

## Open (from the last review, not yet done)
- Burst loop (requeue when n > 8, duplicate block id, restart) is not unit-tested: put it behind a small trait.
- set_exposure_us falls back across node names and can hide an out-of-range error; pick the node once at open
  and return the read-back value. Tie the grab timeout to exposure + 500 ms.
- Jumbo frames (MTU 9000) would cut the burst to ~350 ms; needs an adapter setting on this PC.

## Next
- I-4 periodic edges + reticle scale calibration, then I-5 accuracy test on the 1/2-13 GO gage.

## I-3 notes (edge core)
- Spline sampler matches scipy map_coordinates(order=3, mode="nearest") to ~3e-12 inside/near the image
  (5e-7 only 100+ px outside the image, where no tool samples). Golden vectors: tools/ref/gen_golden.py.
- Parabola peak refinement has a phase-dependent bias up to ~0.017 px; Gaussian (log-parabola) refinement is the
  default and is < 0.004 px on noise-free blurred steps.
- Real part, 8 untouched bursts: edge sd 0.017 px (0.5 um), range 0.05 px. 301 px line sample + locate: ~150 us.
- CALIBRATION RISK: Gaussian refinement assumes a Gaussian-like blur across the edge. Diffraction/Fresnel fringes on a
  backlit silhouette can shift rising and falling edges in opposite directions, changing a diameter by twice the
  bias. Pin this with a certified gage pin before trusting +/-0.0005 in (Phase I-5).
- Phase II API wishes from review: contrast-relative edge threshold (lo/hi plateau levels), reusable scratch buffers,
  plateau (3-sample tie) refinement.

- Measured limit: two opposite edges closer than ~6 samples bias each other (sigma 1.0, blur 1.2): 0.0004 px at 8,
  0.018 px at 6, 0.25 px at 4. Phase II crest/root logic must not trust features narrower than ~6 px (~180 um).
- Phase II prerequisites carried forward: contrast-relative edge threshold, scratch-buffer API for ~2000 scan lines,
  3-sample plateau refinement.


## I-4 (reticle scale) - done
- Lattice fit (no corner detector): 14.5 ms, 30.1424-30.1425 um/px (19.9055 px per 0.6 mm square), repeatable ~5 ppm.
- Apparent tilt ~1.1 deg (cos = 0.99982): real tilt, or pixel/lens anisotropy. Disambiguate by refitting with the
  board rotated 90 deg (tilt turns with the board; anisotropy does not). Use the full matrix [a1 a2], not just sigma_max.
- Rising/falling edge bias on the real board: ~0.003 px (negligible).
- KEY FINDING: local scale varies ~0.1 % across the board (left/right 19.9156/19.8968 px/sq along x, top/bottom
  19.8913/19.9163 along y; whole-board residual 0.10 px rms vs 0.03-0.05 in halves/centre). That equals the +/-0.0005 in
  tolerance on a 0.5 in part, so a single global scale is not enough: I-4b distortion map is required.
- Limits: board rotation <= ~atan(1/min_edges) = 5.7 deg (scan rows must stay in one square row for min_edges squares);
  line_step < 1.5 squares; drift guard refuses aliasing. open() retries ~10 s while the camera is still held by the
  previous process.
- I-4b needs: distortion-aware clipping (the 5-MAD clip against the linear model removes corner edges where distortion is
  largest), a synthetic radial/quadratic distortion test, and indices anchored to a board feature if fits are to be merged.

## I-4b (distortion field map) - done
- FieldMap: polynomial i(x,y), j(x,y) (degree 3 default) fitted to every indexed reticle edge, robust-clipped against the
  polynomial itself; BoardPlane policy (mm = (i,j)*square_mm) is the only VALIDATED one. Calibrated region = 40 px tiles
  holding >= 3 kept edges of both families; outside it to_mm/distance_mm/local_um_per_px return None, to_mm_all flags each
  point. Spatial 2-fold CV (80 px blocks) and held-out-block chord tests on the real board:
    chord 6 mm: linear 564 ppm rms -> cubic 191;  9.6 mm: 480 -> 128;  18 mm: 299 -> 65.
- Real field: local scale 30.104 (lower-left) to 30.182 um/px (upper-right), ~0.26 % peak to peak; gradient 2441 ppm per
  1000 px toward the upper right. A single global scale would be wrong by up to +/-0.13 % depending on position
  (the whole +/-0.0005 in budget on a 0.5 in part).
- OPEN: the gradient points within 10 deg of the nominal board foreshortening axis (keystone signature) but the apparent
  tilt (~1.0 deg) is below the ~1.5 deg noise of the estimate. Settle with a second reticle capture rotated 90 deg
  (tilt/keystone turn with the board; lens distortion does not). Foreshortened policy stays UNVALIDATED until then.
- Not done (reviewer, lower priority): CV fold shortage should not abort a fit (cv_rms_px -> Option), jacobian() still
  allocates, keystone/centre-shifted synthetic test, example should use the map's Jacobian tilt, inverse map for overlays,
  serialisation of the calibration (needed before reuse across sessions).
- The reticle covers x 686..1683, y 851..1609 px; the GO gage (x ~1160..1720, y ~840..1280 in the earlier frame) sits at or
  past its top/right edge: place the gage inside the calibrated region or capture the reticle at another position.

## I-5 first gage run (vertical 1/2-13 GO gage, certificate PD 0.4500 in) - REPEATABLE, NOT YET ACCURATE
- Calibration: rebuilt from the saved reticle PNG (captures/calibration.txt, degree-3 BoardPlane map). 8-bit rounding of
  that PNG was verified NOT to bias the scale (dither / coarser quantisation leave 30.1346 um/px unchanged). It does differ
  from the live-capture scale (30.1425) by 260 ppm, entirely in the y pitch (19.9107 vs 19.9035 px): the reticle pose
  changed between captures. x pitch agrees to 35 ppm, and diameters use x.
- 8 untouched 8-frame bursts, ~3 ms of compute each: major 0.49661 in (sd 0.00001), minor 0.39246 (sd 0.00001),
  pitch 1.95728 mm = 12.977 TPI (nominal 13 = 1.9538 mm, +0.18 %), axis tilt 0.113 deg, silhouette-median PD 0.44071 in
  (sd 0.00006) = -0.0093 vs the certificate; textbook major - 0.6495*pitch = 0.4466 = -0.0034.
- Profile (60 deg flanks, crest flat ~0.12 P, root flat ~0.10 P, 43 px depth) rules out an edge-finder bug.
- Reviewer (method 5/10, speed 8/10): the PD is not defensible yet. Likely cause: uncorrected EDGE BIAS. The reticle pitch is
  a centre-to-centre distance so edge bias cancels out of the scale, but a diameter carries 2x the bias; a horizontal scan
  crosses a 30-degree flank so a normal bias b becomes 2b along the row, moving PD by ~4b and major by ~2b (reviewer estimate,
  not yet measured). Plus the helix envelope (~+0.002 in on PD, computed not measured).
- Roadmap to a defensible PD: (1) calibrate the edge offset normal to the edge on a plain certified master cylinder (gage
  pin), kept separate from scale; (2) fit straight lines to each flank (excluding ~15 % at each corner) and apply the bias
  along the normal; (3) solve for the radius where groove width = P/2; (4) subtract the analytic helix-envelope term;
  (5) validate on GO and NOGO gages, never fit a gage-specific bias; (6) quick check: tilt the part by the 3.1 deg lead angle
  and see if PD moves.
- Other reviewer findings still open: worst-tooth values (headline requirement, BG 5.4); axis fit robustness (chipped crest,
  Theil-Sen, check offset); crossing hysteresis / spacing check; row rejection (specks near the edge, thick particles);
  background level from calibration + bright-margin check; per-pitch flat fits instead of the 8 % extremes; blur/helix/
  edge-bias/chipped-tooth/random-phase tests and an image->edges->thread test; from_text duplicate/unknown keys and range
  checks, camera/exposure context; use every row instead of every 2nd.
- Naming: ThreadFit.pd_silhouette_mm is a silhouette statistic, not a gauge PD.

## Scale / diameter investigation (this session, uncommitted examples: pin_measure, focus_pin)
Setup: reticle calibration 30.1346 um/px (re-fit live later: 30.142). Reticle certificate PT016-024-C: squares 0.6001-0.6005 mm.
- Lengths between flat faces agree with the reticle scale: 0.751 in pin (calipers) reads 0.75251 in (+0.2 %).
- Diameters of polished Deltronic pins read small, vertical and horizontal alike (orientation ruled out):
  6.12 mm (0.24095 in) -0.0024 in; 0.5009 in -0.0058 (V) / -0.0055 (H); 0.985 in -0.0119. Linear fit measured = 0.9873*true + 0.0006 in
  (three points within 0.00002); leave-one-out on 0.5009 = 0.00006 in.
- But the fit does not transfer: 3/16 in pin (calipers 0.1875) reads 0.1869 (-0.0006); 1/2-13 GO gage crests (calipers 0.5000) read
  0.49661 (-0.0034); matte black-oxide SHCS head (calipers 0.3045-0.305) reads 0.3035 (-0.001..-0.0015).
- Not the cause: focus (edge never sharper than ~9 px 10-90 %), backlight distance (+45 mm made edges wider, width +0.0015 in),
  reticle drift, x/y anisotropy, reticle tilt (1.45 deg = 0.03 %).
- Working hypothesis (UNTESTED): specular reflection of the diffuse backlight from the polished limb (grey shoulder ~8 % over ~15 px
  inside the silhouette), growing with radius and surface finish. Fix is optical (collimating/light-control film), not a fitted correction.
- Next: blacken the 0.985 pin (washable marker) and re-measure: if it reads ~0.985 the hypothesis holds.
- Do NOT apply a pin-derived scale/offset to threads: it fails hold-outs on other finishes.

### Aperture backlight result (2026-11-10) - CAUSE CONFIRMED
- Collimator honeycomb (3 mm cells) is imaged sharply by the telecentric lens (grid pitch 3.8 mm, walls 0.8 mm); unusable. Do not use.
- Black matte mask with ONE round hole on the diffuse panel, panel ~3" (76 mm) behind the part, pin centred in the lit disc:
  - 6.12 mm pin, 12 mm hole: 0.24099 in vs 0.240945 certified = +0.00004 in (was -0.00242 in with the open panel).
  - 0.5009 in pin, 18 mm hole: 0.50097 in = +0.00007 in (was -0.0058 in). 6 bursts, row sd 0.00003 in.
- Cause: wide-angle diffuse light + specular polished limb makes diameters read small ~1.1% of the diameter. Narrow light cone (about +-4 deg or less) removes it. Reticle scale/focus/edge code were fine.
- Centre the part in the lit disc (align_pin example); use fresh frames (diagnose output must not be cut with Select-Object -First 1: it kills the process before writing diag_mean.png).
- Still to do: thread measurement needs a wide, narrow-angle source (Fresnel collimator, or aperture/slot at ~6"); retest 3/16 in and 0.985 in pins; then revisit thread PD code.

### Light tunnel hardware (2026-11-10)
- Design: `tools/hw/gen_tunnel.py` (CadQuery, scratch venv) -> `hardware/tunnel/`: tunnel_half_A/B, rear_cap, front_cap, slot_plate (.step assembly coords, .stl print orientation). 24 x 70 mm slot, 200 mm throw (+-3.4 deg across the part), no baffles, flock paper/ultra-flat black inside.
- Support-free: halves print floor-down, caps flat; overhang check 0 mm2. 1.75 mm filament dowels (2.0 mm holes) at 6 ears per half.
- Not yet printed or tested. Test plan: re-measure 6.12 mm, 0.5009 in, 3/16 in, 0.985 in pins (each within +-0.0002 in, unchanged when moved across the field), recalibrate reticle under this light, then 1/2-13 GO gage.

### Tunnel results, y scale and flank PD (2026-11-10)
- Tunnel (24 x 70 mm slot, 200 mm throw, no flock yet): 0.5009 in pin 0.50091 in (+0.00001), shifted 3 mm left 0.50087; 6.12 mm pin 0.24091 in (-0.00004). Lit slot is 24 mm wide: only vertical parts get the narrow cone (+-3.4 deg across); a horizontal pin sees +-9 deg along its diameter and reads ~0.0007 in small (narrow taped window fixes it). Pin 0.5009 has a stepped (smaller) end ~5 mm long: scan the full-diameter part.
- New reticle fit under the tunnel light (`captures/calibration_tunnel.txt`) differs from `captures/calibration.txt`: x chords +0.02 %, y chords +0.13 %. Horizontal-pin check (narrow cone): old map -0.00025 in, new +0.00040 in, so the old y scale is ~0.05 % low, the new ~0.08 % high. GO-gage pitch agrees with the old map being 0.048 % short (1.95292 vs 1.95385 mm). Old map kept; a +0.05 % y trim (pitch/length only, not diameters) is proposed, not applied. Re-seat the reticle 2-3 times to see the fit scatter.
- Flank-based PD (`thread_flank.rs`): flank lines on the middle half of the depth, pitch radius where tooth = P/2, helix-envelope raise removed per flank ((mP/2pi)^2/(2r), ~25 um/side on 1/2-13). Synthetic helix: within 0.3 um; asymmetric 19/44 deg form within 4 um; small threads at 30 um rows OK. Live 1/2-13 GO gage (cert PD 0.4500 in): PD 0.44963 in (-0.00037, sd 0.00001), major 0.50032, pitch 1.95292 mm, flank angles 29.5-30.4 deg. Silhouette median statistic read +0.0015 in (helix raise). One gage, one setup: validate on the NOGO gage and 10-24 screws, retest after flock.
- Reviews of the flank code: speed 9/10, quality 7 -> 7.5 -> 8/10 (three rounds); later fixes (chatter debounce, min slope, finite input) added after the 8/10.
- Open: PoorFit limit (min 30 um, 2 % of pitch) unchecked on real fine-pitch parts; `analyse_side` crossing level is one pass behind the final median (harmless); thread_measure.rs ~600 lines incl. tests.

### NOGO gage and 10-24 screw (2026-11-10, evening; tunnel, no flock)
- NOGO 1/2-13 plug (cert PD 0.4565 in, thread only ~3-4 pitches, rows ~1240-1495 after lowering it): flank fit refused (PoorFit, 48-59 um rms) in three setups, the bad flank staying on the image's right side after a rotation, so likely stray light/glints (edge profiles are non-monotonic: bright-dim-bright). One run with the old map gave PD 0.45722-0.45728 (+0.0007) but right falling flank 37 deg vs 30-31 and rms 28 um: not trusted. Retest after flock; add outlier-rejecting refit that reports the dropped count.
- #10-24 stainless flat head screw (old map, 5 bursts, two row windows): PD(flank) 0.15986 in both windows, major 0.18604-0.18605, minor 0.1342, pitch 1.05833 mm = 24.000 TPI (window 1045-1325; 23.991 on 1030-1340), flank angles 29.3-30.8 deg, fit rms 4-5 um, sd <= 0.00001 in. All inside 2A limits (PD 0.1586-0.1619, major 0.1818-0.1888). No certified PD for it: this checks repeatability and plausibility only.
- y scale: the new tunnel reticle map (calibration_tunnel.txt) reads 23.96 TPI on the screw (0.14 % long in y); with the horizontal pin (+0.08 %) and GO pitch it is ~0.1 % too large in y. The old map gives 24.000/23.991 TPI. DROP the earlier +0.05 % y-trim idea: old map is within about +-0.05 % in y (pin -0.05 %, screw 0 to +0.04 %, GO -0.05 %). Keep captures/calibration.txt; the new fit is not to be used.
- Blind pin check (tunnel, old map): a Deltronic pin measured 0.50032 in before its size was disclosed (the tool's placeholder was 0.5000); it is a 0.5003 in pin: +0.00002 in. Pins under the tunnel: 6.12 mm -0.00004, 0.5009 in +0.00001 (shifted 3 mm -0.00003), 0.5003 in +0.00002.
- Two more blind Deltronic pins (2026-11-11, tunnel, old map, 6 bursts x 520 rows, sd <= 0.00001 in between bursts): 0.49908 in vs marked 0.4991 = -0.00002 in; 0.49964 in vs marked 0.4997 = -0.00006 in. Five certified pins now, 0.24-0.50 in, all within 0.00006 in (target +-0.0005).

### Flock installed (2026-11-11)
- Pins with flock (old map): 0.49885 in vs marked 0.4988 = +0.00005; 0.49976 in vs marked 0.4997 = +0.00006. Seven certified pins, all within 0.00006 in.
- NOGO 1/2-13 plug with flock (rows 1240-1495, 5 bursts): the flank fit now passes (rms 22/14 um, was 48-59 and refused), but PD(flank) 0.45744 in (sd 0.00002) = +0.00094 vs the user-stated 0.4565, outside +-0.0005. Left falling flank 35.5 deg vs 30-31 elsewhere; major 0.4899, minor 0.4039, pitch 13.005 TPI; only 3 periods. Not yet explained: thread truncation/relief on the plug, bad left flank, or the stated PD. Next: outlier-rejecting refit, view the edge profiles, re-run the GO gage with the flock as a control.
- GO 1/2-13 gage with flock (rows 940-1500, 5 bursts, old map): PD(flank) 0.45017 in (sd <0.00001) = +0.00017 vs certified 0.4500 (was -0.00037 without flock: the flock moved it +0.00054). Major 0.50047 (was 0.50032), minor 0.39632, pitch 13.0017 TPI, flank angles 29.6-30.2 deg, rms 7/10 um. Still inside +-0.0005, but the shift is unexplained (flock, a different seating of the gage, or both). NOGO's +0.00094 is not explained by this.
- GO gage re-seat test (flock, old map, rows 940-1500, 5 bursts each; seatings B and C rotated/re-seated by hand): PD(flank) A 0.45017, B 0.45012, C 0.45001 in (errors +0.00017, +0.00012, +0.00001); spread 0.00016 in. Major 0.50047/0.50040/0.50057; minor 0.39632/0.39569/0.39593; pitch 13.0017/12.9982/12.9937 TPI (drifts 0.06 % over the three; axis tilt -0.23/-0.38/-0.46 deg moves with it; cause not established). Seating repeatability of PD is about 0.0002 in, a third of the +-0.0005 target.

### Per-tooth defect report (2026-11-11, evening)
- New `crates/metrology-core/src/thread_teeth.rs` (`inspect`, `ThreadSpec::unified(tpi)`): per side, tooth slots one pitch apart placed on the median phase of the tallest well-formed peaks; per tooth: crest/root radius, width at half depth, centre offset, rise/fall flank half angles (line fit over the middle half of the depth). Flags: Missing, LowCrest, ShallowRoot, Shifted, Narrow, Wide, BadFlank (> 5 deg from 30). Compares with the part's own good teeth (crest upper quartile, root median), not with the drawing. Axis refined from the good teeth's flat tops (a damaged tooth dragged the midpoint axis 0.04 mm off, which made healthy right-side crests look low). `thread_measure::axis_from_crests` extracted and shared.
- Live: `crates/camera/examples/tooth_report.rs <cal> <tpi> [y_lo y_hi x_lo x_hi]` (exit 1 on any defect). `edge_dump.rs` writes the calibrated silhouette to CSV.
- Chipped 1/2-13 part (rows 1060-1300, old map, flock): reference crest dia 0.4918 in, root dia 0.4208 in; every tooth's rising (upper) flank is a 52-60 deg ramp, falling flanks are 30.1-30.7 deg; the lowest left tooth is broken off (crest 5.48 mm vs 6.25, Missing; only judged when its slot falls inside the window, which depends on the grid phase). `measure_thread` correctly refuses the part (PitchMismatch / TooFewPeriods).
- Tests: 9 synthetic tooth tests (clean thread, chipped crest, missing, shifted, narrow, ramped flank, shallow root, bad input) + `tests/chipped_thread.rs` on the real silhouette (`tests/data/chipped_half_inch_13.csv`). No healthy-thread control run through `inspect` yet: needed to bound false positives (thread ends, run-out).
- Open: thread extent autodetect (caller currently supplies the rows); treatment of the first/last partial tooth; control run on the GO gage and a good turned thread; check the camera/tunnel did not move (the bright window shifted ~70 px right after the part was lifted); tolerances (5 deg, 8 %) are unvalidated on good turned threads.
- Review round 1 (speed 9, quality 7) fixed: root radius is now the higher of the lowest r within 0.75 pitch before/after the crest (a one-sided filled root shows; asymmetric chipped teeth still get the true root), flank ranges trimmed at those roots, new `NoFlank` defect (flank with < 8 samples in the middle-half band), reference crest from the upper quartile of peak heights (one burr cannot become the reference), width reach measured from the flat top, stricter input checks (finite spec, half angle in (0, 90), pitch >= 8 rows, ascending y), `NoTeeth` error, `axis_refined` flag. Tests now 14 in `thread_teeth` + the real-part regression; mutation check: the burr test fails with the old max-based reference.
- Review round 2: speed 9, quality 8. Fixed its top item: the flank sample minimum now scales with rows per pitch (a tenth, clamped 3..8; pitch under 20 rows is BadInput) so finer threads are not all flagged NoFlank (test: every 2nd row stays clean, every 4th row is BadInput). Known, documented: references are global, so taper/run-out/neck in the window and the first/last slot's truncated root search give suspect flags; nothing is tested with sensor noise yet; NoAxis / axis_refined=false / Wide{None} paths untested.

### Limits and GO-gage control (2026-11-11, late)
- `thread_standard.rs`: class 2A Unified limits (ASME B1.1 formulas as in Machinery's Handbook) for #10 to 1/2 in, UNC and UNF; all fields public/editable; `tooth_report` prints crest/root diameter against them (`major_max=0.4980` style overrides). Matches 1/2-13 and 1/4-20 table values within 0.0001 in (tests); values not yet checked against the user's Machinery's Handbook. Metric M5-M12 limits not done.
- Control run: certified GO gage (real silhouette in `tests/data/go_gage_half_inch_13.csv`) shows 0 of 15 teeth flagged once the flank tolerance was widened from 5 to 8 degrees (per-tooth flank angles on a healthy gage scatter 24.6 to 31.2; chipped part 52 to 60). The gage is a GO plug, not a 2A bolt, so its diameters read outside the 2A limits (major 0.5005, root 0.3960): expected.
- Chipped part vs 2A limits: crest dia 0.4918 in is inside 0.4877 to 0.4985; root dia 0.4208 in is 0.0167 in above the 0.4041 max minor, i.e. the thread was not cut to depth.

### Other-software review and edge-bias fix (2026-11-14)
- Reviewed Loupe (Apache-2.0) and Measurific (MIT) in `reference/software/`: digest in `docs/OTHER_SOFTWARE_REVIEW.md`. Neither has pitch diameter, helix handling, per-tooth defects or thread tables; both validate on synthetic data only.
- `tests/edge_bias.rs` (analytic pixel-integrated blurred bar, 160 phases x 4 widths): the Catmull-Rom crossing refinement in `profile.rs::refine_crossing` had a phase-dependent shift of up to 0.038 px per edge that does NOT cancel in a diameter when the two edges have different fractional phase (width error up to 0.031 px = 0.9 um at sigma 1.0, 0.075 px at 0.6). Replaced by a local natural cubic spline over +-8 samples (0.0066 px at sigma 1.0, 0.037 at 0.6). All workspace tests incl. scipy parity, clippy and fmt pass.
- `examples/camera_linearity.rs`: camera path is linear (gamma off, LUT off, black level 0, digital shift 0, auto exposure/gain off).
- Live A/B on the 0.5009 pin in the same session: old Catmull-Rom 0.50100 in, new spline 0.50100 in (a 0.1 um difference), so the spline change is validated on real data as harmless. The pin itself now reads +0.00010 in (it read +0.00001 earlier) because the SETUP changed: the pin sits off-centre in the tunnel slot (bright margin about 80 px on its left, 230 px on its right) and the left margin has a visible brightness ramp.
- Experiment, NOT adopted: per-edge local bright/dark levels instead of one global level per scan line (`docs/experiments/local_edge_levels.patch`). On the same uneven lighting it moved the pin from +0.00010 to -0.00015 in (6.4 um), so neither number is trustworthy until the illumination is flat; the theory favours local levels for a pure gradient but it is unvalidated. Decide after re-measuring pins with the pin centred and the field flat.
- Next physical step: centre the pin in the slot, check the bright field is flat to about 1 % beside both edges, re-measure the 0.5009 pin (expect about +0.00001 in).
- Pin centred in the slot (bright margins about equal, field flat beside the pin): 0.5009 in pin reads 0.50095 in = +0.00005 in. A/B on the same pin: local-level edge patch 0.50095, committed global-level code 0.50095 (0.01 um apart), so the -0.00015 under the earlier uneven light was the brightness ramp and the committed code is fine on a flat field. Local levels stay unadopted (unvalidated under a gradient). Lesson: centre the part in the slot and keep the field flat beside both edges; the app should warn when the bright margins differ by more than a few percent.

### Standalone app, first slice (2026-11-14)
- New `crates/app` (eframe/egui 0.37, approved by the user): `worker.rs` camera thread (connect, live ~8 fps, 8-frame measure) and `app.rs` window: zoom/pan image stage with scan-window and edge overlays, top bar (Connect, Live, Measure / M key), inspector with calibration path, scan window, and Diameter or Thread mode. Diameter: tolerance entry (nominal, +, -, guard band), big inch readout, PASS/WARN/FAIL chip, deviation in um, spread/taper/tilt, uneven-light warning (> 3 % background difference). Thread: class 2A limits (editable) from `thread_standard`, diameters against limits, per-tooth defect table, whole-thread fit (or the reason it was refused).
- New core modules: `diameter.rs` (`scan_edges`, `measure_diameter`, taper, tilt, illumination imbalance) and `tolerance.rs` (`Spec`, `Verdict`, guard band, 80 % warn band) with tests; core is now 144 tests. Start with `.\run_app.ps1`.
- Smoke test only: the release app launches, opens a window, stays up, no stderr. NOT yet tried interactively against a part (no way to click it from the session), start and measure times not yet timed, no UI screenshot review. Not committed.
- App review round 1 (UI 6, speed 6) fixed: `verdict.rs` (trust gate: uneven light > 3 %, > 20 % rows missing, row spread > 10 um; `thread_verdict` combining teeth, limits, pitch diameter, refused fit/teeth; NO VERDICT outranks FAIL outranks PASS), `Fit::Invalid` for NaN/inverted limits, non-finite guard band fails, results tied to their Job (hidden when mode/window/thread/calibration change), explicit MeasureFailed/LiveStopped events, latest-frame slot with half-size live frames, saved settings (`settings.rs`, tested), limits edited in inches, big full-width verdict chip, timing split in the status bar. Core 155 tests, app 5.
- Timing on the live camera (`examples/time_measure.rs`, release): open camera 0.45 s (budget 1 s), 8-frame grab 0.38 s, diameter compute 6 ms, thread scan+fit+teeth 6 ms: a measurement takes about 0.4 s (budget 1 s).
- Still missing: tooth markers on the image, ROI drag handles, report/CSV export, automatic thread extent and identification, M5-M12 limits, interactive test by the user (no way to click the window from the session), UI review of a screenshot.
- App review round 2 (UI 7, speed 8) fixed: unjudged light balance is NO VERDICT (`Untrusted::LightNotJudged`), thread numbers hidden under an untrusted scan (show-anyway box, marked NOT TRUSTED), measured pitch checked against the selected thread (2 %, NO VERDICT on a wrong size), dead worker thread handled, live frames no longer overwrite the measured image (`hold_image`), right-drag on the image draws the scan window (primary drag pans), every measurement appended to `measurements.csv` (`log.rs`, ignored by git). Core 157 tests, app 8.
- Open: master-pin check (prove the calibration is still good), tooth markers on the image, persisted thread limits, plain Rust-debug text in error messages, interactive test by the user.
- App review round 3: UI 8, speed 8 (gate met). Fixed its finding that Live stayed "on" while the image was held after a measurement (the stream now stops on Measured). Known, not fixed: the CSV verdict is recomputed from the current settings instead of stored with the measurement; `measurements.csv` is written to the working directory; a right-drag under 20 px gives no feedback.
