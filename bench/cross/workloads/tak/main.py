# Knuth's y-variant Takeuchi (returns y in the base case), ported literally
# from tak.garden: tak(18, 12, 6) = 18. Three-way recursion; shallow depth,
# large call count.
import sys


def tak(x, y, z):
    if y < x:
        return tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y))
    return y


if tak(18, 12, 6) != 18:
    sys.exit(1)
