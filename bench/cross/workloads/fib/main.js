// fib(30) doubly-recursive + fibt(50) accumulator form. Same two checks as
// main.garden. fibt is a loop here: V8/JSC have no TCO, and garden's tailcall
// pass turns its recursion into one anyway.
function fib(n) {
  if (n === 0) return 0;
  if (n === 1) return 1;
  return fib(n - 1) + fib(n - 2);
}
function fibt(n, a, b) {
  while (n > 1) {
    const t = a + b;
    a = b;
    b = t;
    n--;
  }
  return n === 0 ? a : b;
}
if (fib(30) !== 832040) process.exit(1);
if (fibt(50, 0, 1) !== 12586269025) process.exit(1);
