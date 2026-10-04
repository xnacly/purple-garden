# fib(30) doubly-recursive + fibt(50) accumulator form. Same two checks as
# main.garden. fibt is a loop: CPython has no TCO.
import sys


def fib(n):
    if n == 0:
        return 0
    if n == 1:
        return 1
    return fib(n - 1) + fib(n - 2)


def fibt(n, a, b):
    while n > 1:
        a, b = b, a + b
        n -= 1
    return a if n == 0 else b


if fib(30) != 832040:
    sys.exit(1)
if fibt(50, 0, 1) != 12586269025:
    sys.exit(1)
