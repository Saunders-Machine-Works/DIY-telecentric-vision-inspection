"""Generate the backlight collimator (square-cell louvre): STL + dimensioned SVG + concept SVG.

Standard library only. Run:  python tools/hw/gen_collimator.py [outdir]
Change the constants below and re-run; every output follows them.

Geometry: an N x N grid of square through-holes (CELL wide) separated by WALL-thick walls and
surrounded by a FRAME, extruded DEPTH along Z (= the optical axis). Light can only leave within
about atan(CELL/DEPTH) of the axis. Printed with the cell axes vertical.
"""
import math
import struct
import sys
from pathlib import Path

CELL = 3.0     # mm, hole width
WALL = 0.8     # mm, wall between holes (two 0.4 mm perimeters)
FRAME = 4.0    # mm, solid border all round
N = 22         # holes per side
DEPTH = 50.0   # mm, along the optical axis

SIZE = 2 * FRAME + N * CELL + (N - 1) * WALL


def lattice():
    """Coordinates of the tile boundaries along one axis and which tiles are solid (index even)."""
    xs = [0.0]
    widths = [FRAME]
    for i in range(N):
        widths += [CELL, WALL if i < N - 1 else FRAME]
    for w in widths:
        xs.append(xs[-1] + w)
    return xs  # len = 2N + 2, tiles 0..2N (even = wall/frame, odd = hole)


def solid(i, j):
    n = 2 * N + 1
    if i < 0 or j < 0 or i >= n or j >= n:
        return False
    return i % 2 == 0 or j % 2 == 0


def build_triangles():
    xs = lattice()
    n = 2 * N + 1
    tris = []

    def quad(a, b, c, d):  # a,b,c,d counter-clockwise seen from outside
        tris.append((a, b, c))
        tris.append((a, c, d))

    z0, z1 = 0.0, DEPTH
    for i in range(n):
        for j in range(n):
            if not solid(i, j):
                continue
            xa, xb, ya, yb = xs[i], xs[i + 1], xs[j], xs[j + 1]
            quad((xa, ya, z1), (xb, ya, z1), (xb, yb, z1), (xa, yb, z1))  # top, normal +z
            quad((xa, ya, z0), (xa, yb, z0), (xb, yb, z0), (xb, ya, z0))  # bottom, normal -z
            if not solid(i - 1, j):  # -x side
                quad((xa, ya, z0), (xa, ya, z1), (xa, yb, z1), (xa, yb, z0))
            if not solid(i + 1, j):  # +x side
                quad((xb, ya, z0), (xb, yb, z0), (xb, yb, z1), (xb, ya, z1))
            if not solid(i, j - 1):  # -y side
                quad((xa, ya, z0), (xb, ya, z0), (xb, ya, z1), (xa, ya, z1))
            if not solid(i, j + 1):  # +y side
                quad((xa, yb, z0), (xa, yb, z1), (xb, yb, z1), (xb, yb, z0))
    return tris


def normal(t):
    (ax, ay, az), (bx, by, bz), (cx, cy, cz) = t
    ux, uy, uz = bx - ax, by - ay, bz - az
    vx, vy, vz = cx - ax, cy - ay, cz - az
    nx, ny, nz = uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx
    ln = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
    return nx / ln, ny / ln, nz / ln


def write_stl(path, tris):
    with open(path, "wb") as f:
        f.write(b"Backlight collimator (square-cell louvre)".ljust(80, b" "))
        f.write(struct.pack("<I", len(tris)))
        for t in tris:
            f.write(struct.pack("<3f", *normal(t)))
            for v in t:
                f.write(struct.pack("<3f", *v))
            f.write(b"\x00\x00")


def check_mesh(tris):
    """Watertight (every edge shared by exactly two triangles, once each way) + volume."""
    edges = {}
    for t in tris:
        for k in range(3):
            a, b = t[k], t[(k + 1) % 3]
            edges[(a, b)] = edges.get((a, b), 0) + 1
    bad = [e for e, c in edges.items() if c != 1 or edges.get((e[1], e[0]), 0) != 1]
    vol = 0.0
    for (ax, ay, az), (bx, by, bz), (cx, cy, cz) in tris:
        vol += (ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx)) / 6.0
    return len(bad), vol


def svg_drawing(path):
    s = 5.0  # px per mm
    xs = lattice()
    m = 70
    w = SIZE * s + m + 130
    h = SIZE * s + DEPTH * s + 3 * m + 70
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:.0f} {h:.0f}" width="{w:.0f}" height="{h:.0f}" font-family="Arial" font-size="12">',
           f'<rect width="100%" height="100%" fill="white"/>',
           f'<text x="{m}" y="28" font-size="16" font-weight="bold">Backlight collimator - square-cell louvre (all mm)</text>',
           f'<text x="{m}" y="46" fill="#444">{N} x {N} cells, cell {CELL}, wall {WALL}, frame {FRAME}, depth {DEPTH}. Print matte black, cell axes vertical.</text>']
    ox, oy = m, 96
    out.append(f'<rect x="{ox}" y="{oy}" width="{SIZE*s:.1f}" height="{SIZE*s:.1f}" fill="#222"/>')
    for i in range(N):
        for j in range(N):
            x = ox + xs[2 * i + 1] * s
            y = oy + xs[2 * j + 1] * s
            out.append(f'<rect x="{x:.1f}" y="{y:.1f}" width="{CELL*s:.1f}" height="{CELL*s:.1f}" fill="white"/>')

    def hdim(x0, x1, y, label):
        out.append(f'<line x1="{x0}" y1="{y}" x2="{x1}" y2="{y}" stroke="#c00" stroke-width="1"/>')
        for x in (x0, x1):
            out.append(f'<line x1="{x}" y1="{y-5}" x2="{x}" y2="{y+5}" stroke="#c00"/>')
        out.append(f'<text x="{(x0+x1)/2}" y="{y-6}" text-anchor="middle" fill="#c00">{label}</text>')

    def vdim(x, y0, y1, label):
        out.append(f'<line x1="{x}" y1="{y0}" x2="{x}" y2="{y1}" stroke="#c00"/>')
        for y in (y0, y1):
            out.append(f'<line x1="{x-5}" y1="{y}" x2="{x+5}" y2="{y}" stroke="#c00"/>')
        out.append(f'<text x="{x+8}" y="{(y0+y1)/2}" fill="#c00">{label}</text>')

    hdim(ox, ox + SIZE * s, oy - 10, f"{SIZE:.1f}")
    vdim(ox + SIZE * s + 14, oy, oy + SIZE * s, f"{SIZE:.1f}")
    hdim(ox + (FRAME) * s, ox + (FRAME + CELL) * s, oy + SIZE * s + 16, f"cell {CELL}")
    hdim(ox + (FRAME + CELL) * s, ox + (FRAME + CELL + WALL) * s, oy + SIZE * s + 34, f"wall {WALL}")
    out.append(f'<text x="{ox}" y="{oy + SIZE*s + 62}" font-weight="bold">Top view (looking along the optical axis)</text>')

    # section through the middle row of holes (the central hole row index N//2)
    sy = oy + SIZE * s + 96
    out.append(f'<rect x="{ox}" y="{sy}" width="{FRAME*s}" height="{DEPTH*s}" fill="#222"/>')
    out.append(f'<rect x="{ox+(SIZE-FRAME)*s:.1f}" y="{sy}" width="{FRAME*s}" height="{DEPTH*s}" fill="#222"/>')
    for i in range(N - 1):
        x = ox + xs[2 * i + 2] * s
        out.append(f'<rect x="{x:.1f}" y="{sy}" width="{WALL*s:.1f}" height="{DEPTH*s}" fill="#222"/>')
    out.append(f'<rect x="{ox}" y="{sy}" width="{SIZE*s:.1f}" height="{DEPTH*s}" fill="none" stroke="#888" stroke-dasharray="4 3"/>')
    vdim(ox + SIZE * s + 14, sy, sy + DEPTH * s, f"{DEPTH:.0f}")
    out.append(f'<text x="{ox}" y="{sy + DEPTH*s + 22}" font-weight="bold">Section through a hole row (light travels along the 50 mm depth)</text>')
    half = math.degrees(math.atan(CELL / DEPTH))
    diag = math.degrees(math.atan(CELL * math.sqrt(2) / DEPTH))
    out.append(f'<text x="{ox}" y="{sy + DEPTH*s + 40}" fill="#444">Light acceptance about +/-{half:.1f} deg (along a cell axis), +/-{diag:.1f} deg (diagonal).</text>')
    out.append("</svg>")
    Path(path).write_text("\n".join(out), encoding="utf-8")


def svg_concept(path):
    """Why a diffuse backlight shrinks a polished cylinder's silhouette, and how collimation fixes it."""
    R = 40.0
    out = ['<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 900 540" width="900" height="540" font-family="Arial" font-size="13">',
           '<rect width="100%" height="100%" fill="white"/>',
           '<text x="20" y="26" font-size="16" font-weight="bold">Why the collimator: polished pin seen end-on (light travels left to right toward the lens)</text>']
    for k, (title, cx, collim) in enumerate([("Diffuse backlight (now)", 230, False), ("Collimated backlight (louvre)", 680, True)]):
        cy = 290
        px = cx - 190  # panel x
        out.append(f'<text x="{cx-130}" y="56" font-weight="bold">{title}</text>')
        out.append(f'<rect x="{px}" y="{cy-215}" width="14" height="430" fill="#2a2aff"/>')
        if collim:
            for j in range(-8, 9):
                out.append(f'<rect x="{px+14}" y="{cy+j*21-1}" width="48" height="2" fill="#222"/>')
        out.append(f'<circle cx="{cx}" cy="{cy}" r="{R}" fill="#888" stroke="#222" stroke-width="2"/>')
        lensx = cx + 150
        out.append(f'<rect x="{lensx}" y="{cy-60}" width="16" height="120" fill="#555"/><text x="{lensx-6}" y="{cy+82}">lens</text>')
        if not collim:
            for a_deg in (70, 78, 85):
                a = math.radians(a_deg)
                Px, Py = cx + R * math.cos(a), cy - R * math.sin(a)
                rx, ry = math.cos(2 * a), -math.sin(2 * a)  # reverse-ray direction (screen y down)
                t = (Px - (px + 14)) / (-rx)
                Sx, Sy = Px + t * rx, Py + t * ry
                out.append(f'<polyline points="{Sx:.1f},{Sy:.1f} {Px:.1f},{Py:.1f} {lensx:.1f},{Py:.1f}" fill="none" stroke="#e08000" stroke-width="1.6"/>')
            out.append(f'<text x="{cx-120}" y="{cy+150}" fill="#a05000">Rays from the panel reflect off the</text>')
            out.append(f'<text x="{cx-120}" y="{cy+168}" fill="#a05000">outer rim straight into the lens: a bright</text>')
            out.append(f'<text x="{cx-120}" y="{cy+186}" fill="#a05000">rim eats into the dark silhouette.</text>')
        else:
            for j in range(-3, 4):
                y = cy + j * 15
                hit = abs(y - cy) < R
                x_end = cx - math.sqrt(max(R * R - (y - cy) ** 2, 0)) if hit else lensx
                out.append(f'<line x1="{px+62}" y1="{y}" x2="{x_end:.1f}" y2="{y}" stroke="#2a2aff" stroke-width="1.4"/>')
                if not hit:
                    out.append(f'<line x1="{x_end:.1f}" y1="{y}" x2="{lensx}" y2="{y}" stroke="#2a2aff" stroke-width="1.4"/>')
            out.append(f'<text x="{cx-120}" y="{cy+150}" fill="#003">Rays all run parallel to the lens axis: only the</text>')
            out.append(f'<text x="{cx-120}" y="{cy+168}" fill="#003">true tangent ray passes, so the silhouette</text>')
            out.append(f'<text x="{cx-120}" y="{cy+186}" fill="#003">edge is the real edge of the pin.</text>')
    out.append("</svg>")
    Path(path).write_text("\n".join(out), encoding="utf-8")


def main():
    outdir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("hardware/collimator")
    outdir.mkdir(parents=True, exist_ok=True)
    tris = build_triangles()
    bad, vol = check_mesh(tris)
    write_stl(outdir / "collimator.stl", tris)
    svg_drawing(outdir / "collimator_drawing.svg")
    svg_concept(outdir / "collimator_concept.svg")
    grid = N * CELL + (N - 1) * WALL
    open_frac = (N * CELL) ** 2 / (SIZE * SIZE)
    print(f"size {SIZE:.2f} x {SIZE:.2f} x {DEPTH} mm; grid {grid:.1f} mm; triangles {len(tris)}")
    print(f"watertight: bad edges {bad}; volume {vol:.1f} mm^3 ({vol/1000:.2f} cm^3)")
    print(f"open area {open_frac*100:.1f} % of plate ({(CELL/(CELL+WALL))**2*100:.1f} % inside the grid)")
    print(f"acceptance +/-{math.degrees(math.atan(CELL/DEPTH)):.2f} deg axis, +/-{math.degrees(math.atan(CELL*math.sqrt(2)/DEPTH)):.2f} deg diagonal")


if __name__ == "__main__":
    main()
