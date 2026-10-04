// Knuth's y-variant Takeuchi (returns y in the base case), ported literally
// from tak.garden: tak(18, 12, 6) = 18. Three-way recursion; depth is shallow
// but the call count is large.
function tak(x, y, z) {
  return y < x ? tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y)) : y;
}
if (tak(18, 12, 6) !== 18) process.exit(1);
