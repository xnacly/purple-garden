# Sum of gcd(i, j) for i, j in [1, 1000] = 4449880. Idiomatic Python: iterative
# gcd (CPython has no TCO) inside nested for-loops.
import sys


def gcd(a, b):
    while b != 0:
        a, b = b, a % b
    return a


s = 0
for i in range(1, 1001):
    for j in range(1, 1001):
        s += gcd(i, j)
if s != 4449880:
    sys.exit(1)
