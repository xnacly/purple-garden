# Sum of gcd(i, j) for i, j in [1, 100] = 31080. Idiomatic Python: iterative
# gcd (CPython has no TCO) inside nested for-loops.
import sys


def gcd(a, b):
    while b != 0:
        a, b = b, a % b
    return a


s = 0
for i in range(1, 101):
    for j in range(1, 101):
        s += gcd(i, j)
if s != 31080:
    sys.exit(1)
