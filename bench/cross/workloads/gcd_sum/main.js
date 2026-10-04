// Sum of gcd(i, j) for i, j in [1, 100]. Idiomatic JS: iterative gcd (V8 has no TCO).
function gcd(a, b) {
  while (b !== 0) {
    const t = a % b;
    a = b;
    b = t;
  }
  return a;
}
let s = 0;
for (let i = 1; i <= 100; i++) {
  for (let j = 1; j <= 100; j++) {
    s += gcd(i, j);
  }
}
if (s !== 31080) process.exit(1);
