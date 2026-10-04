// Sum of Collatz step counts for 1..30000 = 2864311. Integer /, %, * mix.
// Loop form; garden's is_even helper is inlined as the idiomatic n % 2 test.
// Sized so compute dominates process startup (~2.9M steps); the peak value
// reached is ~106M, exact in a double.
function collatzSteps(n) {
  let acc = 0;
  while (n !== 1) {
    n = n % 2 === 0 ? n / 2 : 3 * n + 1;
    acc++;
  }
  return acc;
}
let s = 0;
for (let i = 1; i <= 30000; i++) s += collatzSteps(i);
if (s !== 2864311) process.exit(1);
