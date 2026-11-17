"""Light tunnel for the telecentric backlight (slot source at long throw), built to print without
supports: two tunnel half-troughs, a rear cap and a front cap, plus the slot plate.

Needs CadQuery (not a project dependency; used in a scratch venv):
    python tools/hw/gen_tunnel.py [outdir]        (default hardware/tunnel)

Output (in outdir):
  *.step   assembly coordinates (parts positioned as they go together)
  *.stl    print orientation (each part lying on its bed face at z=0, no rotation needed)
Parts: tunnel_half_A, tunnel_half_B, rear_cap, front_cap, slot_plate (+ tunnel_assembly.step)

Frame (assembly): Z is the optical axis, z=0 at the panel face (slot-plate face), z=LENGTH at the
open end facing the part. X across the part, Y along the part. All units mm.

Print orientation: each half lies with its outside (floor) face on the bed and its cavity facing
up; the lap lips, wall tops and dowel ears all grow upward from the walls, and the ears have 45
degree undersides, so no supports and no bridges. Caps print flat.

Locating features: ship-lap seam along both side walls (halves cannot slide sideways), six dowel
ears per half taking 1.75 mm filament pins, and caps whose tunnel socket grips both halves at the ends.
"""
import math
import sys
from pathlib import Path

import cadquery as cq

# --- design constants --------------------------------------------------------------------
LENGTH = 200.0                 # z=0 (panel face) to the open end
BORE = (30.0, 80.0)            # inside clear size, X x Y
WALL = 2.4                     # 6 perimeters at 0.4 mm
OUTER = (BORE[0] + 2 * WALL, BORE[1] + 2 * WALL)
CLR = 0.15                     # gap between mating faces so printed parts actually assemble
SLOT = (24.0, 70.0)
PLATE = (62.0, 102.0, 3.0)     # slot plate
PLATE_CLEAR = 1.6              # seat oversize (= +-0.8 mm of centring travel)
SOCKET = (OUTER[0] + 2 * CLR, OUTER[1] + 2 * CLR)   # cap socket that grips the tunnel
CAP = (90.0, 134.0)            # cap outline X x Y
HOLE_D = 3.4                   # M3 clearance
HOLE_PITCH = (80.0, 120.0)     # 4 holes, X x Y, same pattern on both caps
REAR_SOCKET_DEPTH = 5.0        # rear cap: seat z 0..3, tunnel socket z 3..8
REAR_T = PLATE[2] + REAR_SOCKET_DEPTH
FRONT_T = 6.0                  # front cap: socket all the way through
TUBE_Z0, TUBE_Z1 = REAR_T - REAR_SOCKET_DEPTH, LENGTH   # tunnel halves span z 3..200

# --- seam: ship-lap + dowel ears -----------------------------------------------------------
LAP = 3.0                      # ship-lap overlap each side of the seam
EAR_W = 12.0                   # ear reach beyond the wall; 45 degree underside (self-supporting)
EAR_LEN = 14.0                 # ear length along Z
EAR_Z = (45.0, 100.0, 155.0)
DOWEL_Y = 46.0                 # dowel axis, |y|
DOWEL_D, DOWEL_DEPTH = 2.0, 5.0   # for 1.75 mm filament pieces (cut ~9 mm long)


def rbox(x0, x1, y0, y1, z0, z1):
    return cq.Workplane("XY").box(x1 - x0, y1 - y0, z1 - z0, centered=False).translate((x0, y0, z0))


def _cyl_x(y, z, x0, length, dia):
    """Cylinder along X from x0 (length may be negative), centred at (y, z)."""
    return cq.Workplane("YZ").workplane(offset=x0).center(y, z).circle(dia / 2).extrude(length)


def _ear(sign_x: int, sign_y: int, z: float) -> cq.Workplane:
    """Triangular dowel ear on the outside of a side wall at the seam, 45 degree underside."""
    x0 = sign_x * CLR / 2
    yb = OUTER[1] / 2 - 0.2                      # base overlaps the wall so the ear fuses to it
    pts = [(x0, sign_y * yb), (x0, sign_y * (yb + EAR_W)), (x0 + sign_x * EAR_W, sign_y * yb)]
    ear = cq.Workplane("XY").workplane(offset=z - EAR_LEN / 2).polyline(pts).close().extrude(EAR_LEN)
    # keep clear of the other half's lap lip, which lies at |y| <= OUTER/2 for |x| < LAP
    keep = rbox(-LAP, LAP, 0, 0, 0, 0) if False else None
    lo, hi = sorted((sign_y * (OUTER[1] / 2 - 1.0), sign_y * (OUTER[1] / 2 + CLR)))
    return ear.cut(rbox(-LAP, LAP, lo, hi, z - EAR_LEN, z + EAR_LEN))


def tunnel_halves() -> tuple[cq.Workplane, cq.Workplane]:
    tube = rbox(-OUTER[0] / 2, OUTER[0] / 2, -OUTER[1] / 2, OUTER[1] / 2, TUBE_Z0, TUBE_Z1)
    tube = tube.cut(rbox(-BORE[0] / 2, BORE[0] / 2, -BORE[1] / 2, BORE[1] / 2, TUBE_Z0 - 1, TUBE_Z1 + 1))
    big = 500.0
    a = tube.intersect(rbox(0.0, big, -big, big, -big, big))
    b = tube.intersect(rbox(-big, 0.0, -big, big, -big, big))

    yi, yo = BORE[1] / 2, OUTER[1] / 2            # wall spans |y| in [yi, yo]
    ym = (yi + yo) / 2                            # lap interface
    for s in (1, -1):
        def yr(lo, hi):                           # |y| range -> signed y range
            return (lo, hi) if s > 0 else (-hi, -lo)
        # A: outer layer removed for x in [0, LAP]; inner layer extended to x = -LAP
        y0, y1 = yr(ym - CLR / 2, yo + 1)
        a = a.cut(rbox(0.0, LAP, y0, y1, TUBE_Z0 - 1, TUBE_Z1 + 1))
        y0, y1 = yr(yi, ym - CLR / 2)
        a = a.union(rbox(-LAP, 0.0, y0, y1, TUBE_Z0, TUBE_Z1))
        # B: inner layer removed for x in [-LAP-CLR, 0]; outer layer extended to x = LAP - CLR
        y0, y1 = yr(yi - 1, ym + CLR / 2)
        b = b.cut(rbox(-LAP - CLR, 0.0, y0, y1, TUBE_Z0 - 1, TUBE_Z1 + 1))
        y0, y1 = yr(ym + CLR / 2, yo)
        b = b.union(rbox(0.0, LAP - CLR, y0, y1, TUBE_Z0, TUBE_Z1))
        # dowel ears with 1.75 mm filament holes
        for z in EAR_Z:
            a = a.union(_ear(+1, s, z)).cut(_cyl_x(s * DOWEL_Y, z, CLR / 2, DOWEL_DEPTH, DOWEL_D))
            b = b.union(_ear(-1, s, z)).cut(_cyl_x(s * DOWEL_Y, z, -CLR / 2, -DOWEL_DEPTH, DOWEL_D))
    return a, b


def _cap_holes(w: cq.Workplane, z0: float, z1: float) -> cq.Workplane:
    pts = [(sx * HOLE_PITCH[0] / 2, sy * HOLE_PITCH[1] / 2) for sx in (-1, 1) for sy in (-1, 1)]
    return w.cut(cq.Workplane("XY").workplane(offset=z0 - 1).pushPoints(pts).circle(HOLE_D / 2).extrude(z1 - z0 + 2))


def rear_cap() -> cq.Workplane:
    cap = rbox(-CAP[0] / 2, CAP[0] / 2, -CAP[1] / 2, CAP[1] / 2, 0.0, REAR_T)
    pw, ph = PLATE[0] + PLATE_CLEAR, PLATE[1] + PLATE_CLEAR
    cap = cap.cut(rbox(-pw / 2, pw / 2, -ph / 2, ph / 2, -1.0, PLATE[2]))                       # slot-plate seat
    cap = cap.cut(rbox(-SOCKET[0] / 2, SOCKET[0] / 2, -SOCKET[1] / 2, SOCKET[1] / 2, PLATE[2] - 0.5, REAR_T + 1))
    return _cap_holes(cap, 0.0, REAR_T)


def front_cap() -> cq.Workplane:
    z0 = LENGTH - FRONT_T
    cap = rbox(-CAP[0] / 2, CAP[0] / 2, -CAP[1] / 2, CAP[1] / 2, z0, LENGTH)
    cap = cap.cut(rbox(-SOCKET[0] / 2, SOCKET[0] / 2, -SOCKET[1] / 2, SOCKET[1] / 2, z0 - 1, LENGTH + 1))
    return _cap_holes(cap, z0, LENGTH)


def slot_plate() -> cq.Workplane:
    return rbox(-PLATE[0] / 2, PLATE[0] / 2, -PLATE[1] / 2, PLATE[1] / 2, 0.0, PLATE[2]).cut(
        rbox(-SLOT[0] / 2, SLOT[0] / 2, -SLOT[1] / 2, SLOT[1] / 2, -1.0, PLATE[2] + 1.0))


def to_print(w: cq.Workplane, how: str) -> cq.Workplane:
    """Rotate an assembly-frame part so its bed face is z=0 (and nothing hangs below)."""
    if how == "A":                                # floor at x=+17.4 goes down
        w = w.rotate((0, 0, 0), (0, 1, 0), 90)    # (x,y,z) -> (z, y, -x)
    elif how == "B":                              # floor at x=-17.4 goes down
        w = w.rotate((0, 0, 0), (0, 1, 0), -90)   # (x,y,z) -> (-z, y, x)
    elif how == "flip":                           # tunnel-side face (z=top of cap) goes down
        w = w.rotate((0, 0, 0), (0, 1, 0), 180)
    bb = w.val().BoundingBox()
    return w.translate((0, 0, -bb.zmin))


def overhang_area(w: cq.Workplane, limit_deg: float = 45.0) -> float:
    """Area (mm2) of surfaces facing down steeper than limit_deg from vertical, excluding the bed."""
    verts, tris = w.val().tessellate(0.02, 0.1)
    area = 0.0
    lim = -math.cos(math.radians(limit_deg)) - 1e-3
    for i, j, k in tris:
        p, q, r = verts[i], verts[j], verts[k]
        u = (q.x - p.x, q.y - p.y, q.z - p.z)
        v = (r.x - p.x, r.y - p.y, r.z - p.z)
        n = (u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0])
        mag = math.sqrt(sum(c * c for c in n))
        if mag == 0:
            continue
        nz = n[2] / mag
        if nz < lim and min(p.z, q.z, r.z) > 0.05:
            area += mag / 2
    return area


def main(out: Path) -> None:
    out.mkdir(parents=True, exist_ok=True)
    a, b = tunnel_halves()
    parts = [("tunnel_half_A", a, "A"), ("tunnel_half_B", b, "B"), ("rear_cap", rear_cap(), "flip"),
             ("front_cap", front_cap(), None), ("slot_plate", slot_plate(), None)]
    for name, w, how in parts:
        s = w.val()
        assert s.isValid() and len(w.solids().vals()) == 1, f"{name} is not one valid solid"
        pw = to_print(w, how) if how else to_print(w, "")
        bb = pw.val().BoundingBox()
        print(f"{name:14s} volume {s.Volume() / 1000:6.1f} cm3   print footprint {bb.xlen:5.1f} x {bb.ylen:5.1f}, "
              f"height {bb.zlen:5.1f} mm   overhang >45deg off-bed: {overhang_area(pw):.1f} mm2")
        cq.exporters.export(w, str(out / f"{name}.step"))
        cq.exporters.export(pw, str(out / f"{name}.stl"), tolerance=0.01, angularTolerance=0.1)
    asm = a.union(b, clean=False).union(rear_cap(), clean=False).union(front_cap(), clean=False)
    cq.exporters.export(asm, str(out / "tunnel_assembly.step"))
    ov = a.intersect(b)
    print(f"A/B interference: {ov.val().Volume() if ov.solids().vals() else 0.0:.4f} mm3")
    print(f"acceptance across the part at the open end: +-{math.degrees(math.atan(SLOT[0] / 2 / LENGTH)):.2f} deg")


if __name__ == "__main__":
    main(Path(sys.argv[1]) if len(sys.argv) > 1 else Path("hardware/tunnel"))
