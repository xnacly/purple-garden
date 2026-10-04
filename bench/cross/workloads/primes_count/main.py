# Count primes in [2, 10000] by trial division = 1229. Loops; the inner guard
# d*d <= n walks divisors up to the square root, as in main.garden.
import sys


def is_composite(n):
    d = 2
    while d * d <= n:
        if n % d == 0:
            return True
        d += 1
    return False


c = 0
for i in range(2, 10001):
    if not is_composite(i):
        c += 1
if c != 1229:
    sys.exit(1)
