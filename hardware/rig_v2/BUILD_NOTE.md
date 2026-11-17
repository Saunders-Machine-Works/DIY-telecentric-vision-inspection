# Rig v2 build note

One printed body for the in-line telecentric rig:
backlight → 200 mm tunnel → part station → 157.8 mm air gap → TC12056 lens → acA2440 camera.

Fusion document: *Telecentric Vision Camera System + Mount*, component `Rig_v2` (version 31).
Files in this folder:

- `rig_v2_assembly.step`: the whole `Rig_v2` component, including the reference parts.
- One STL per printed part, in mm.
- `rig_v2_iso.png`: an iso view of the assembly.

Overall size: **712 × 199 × 186 mm** (L × W × H), plus 1 mm for the feet.
The optical axis sits **100 mm** above the bench (`AxisHeight`).

## Coordinate frame (Rig_v2)

| Axis | Direction | Zero point |
|---|---|---|
| X | Along the optical axis, toward the lens | Focal plane (`FocalDist` from the lens front) |
| Y | Across the part | Optical axis |
| Z | Up | Bench (underside of the print) |

Key stations along X (mm):

| Item | X (mm) |
|---|---|
| Light-head back wall | −240.6 |
| Backlight face | −218.6 |
| Slot plate | −212.6 |
| Tunnel exit | −12.6 |
| Focal plane | 0 |
| Lens front | 157.8 |
| A/B joint | 35 |
| B/C joint | 270 |
| Camera rear | 416.3 |
| End of rig | 471.3 |

## Printed parts

Bounding boxes are given as X × Y × Z in the model frame. Mass is for solid PETG at 1.27 g/cm³. With 6 walls and 20–25 % infill, expect roughly 45–60 % of that figure.

| Part | Size (mm) | Solid mass | Print orientation |
|---|---|---|---|
| A_Base (light-head cradle, part station, A/B tongue) | 295 × 199 × 55 | 1.52 kg | As modelled, bottom down |
| A_LightHead (cartridge: backlight bay + tunnel bulkheads) | 222 × 172.5 × 172.5 | 0.91 kg | As modelled: open top up, lying on its bottom face |
| A_Lid (light hood, part station to lens front) | 178.6 × 150 × 146 | 0.38 kg | **Flip 180°**: top face on the bed |
| A_PartStage (Y-centring plate, dovetail seat) | 36 × 54 × 10 | 19 g | As modelled |
| A_HolderPlug (vertical holder, 90° V seat) | 29.8 × 24 × 17.8 | 13 g | As modelled, tenon down |
| B_Bridge (shroud floor, front lens saddle, controller bay, B/C tongue) | 255 × 160 × 100 | 1.03 kg | As modelled; **supports inside the controller bay only** |
| B_SaddleCap (front, Ø80 barrel) | 20 × 112 × 46 | 64 g | **Flip 180°**: flat top on the bed |
| B_ControllerDrawer (DCS-100E tray) | 118 × 160 × 29 | 71 g | As modelled, floor down |
| C_CameraBase (rear lens saddle, camera cradle, strain relief) | 201.3 × 150 × 118 | 1.40 kg | As modelled, bottom down |
| C_SaddleCap (rear, Ø61 barrel) | 20 × 96 × 37 | 51 g | **Flip 180°**: flat top on the bed |

Every part fits the H2D bed. The largest is A_Base at 295 mm.

The STLs are in assembly orientation. In Bambu Studio, use *Lay on face* to flip the lid and the two saddle caps.

### Print settings (PETG, 0.4 mm nozzle)

- **Layers and walls:** 0.2 mm layers, 6 walls (2.4 mm), 5 top / 5 bottom layers, 20–25 % gyroid infill.
- **Black PETG:** use it for A_LightHead, A_Lid, B_Bridge and A_Base. Everything that faces the light path must be black.
- **Brim:** use a 5 mm brim on A_Base, B_Bridge and C_CameraBase. Long flat PETG parts lift at the corners.
- **B_Bridge controller bay:** the bay roof is a 116 × 157 mm bridge and needs supports. Paint supports in the bay only. The bay is open on +Y, so they pull out easily.
- **Ironing:** optional, on the cradle floor (A_Base, z = 13.75) and the saddle seat tops.

### Print problems already designed around

- **A_LightHead open top face.** It has a 6 mm rail instead of a full top wall, so the 166 mm span doesn't have to bridge. Both bulkhead windows are 40 mm wide or less, so they bridge cleanly.
- **Backlight bay open on top.** The 166 mm backlight drops in from above.
- **Hood keyhole.** The hood's front wall has a keyhole that is open downward, so the hood lifts straight off over the lens.

## Fasteners and hardware

| Item | Qty | Where |
|---|---|---|
| M3 heat-set insert (hole Ø4.0) | 9 | Light-head bulkheads ×8 (tunnel caps, 80 × 120 pitch); part-stage plug lock ×1 |
| M3 × 12 SHCS | 4 | Rear tunnel cap → rear bulkhead (fit from inside the cartridge) |
| M3 × 10 SHCS | 4 | Front tunnel cap → front bulkhead |
| M3 × 16 thumbscrew | 1 | Holder plug lock, stage +Y side |
| M4 heat-set insert (hole Ø5.6) | 16 | See breakdown below |
| M4 × 16 cup-point set screw, or thumbscrew | 6 | Cartridge centring ×4; stage push ×2 |
| M4 × 16 SHCS + washer | 2 | Stage clamp, through the Y slots |
| M4 × 20 SHCS | 4 | A/B and B/C joints, counterbored from the top |
| M4 × 55 SHCS | 2 | Front saddle cap |
| M4 × 45 SHCS | 2 | Rear saddle cap |
| Ø3 × 20 steel dowel pin | 4 | Two per section joint |
| 1/4-20 heat-set insert (hole Ø8.0 × 13 deep) | 2 | B_Bridge underside, centreline, X = 90 and 140 (50 mm apart). Check the hole size against your insert's spec. |
| Rubber foot, Ø16 × 3 mm, self-adhesive | 12 | Four per base section, in Ø16.5 × 2 mm pockets |
| 1 mm rubber or silicone pad, about 30 × 30 mm | 1 | Camera cradle floor; it carries the camera's weight |
| Cable ties, 3.6 mm wide | 2 | Strain-relief post, C_CameraBase |
| 1.75 mm filament pins | as in tunnel note | Tunnel halves (unchanged) |

M4 insert breakdown:

- A_Base: 10 (cradle ×4, stage clamp ×2, stage push ×2, A/B tongue ×2)
- B_Bridge: 4 (front saddle ×2, B/C tongue ×2)
- C_CameraBase: 2 (rear saddle)

## Joints between sections

Both joints use the same scheme:

- The rear section has a **lap tongue** 20 mm long, 140 mm wide and 15 mm tall.
- The forward section sits over it in a matching 0.15 mm-clearance recess.
- **2 × Ø3 dowels** locate the joint, and **2 × M4** screws go down through counterbores into heat-set inserts in the tongue.

The two joints are:

- **A/B joint at X = 35.** Its screws are under the hood: lift the hood to reach them.
- **B/C joint at X = 270.** It sits under the lens cone; the screws are reached from above, beside the lens.

No joint crosses the optical path.

In Fusion, the sections are modelled in place in one frame and **grounded to `Rig_v2`**; there are no Fusion joints. The physical location comes from the dowels.

## Assembly order

1. Press the heat-set inserts: M3, M4 and 1/4-20.
2. Stick the rubber feet into the pockets.
3. Join A_Base → B_Bridge → C_CameraBase on a flat reference surface (a granite plate if you have one). Use dowels first, then the M4 screws. Tighten while the bases sit flat.
4. Build the tunnel as before. Bolt the rear cap to the rear bulkhead and the front cap to the front bulkhead, M3 from inside.
5. Drop the backlight into its bay with its face against the rear bulkhead. Route the cable out of the slot in the back wall.
6. Set the cartridge in the cradle and slide it forward until it stops against the station block. Run in the four side screws until they just touch.
7. Fit the part stage: two clamp screws loose, two push screws touching. Slide in the holder plug and tighten the M3 lock.
8. Lay the lens in both saddles and fit the caps. Put the 1 mm pad on the camera cradle floor. The camera rests on it with 1 mm clearance on the printed walls.
9. Zip-tie the GigE and I/O cables to the strain-relief post, leaving slack between the camera and the post.
10. Put the DCS-100E in the drawer, route its cables through the front-plate notch, and slide the drawer in from +Y.
11. Drop the hood into its grooves.

## Alignment

1. **Slot onto the lens axis.** With no part in place, take a live image. Move the cartridge with the two pairs of side screws, one opposing pair at each end, until the bright slot image is centred left-to-right. Snug the screws evenly.
2. **Part centred in the slot.** Put a certified pin in the V seat. Use the stage push screws to move the part in Y until the two bright margins match. The software's 3 % background-asymmetry warning is the test.
3. Tighten the two stage clamp screws, then recheck. Back the push screws off a quarter turn, or leave them snug as a backup lock.
4. Check the reading against the certified pins.
5. **Re-check after about a week.** PETG creeps under screw load at inserts and clamps, so expect a few micrometres of drift in the first days.

## Design decisions made during the build

- **Fixed focus (your choice).** There is no focus slide. The V seat apex is 6 mm behind the focal plane. A 15.24 mm part's axis lands 4.8 mm in front of focus; a 3 mm part's lands about 4 mm behind. Both are well inside the ±16 mm depth of field.
- **Horizontal swap: reseat the square cartridge.**
  1. Lift the cartridge out.
  2. Turn it 90° about the axis.
  3. Set it back in the cradle and re-centre.

  The slot turns 90° with it, and the backlight stays square. The horizontal-mode V-block carrier isn't modelled yet. It plugs into the same dovetail seat and will need its own height-centring screw.
- **Tunnel caps bolted, not pressed in.** They use the caps' existing 3.4 mm holes. This moved the backlight 6 mm back to make room for the rear bulkhead (6 mm). The tunnel exit stays 12.62 mm from the part axis.
- **`TunnelGap` measured to the part surface.** The brief measured it to the part axis, which would let a 15.24 mm part hit the tunnel. With fixed focus the actual exit-to-surface gap is 9.8 mm for the largest part, inside the brief's 5–10 mm.
- **Reticle.** It uses a carrier on the same dovetail seat, so no reticle gate is in the body. The carrier isn't modelled yet.
- **Camera pocket.** It clears the camera by 1.0 mm on all sides, as the brief requires; the 1 mm pad carries the weight. The lens saddle bores are 0.3 mm over the barrels, also per the brief.

## Open items

1. **Backlight retention.** The backlight bay is open on top. In horizontal mode it needs a strap or bar across the opening. I didn't have the CX0404's rear mounting-hole positions.
2. **Short parts sit low in the field.** The seat is at Z = 65, the bottom of the 70 mm lit field. A 38.1 mm part reaches 3 mm above the axis; parts shorter than about 35 mm don't reach the field centre. If the scan window needs the centre, print a riser plug. It's the same dovetail, only taller.
3. **Not modelled yet:**
   - the V-block carrier for horizontal parts
   - the reticle carrier
   - the part spring clip
4. **Camera connectors.** Their positions aren't in the camera model, so the strain-relief notch is generic: centred, Ø8 mm cable.
5. **Parametric coverage is partial.** These are driven by named parameters:
   - main section heights (`BaseTop`, `AxisHeight`, `CradleFloor`)
   - light-head size and walls (`HeadBox`, `HeadWall`, `BulkThk`, `TrayLen`)
   - tunnel stations (`CartFrontD`, `CartRearD`)
   - saddle bores and widths (`LensFrontR`, `LensRearR`, `SaddleW`)
   - foot pockets and dowel diameter

   Many hole and feature positions are fixed numbers in their sketches. Changing `AxisHeight` or `FocalDist` will **not** move everything consistently; re-check before relying on a parameter change.
6. **Fusion has no PETG material.** The masses above are computed from volume. The parts still carry Fusion's default steel material, so ignore any mass Fusion shows.
