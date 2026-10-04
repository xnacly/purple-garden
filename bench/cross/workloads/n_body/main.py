# Simplified 2-body n-body (Sun + Jupiter), 1000 Euler steps, in the exact op
# order of main.garden. This file is also the independent reference: it prints
# nothing but computes 0.0688067665339183, which the garden source matches to
# <1e-7, so the check band is tight. Loop form: CPython has no TCO.
import math
import sys


def advance(x1, y1, z1, vx1, vy1, vz1, x2, y2, z2, vx2, vy2, vz2, n):
    m1 = 39.478417604357434
    m2 = 0.03769367487038906
    for _ in range(n):
        dx = x1 - x2
        dy = y1 - y2
        dz = z1 - z2
        d2 = dx * dx + dy * dy + dz * dz
        mag = 0.01 / (d2 * math.sqrt(d2))
        vx1 -= dx * m2 * mag
        vy1 -= dy * m2 * mag
        vz1 -= dz * m2 * mag
        vx2 += dx * m1 * mag
        vy2 += dy * m1 * mag
        vz2 += dz * m1 * mag
        x1 += 0.01 * vx1
        y1 += 0.01 * vy1
        z1 += 0.01 * vz1
        x2 += 0.01 * vx2
        y2 += 0.01 * vy2
        z2 += 0.01 * vz2
    return x2 + y2 + z2 + vx2 + vy2 + vz2


r = advance(
    0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
    4.84143144246472090, -1.16032004402742839, -0.103622044471123109,
    0.606326392995832020, 2.81198684491626016, -0.0252183616598876821,
    1000,
)
if not (0.0688067 < r < 0.0688068):
    sys.exit(1)
