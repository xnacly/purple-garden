# 6000 samples on the real axis [-1.5, 1.5), 500 max iters; sum of the escape
# counts = 1761213, asserted exactly: every runtime performs the identical
# IEEE op sequence. Loop form; check order matches main.garden exactly: test
# escape with the current z, then advance.
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
for i in range(6000):
    acc += mandel_iter(i / 2000.0 - 1.5, 0.0, 500)
if acc != 1761213:
    sys.exit(1)
