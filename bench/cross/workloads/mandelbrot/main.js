// 300 samples on the real axis [-1.5, 1.5), 50 max iters; sum the escape
// counts. Same check as mandelbrot.garden (sum > 0). The per-pixel recursion
// is a loop here; the arithmetic and check order match exactly: test escape
// with the current z, then advance.
function mandelIter(cr, ci, max) {
  let zr = 0.0;
  let zi = 0.0;
  for (let i = 0; i < max; i++) {
    if (zr * zr + zi * zi > 4.0) return i;
    const nzr = zr * zr - zi * zi + cr;
    zi = 2.0 * zr * zi + ci;
    zr = nzr;
  }
  return max;
}
let acc = 0;
for (let i = 0; i < 300; i++) {
  acc += mandelIter(i / 100.0 - 1.5, 0.0, 50);
}
if (!(acc > 0)) process.exit(1);
