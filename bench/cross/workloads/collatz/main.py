# Sum of Collatz step counts for 1..1000 = 59542. Integer //, %, * mix.
# Loop form; garden's is_even helper is inlined as the idiomatic n % 2 test.
import sys


def collatz_steps(n):
    acc = 0
    while n != 1:
        n = n // 2 if n % 2 == 0 else 3 * n + 1
        acc += 1
    return acc


s = 0
for i in range(1, 1001):
    s += collatz_steps(i)
if s != 59542:
    sys.exit(1)
