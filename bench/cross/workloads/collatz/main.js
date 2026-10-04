// Sum of Collatz step counts for 1..1000 = 59542. Integer /, %, * mix.
// Loop form; garden's is_even helper is inlined as the idiomatic n % 2 test.
function collatzSteps(n) {
  let acc = 0;
  while (n !== 1) {
    n = n % 2 === 0 ? n / 2 : 3 * n + 1;
    acc++;
  }
  return acc;
}
let s = 0;
for (let i = 1; i <= 1000; i++) s += collatzSteps(i);
if (s !== 59542) process.exit(1);
