# Thread Vision

A shop-floor optical comparator for small turned parts. You put a pin or a threaded part between a
backlight and a telecentric camera, and the software measures its silhouette: diameter, and for
threads the major, minor and pitch diameter, pitch (TPI), flank angles and per-tooth defects. Each
result is checked against tolerance limits and given a PASS / WARN / FAIL verdict. If the image
can't be trusted, the verdict is NO VERDICT.

It was built at Saunders Machine Works as a replacement for calipers and micrometers in routine QC
of gage pins, thread gages and turned parts from about #10 to 1/2 in.

> **Status: working prototype.** It measures reliably on the rig described below, but it isn't a
> finished product. Windows only (the only platform built and tested). See
> [Known limits](#known-limits) before relying on a reading.

## Results so far

Measured on the rig below against certified references. Full log: [`docs/PROGRESS.md`](docs/PROGRESS.md).

| Check | Result |
|---|---|
| Seven certified Deltronic gage pins, 0.24–0.50 in (several measured blind) | All within **±0.00006 in** of their marked size |
| Repeatability, untouched (8-frame bursts) | Standard deviation ≤ 0.00001 in |
| 1/2-13 GO thread plug gage, pitch diameter (certificate 0.4500 in) | 0.45001–0.45017 in over three re-seatings |
| Re-seating the part by hand | Pitch diameter spread about 0.0002 in |
| Time per measurement | About 0.4 s (8-frame capture 0.38 s, computation about 6 ms) |

The design target is ±0.0005 in.

## How it works

1. **Silhouette imaging.** A blue backlight shines through a 200 mm light tunnel past the part into
   a bi-telecentric lens. The lens has fixed magnification and no perspective, so a part's
   distance from the lens doesn't change its measured size.
2. **Narrow light cone.** The tunnel limits the light to about ±3.4° across the part. With a wide,
   diffuse backlight, polished parts read about 1 % small, because light reflects off the curved
   edge into the lens. This was the hardest problem in the project; the full story is in
   `docs/PROGRESS.md`.
3. **Averaging.** Each measurement averages 8 frames from the camera, which is set to a linear
   response (no gamma, no auto exposure or gain).
4. **Sub-pixel edges.** Each scan line locates its edges to a few hundredths of a pixel (1 px is
   about 30 µm), using a local cubic spline. These routines are checked against SciPy reference
   values and analytic blurred-edge tests.
5. **Field-map calibration.** A chrome-on-glass chessboard reticle is fitted with a polynomial
   distortion map, because the scale varies by about 0.26 % across the field. Points outside the
   calibrated region are refused rather than guessed.
6. **Thread analysis.**
   - Pitch diameter comes from straight-line fits to each flank, with the helix effect removed.
   - Each tooth is checked for missing, low, shifted, narrow, wide or badly formed flanks.
   - Class 2A Unified limits (#10 to 1/2 in, UNC/UNF) are built in and can be edited.
7. **Trust gate.** The app gives NO VERDICT instead of a pass or fail when any of these happen:
   - uneven light beside the part (more than 3 % difference)
   - too many missing scan rows
   - a large spread between rows
   - a thread whose measured pitch doesn't match the size selected

The code is a Rust workspace:

| Crate | What it does |
|---|---|
| `crates/metrology-core` | All measurement maths. Pure Rust, no I/O, independent of the camera and UI. Most of the tests are here. |
| `crates/camera` | Basler pylon camera access (hand-written C bindings), frame checks, burst averaging, a mock camera. `examples/` holds the command-line tools. |
| `crates/app` | The desktop app (eframe/egui): live view, scan window, Diameter and Thread modes, verdicts, CSV log. |

## Bill of materials

### Optics and electronics

| Qty | Item | Notes |
|---|---|---|
| 1 | **Basler acA2440-20gm** camera | 5 MP (2448 × 2048) mono, Sony IMX264, global shutter, GigE Vision, C-mount |
| 1 | **Opto Engineering TC12056** bi-telecentric lens | 0.114× magnification, 157.8 mm working distance from the lens front. Gives a field of about 74 × 62 mm at about 30 µm/px. |
| 1 | **Advanced Illumination CX0404-455C1** backlight | 4 × 4 in, blue 455 nm |
| 1 | **SignaTech DCS-100E** LED controller, plus its power supply | Drives the backlight |
| 1 | Gigabit Ethernet port for the camera, ideally a dedicated one | Plus PoE (injector or PoE switch) or I/O-cable power for the camera |
| 1 | Windows 10/11 PC | Any modern machine; computing a measurement takes milliseconds |

### Calibration and reference standards

| Qty | Item | Notes |
|---|---|---|
| 1 | **Opto Engineering PT-series** chrome-on-glass chessboard reticle, 0.6 mm squares | Sets the pixel-to-mm scale and the distortion map. The one used here is PT016-024-C. |
| several | **Certified gage pins** (Deltronic used here), spanning the sizes you measure | Check the calibration and each new setup |
| 1+ | **Certified thread plug gages** (1/2-13 GO and NOGO used here) | Validate pitch diameter |

### Printed rig (rig v2)

One in-line printed body that holds the backlight, tunnel, part station, lens and camera. It is
712 × 199 × 186 mm and printed in black PETG in three sections that dowel and bolt together. Full
details are in [`hardware/rig_v2/BUILD_NOTE.md`](hardware/rig_v2/BUILD_NOTE.md): part list, print
settings, assembly and alignment.

| Qty | Item |
|---|---|
| 1 roll (1 kg) | Black PETG filament. The printer bed must take a 295 mm part (built on a Bambu H2D). |
| 9 / 16 / 2 | Heat-set inserts: M3 / M4 / 1/4-20 |
| about 25 | M3 and M4 socket-head screws, M4 set screws and an M3 thumbscrew (exact list in the build note) |
| 4 | Ø3 × 20 mm steel dowel pins |
| 12 | Ø16 × 3 mm self-adhesive rubber feet |
| 1 | 1 mm rubber pad for the camera cradle; 2 cable ties |
| as needed | Black flock paper to line the tunnel and the light-controlled space. It noticeably improved results. |
| as needed | 1.75 mm filament pieces, used as dowels in the tunnel halves |

The light tunnel on its own (`hardware/tunnel/`) is also available as separate STEP/STL parts,
generated by `tools/hw/gen_tunnel.py`. `hardware/collimator/` holds an earlier honeycomb
collimator. **Don't use it:** the telecentric lens images its cell walls sharply.

## Setup

### 1. Install the tools

1. **Rust** (stable, via [rustup](https://rustup.rs)) with the **MSVC Build Tools** (the
   "Desktop development with C++" workload). The project was built with Rust 1.99.
2. **Basler pylon 7.x** from Basler's website. Install the Developer and Runtime components;
   7.5 was used here. The build links `PylonC_v8_0.lib` from
   `C:\Program Files\Basler\pylon 7\Development`. If pylon is installed somewhere else, set
   `PYLON_DEV_DIR`.
3. **Git**.

Optional, only to regenerate generated files:
- Python with numpy + scipy for `tools/ref/gen_golden.py` (test reference values)
- CadQuery for `tools/hw/gen_tunnel.py` (tunnel CAD)

### 2. Connect the camera

Connect the camera to its Ethernet port and power it. Use pylon Viewer (installed with pylon) to
confirm it shows up and streams. Close pylon Viewer before starting this app: only one program can
hold the camera. Jumbo frames (MTU 9000) on that network adapter are optional; they would make the
8-frame capture a little faster.

### 3. Build and test

```powershell
git clone <this repo>
cd ClaudeThreadVision
cargo test --workspace        # runs without a camera
cargo build --release
```

### 4. Calibrate

Do this once per rig, and again whenever the lens, camera or tunnel is moved.

1. Put the chessboard reticle at the part position, in focus and lit by the tunnel.
2. Capture an 8-frame mean and save it:
   ```powershell
   cargo run --release -p camera --example diagnose -- captures/reticle.png
   ```
3. Build the calibration from it (arguments: square size in mm, nominal pixels per square, map
   degree):
   ```powershell
   cargo run --release -p camera --example calibrate_from_png -- captures/reticle.png captures/calibration.txt 0.6 20 3
   ```
4. Check the calibration with a certified pin: put the pin in the V seat, centred in the slot, and
   run `pin_measure` (see below). It should read within a few hundred-thousandths of an inch.

`captures/` is gitignored. The app looks for `captures/calibration.txt` by default, and you can
point it at another file in the inspector.

### 5. Run the app

```powershell
.\run_app.ps1
```

The script adds the pylon runtime to `PATH`, builds the release app and starts it.

- **Connect**, then **Live** to see the camera. Drag to pan and scroll to zoom. Right-drag draws
  the scan window around the part.
- Choose **Diameter** or **Thread** mode. Enter the nominal size and tolerance, or pick a thread
  size; its 2A limits fill in and can be edited.
- **Measure** (or press **M**) captures 8 frames and shows the result and verdict.
- Each measurement is added to `measurements.csv` in the working folder.

**Keep the part centred in the slot**, so the bright margins on both sides match. The app warns
when they differ by more than 3 %, because uneven light shifts the edges.

## Command-line tools

These are in `crates/camera/examples/` and run with
`cargo run --release -p camera --example <name> -- <args>`. Most need the camera; each file's
header comment documents its arguments.

| Tool | Use |
|---|---|
| `probe` | List cameras, open the first, print its settings and grab timings |
| `diagnose` | Save an 8-frame mean PNG and report backlight uniformity |
| `camera_linearity` | Confirm the camera is set to a linear response |
| `calibrate_from_png` | Build a calibration file from a reticle image (no camera needed) |
| `field_map` | Validate the distortion map on reticle edges it wasn't fitted to |
| `align_pin` | Live meter for centring a pin in the lit area |
| `pin_measure` | Measure a certified plain pin and report the per-side edge offset |
| `gage_measure` | Measure a vertical thread gage: major, minor, pitch diameter, pitch |
| `tooth_report` | Per-tooth defect report against class 2A limits; exit code 1 on a defect |
| `time_measure` | Time the open, capture and compute steps |
| `edge_dump`, `edge_probe`, `row_debug`, `quantization_probe`, `focus_pin`, `map_compare`, `reticle_scale`, `scale_field` | Investigation tools used during development |

## Known limits

- One camera, one rig, Windows only.
- **Vertical parts are the main mode.** A horizontal part sees a wider light cone along its
  diameter and reads about 0.0007 in small unless the window is narrowed. The rig v2
  horizontal V-block carrier isn't designed yet.
- **NOGO thread gage:** it read 0.0009 in over its stated pitch diameter, and the cause is still
  unexplained.
- **Thread extent is not detected automatically.** You draw the scan window over the threaded
  section.
- **Limits:** only Unified class 2A, #10 to 1/2 in; metric M5–M12 is not done. The defect
  thresholds (8° flank, 8 % width) haven't been checked on a range of good turned threads yet.
- The y-axis scale is good to about ±0.05 %. Diameters use the x axis.
- Open items, with the evidence behind each, are listed in [`docs/PROGRESS.md`](docs/PROGRESS.md).

## Repository layout

```
crates/            Rust workspace: metrology-core, camera (+ examples/), app
hardware/          CAD: rig_v2 (printed body), tunnel, collimator (abandoned)
tools/             Python generators: test reference values, tunnel and collimator CAD
docs/              Progress log, design briefs, third-party software review, experiments
reference/         Local-only source material (only its README is in git)
run_app.ps1        Build and start the app with pylon on PATH
```

## Documentation

| File | What's in it |
|---|---|
| [`docs/PROGRESS.md`](docs/PROGRESS.md) | Lab notebook: what was measured, what was found, what is still open |
| [`PROJECT_BACKGROUND.md`](PROJECT_BACKGROUND.md) | Lessons from the first (Python) build on the same hardware: camera quirks, algorithms, calibration pitfalls |
| [`MILESTONES.md`](MILESTONES.md) | The original phased plan (Phase I accuracy, Phase II threads, Phase III auto-detect and defects) |
| [`DESIGN_GUIDELINES.md`](DESIGN_GUIDELINES.md) | UI and measurement-workflow design rules |
| [`DESIGN_REFERENCE.md`](DESIGN_REFERENCE.md) | Notes on how commercial systems present thread measurement |
| [`docs/OTHER_SOFTWARE_REVIEW.md`](docs/OTHER_SOFTWARE_REVIEW.md) | What was learned from two open-source inspection projects |
| [`hardware/rig_v2/BUILD_NOTE.md`](hardware/rig_v2/BUILD_NOTE.md) | Printing, assembling and aligning the rig |
| [`docs/design/FUSION_BRIEF.md`](docs/design/FUSION_BRIEF.md) | Design brief for the printed rig |
| [`reference/README.md`](reference/README.md) | The local reference folder and its rules |

### Working on it with AI coding agents

This project was developed with Claude Code. [`AGENTS.md`](AGENTS.md) (read by Codex and others)
and [`CLAUDE.md`](CLAUDE.md) hold the rules agents follow here: commands, testing, secrets and git.
The skills in `.claude/skills/` and `.agents/skills/` and the files in `optional/` come from a
generic project kit. `ARCHITECTURE.md`, `PRACTICES.md`, `TESTING-PIPELINE.md` and
`SECRETS-PIPELINE.md` belong to that kit and describe its process, not this application.

## References and acknowledgements

The material below informed the design. None of it is redistributed here. Copies kept by the
project owner live in the local `reference/` folder (see [`reference/README.md`](reference/README.md)).

**Open-source software studied**
- **Measurific** by Kyle Bartholomew (MIT), <https://github.com/kylebme/measurific>. Sub-pixel
  inspection of backlit telecentric images. Its synthetic edge-threshold study prompted this
  project's analytic edge-bias test, which led to replacing the Catmull-Rom edge refinement with a
  cubic spline. Its camera-linearity notes became `examples/camera_linearity.rs`.
- **Loupe** by Quintin Smits (Apache-2.0). Browser-based optical inspection for small shops. Its
  metrology accuracy tests also informed the edge-bias test. Its robust line-fit quality gates and
  its rule for flagging ambiguous edges are on the to-do list.
- Ideas and test methods from both were re-implemented in this codebase. Details and the remaining
  to-do list are in [`docs/OTHER_SOFTWARE_REVIEW.md`](docs/OTHER_SOFTWARE_REVIEW.md).

**Commercial systems studied** (for the UI and workflow; details in `DESIGN_REFERENCE.md`)
- Keyence IM-7000 / IM-X series image dimension measurement systems: the IM-7xxx spec sheet and
  these videos:
  - "Verifying Gauge Accuracy with the Keyence" and "Keyence IM Series Vision System", by
    Kremin Inc.
  - "Our New Keyence Thread Inspection Machine", by Rolled Threads Unlimited
- Mitutoyo Quick Vision (ACTIVE, Pro) and M3 software: product brochures and Mitutoyo's video
  "Thread Measuring Made Easy with M3".
- *Industrial Vision Metrology Systems Reference Guide*: a general survey document.

**Standards and data**
- Class 2A Unified thread limits use the ASME B1.1 formulas as given in *Machinery's Handbook*.

**Software this project uses**
- Basler pylon SDK (camera driver, installed separately; not redistributed)
- [egui / eframe](https://github.com/emilk/egui) for the desktop app
- NumPy and SciPy to generate the reference values for the edge and spline tests
- CadQuery for the tunnel CAD

**Earlier work.** An earlier Python implementation (pypylon) on identical hardware came first.
Its lessons are written up in `PROJECT_BACKGROUND.md`, and this Rust version was built from that
write-up.

## License

MIT. See [`LICENSE`](LICENSE). The CAD files in `hardware/` are covered by the same license.

The projects and documents listed under References are their owners' work under their own terms;
none of their code or content is included here.
