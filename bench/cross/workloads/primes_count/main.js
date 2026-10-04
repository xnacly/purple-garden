// Count primes in [2, 100000] by trial division = 9592. Idiomatic JS: loops;
// the inner guard d*d <= n walks divisors up to the square root, as in
// main.garden.
function isComposite(n) {
  for (let d = 2; d * d <= n; d++) {
    if (n % d === 0) return true;
  }
  return false;
}
let c = 0;
for (let i = 2; i <= 100000; i++) {
  if (!isComposite(i)) c++;
}
if (c !== 9592) process.exit(1);
