// Count primes in [2, 1000000] by trial division = 78498. Idiomatic JS: loops;
// the inner guard d*d <= n walks divisors up to the square root, as in
// main.garden.
function isComposite(n) {
  for (let d = 2; d * d <= n; d++) {
    if (n % d === 0) return true;
  }
  return false;
}
let c = 0;
for (let i = 2; i <= 1000000; i++) {
  if (!isComposite(i)) c++;
}
if (c !== 78498) process.exit(1);
