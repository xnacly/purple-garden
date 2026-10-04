# ack(3, 8) = 2045. Inherently recursive, ~2k frames deep. CPython's default
# recursion limit is 1000, so it must be raised; 3.11+ keeps pure-Python frames
# off the C stack so the depth itself is safe.
import sys

sys.setrecursionlimit(100_000)


def ack(m, n):
    if m == 0:
        return n + 1
    if n == 0:
        return ack(m - 1, 1)
    return ack(m - 1, ack(m, n - 1))


if ack(3, 8) != 2045:
    sys.exit(1)
