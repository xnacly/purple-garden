// Count primes in [2, 10000] by trial division. Idiomatic JS: loops.
function isComposite(n) {
  for (let d = 2; d * d <= n; d++) {
    if (n % d === 0) return true;
  }
  return false;
}
let c = 0;
for (let i = 2; i <= 10000; i++) {
  if (!isComposite(i)) c++;
}
if (c !== 1229) process.exit(1);
