// Sum of gcd(i, j) for i, j in [1, 1000] = 4449880. Idiomatic JS: iterative
// gcd (no TCO) inside nested loops.
function gcd(a, b) {
  while (b !== 0) {
    const t = a % b;
    a = b;
    b = t;
  }
  return a;
}
let s = 0;
for (let i = 1; i <= 1000; i++) {
  for (let j = 1; j <= 1000; j++) {
    s += gcd(i, j);
  }
}
if (s !== 4449880) process.exit(1);
