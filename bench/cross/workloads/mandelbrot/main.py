# 300 samples on the real axis [-1.5, 1.5), 50 max iters; sum the escape
# counts. Same check as mandelbrot.garden (sum > 0). Loop form; arithmetic and
# check order match exactly: test escape with the current z, then advance.
import sys


def mandel_iter(cr, ci, mx):
    zr = 0.0
    zi = 0.0
    for i in range(mx):
        if zr * zr + zi * zi > 4.0:
            return i
        nzr = zr * zr - zi * zi + cr
        zi = 2.0 * zr * zi + ci
        zr = nzr
    return mx


acc = 0
for i in range(300):
    acc += mandel_iter(i / 100.0 - 1.5, 0.0, 50)
if not acc > 0:
    sys.exit(1)
