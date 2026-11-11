// 32 ints and 16 doubles live per call to `mix`, in the op order of main.garden. Integer
// division and modulo truncate like Rust; every value stays far below 2^53, so doubles hold
// them exactly. Loop form.
const trunc = Math.trunc;
const div = (a, b) => trunc(a / b);
const mod = (a, b) => a - b * trunc(a / b);
function weight(n) {
  if (n === 0) return 3;
  if (n === 1) return 5;
  if (n === 2) return 7;
  if (n === 3) return 11;
  return 13;
}
function mix(i) {
  const a1 = i * 3 + 1;
  const a2 = a1 - 7 * i;
  const a3 = div(a1, 7);
  const a4 = div(a2, 3);
  const a5 = mod(a2, 5);
  const a6 = div(a1, 4);
  const a7 = div(a2, 8);
  const a8 = mod(a2, 16);
  const a9 = a1 * a2;
  const a10 = div(a9, a3 + 1);
  const a11 = mod(a9, a6 + 2);
  const a12 = weight(mod(a1, 5));
  const a13 = Math.abs(a2);
  const a14 = a13 - a12;
  const a15 = a14 * 3;
  const a16 = div(a15, 5);
  const a17 = mod(a16, 9);
  const a18 = a17 + a1;
  const a19 = a18 * a5;
  const a20 = a19 - a4;
  const a21 = div(a20, a12 + 0);
  const a22 = a21 + a7;
  const a23 = a22 * 2;
  const a24 = a23 - a8;
  const a25 = mod(a24, 11);
  const a26 = a25 + a10;
  const a27 = a26 - a11;
  const a28 = a27 * 5;
  const a29 = div(a28, 6);
  const a30 = a29 + a3;
  const a31 = a30 - a6;
  const a32 = a31 + a2;
  const d1 = a1 * 0.5;
  const d2 = a2 / 3.0;
  const d3 = d1 - d2;
  const d4 = d1 * d2;
  const d5 = Math.sqrt(d1 + 1.0);
  const d6 = d3 / (d5 + 1.0);
  const d7 = d4 + d6;
  const d8 = a5 - d7;
  const d9 = d8 * 0.25;
  const d10 = d9 + d1;
  const d11 = d10 - d5;
  const d12 = d11 / 7.0;
  const d13 = d12 + d3;
  const d14 = d13 + a17;
  const d15 = d14 - d2;
  const d16 = d15 * 0.125;
  const pa = a10, pb = d8;
  const c1 = d3 > d4 ? 1 : 2;
  const c2 = d1 < d5 ? 4 : 8;
  const c3 = a2 < a3 ? 16 : 32;
  const ints = a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12 + a13 + a14 + a15 + a16
    + a17 + a18 + a19 + a20 + a21 + a22 + a23 + a24 + a25 + a26 + a27 + a28 + a29 + a30 + a31 + a32;
  const doubles = d1 + d2 + d3 + d4 + d5 + d6 + d7 + d8 + d9 + d10 + d11 + d12 + d13 + d14 + d15 + d16;
  return mod(ints, 1000003) + mod(trunc(doubles), 1009) + mod(pa, 101) + mod(trunc(pb), 13) + c1 + c2 + c3;
}
let acc = 0;
for (let i = 0; i < 1000000; i++) acc += mix(i);
if (acc !== -500321797617) process.exit(1);
