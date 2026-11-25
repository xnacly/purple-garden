// 72000 samples on the real axis [-1.5, 1.5), 500 max iters; sum of the escape
// counts = 21130815, asserted exactly: every runtime performs the identical
// IEEE op sequence. Loop form; check order matches main.garden exactly: test
// escape with the current z, then advance.
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
for (let i = 0; i < 72000; i++) {
  acc += mandelIter(i / 24000.0 - 1.5, 0.0, 500);
}
if (acc !== 21130815) process.exit(1);
