"""Generate golden reference vectors for metrology-core from scipy/numpy.

Run with a Python that has numpy + scipy:  python tools/ref/gen_golden.py
Writes crates/metrology-core/tests/golden/*.txt (plain text, full-precision floats).
The Rust implementation must reproduce these to ~1e-9 (see tests/golden_parity.rs).
"""
import math
import pathlib

import numpy as np
from scipy import ndimage as ndi
from scipy.special import erf

OUT = pathlib.Path(__file__).resolve().parents[2] / "crates" / "metrology-core" / "tests" / "golden"
OUT.mkdir(parents=True, exist_ok=True)


def f(x):
    return repr(float(x))


def row(name, values):
    return f"{name}: " + " ".join(f(v) for v in np.ravel(values)) + "\n"


# ---------------------------------------------------------------- spline sampling
def gen_spline(name="spline", h=40, w=50, seed=20261107, n_interior=120, far=False):
    rng = np.random.default_rng(seed)
    # Round to float32 first: the Rust core takes f32 images, so scipy must see the same pixels.
    img = ndi.gaussian_filter(rng.random((h, w)) * 255.0, 1.1).astype(np.float32).astype(np.float64)
    pts = []
    # interior, near each border, just outside, and well outside
    for _ in range(n_interior):
        pts.append((rng.uniform(0, h - 1), rng.uniform(0, w - 1)))
    for _ in range(40):
        pts.append((rng.uniform(-3.5, 3.5), rng.uniform(-3.5, w + 3.5)))
        pts.append((rng.uniform(h - 4.5, h + 3.5), rng.uniform(-3.5, w + 3.5)))
        pts.append((rng.uniform(-3.5, h + 3.5), rng.uniform(-3.5, 3.5)))
        pts.append((rng.uniform(-3.5, h + 3.5), rng.uniform(w - 4.5, w + 3.5)))
    if far:  # replicate extension far outside the image
        pts += [(-100.0, 50.0), (1e6, 1e6), (-1e4, -1e4), (50.0, 1e5), (h / 2, -37.25)]
    pts.append((10.0, 10.0))  # exactly on a grid node
    pts.append((0.0, 0.0))
    pts.append((h - 1.0, w - 1.0))
    pts = np.array(pts)
    expect = ndi.map_coordinates(img, pts.T, order=3, mode="nearest")
    s = f"width: {w}\nheight: {h}\n"
    s += row("image", img)
    s += row("points_yx", pts)
    s += row("expected", expect)
    (OUT / f"{name}.txt").write_text(s)


# ---------------------------------------------------------------- 1-D filters
def gen_filters():
    rng = np.random.default_rng(7)
    sig = np.cumsum(rng.normal(0, 3, 60)) + 100.0
    s = row("signal", sig)
    for sigma in (0.7, 1.0, 1.5):
        s += row(f"gauss_{sigma}", ndi.gaussian_filter1d(sig, sigma, mode="nearest"))
    s += row("gradient", np.gradient(sig))
    (OUT / "filters.txt").write_text(s)


# ---------------------------------------------------------------- edge locator reference
def locate_edges(profile, sigma, min_strength, polarity, refine="parabola"):
    """Reference for the Rust locate_edges_with: smooth, np.gradient, peaks of the signed
    gradient (neighbours of the opposite sign count as zero), 3-point refinement.
    polarity: +1 rising, -1 falling, 0 either. refine: "parabola" | "gaussian"."""
    g = np.gradient(ndi.gaussian_filter1d(np.asarray(profile, float), sigma, mode="nearest"))
    out = []
    for i in range(1, len(g) - 1):
        sign = 1 if g[i] > 0 else -1
        a, b, c = max(sign * g[i - 1], 0.0), sign * g[i], max(sign * g[i + 1], 0.0)
        if b < min_strength or not (b > a and b >= c):
            continue
        if polarity != 0 and sign != polarity:
            continue
        if refine == "gaussian" and a > 0.0 and c > 0.0:
            la, lb, lc = math.log(a), math.log(b), math.log(c)
            denom = la - 2.0 * lb + lc
            d = 0.5 * (la - lc) / denom if denom != 0.0 else 0.0
            strength = math.exp(lb - 0.25 * (la - lc) * d)
        else:
            denom = a - 2.0 * b + c
            d = 0.5 * (a - c) / denom if denom != 0.0 else 0.0
            strength = b - 0.25 * (a - c) * d
        out.append((i + d, strength, sign))
    return out


def gen_edges():
    rng = np.random.default_rng(99)
    n = 80
    x = np.arange(n, dtype=float)
    cases = []
    # single rising and falling steps at known subpixel positions, blurred, with noise
    for x0, lo, hi, polarity_step in [(30.37, 20, 200, 1), (44.81, 210, 25, -1), (12.5, 40, 190, 1)]:
        prof = lo + (hi - lo) * 0.5 * (1 + erf((x - x0) / (1.2 * math.sqrt(2)))) + rng.normal(0, 1.5, n)
        cases.append(prof)
    # sawtooth-like periodic structure (thread silhouette crests/roots)
    period = 11.3
    saw = 120 + 60 * ((x / period) % 1.0 - 0.5)
    cases.append(saw + rng.normal(0, 0.8, n))
    # thin dark line: falling edge immediately followed by a rising edge
    line = 200 - 160 * (0.5 * (1 + erf((x - 28.5) / (0.6 * math.sqrt(2)))) - 0.5 * (1 + erf((x - 31.5) / (0.6 * math.sqrt(2)))))
    cases.append(line + rng.normal(0, 0.5, n))
    # pure noise (no edge above threshold)
    cases.append(100 + rng.normal(0, 1.0, n))
    s = f"count: {len(cases)}\n"
    for k, prof in enumerate(cases):
        s += row(f"profile_{k}", prof)
        for sigma, thr, pol in [(1.0, 5.0, 0), (1.0, 5.0, 1), (1.5, 8.0, -1)]:
            for refine in ("parabola", "gaussian"):
                edges = locate_edges(prof, sigma, thr, pol, refine)
                flat = [v for e in edges for v in e]
                s += row(f"edges_{k}_s{sigma}_t{thr}_p{pol}_{refine}", flat)
    (OUT / "edges.txt").write_text(s)


if __name__ == "__main__":
    gen_spline()
    gen_spline("spline_large", h=200, w=200, seed=424242, n_interior=150, far=True)
    gen_filters()
    gen_edges()
    print("wrote", [p.name for p in sorted(OUT.glob("*.txt"))], "numpy", np.__version__)


