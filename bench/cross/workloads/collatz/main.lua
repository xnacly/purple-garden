-- Sum of Collatz step counts for 1..1000 = 59542. Tail-recursive like the
-- garden source (proper tail calls); is_even inlined as the idiomatic n % 2.
-- n / 2 is exact for even n in doubles; the largest value reached is ~250k.
local function collatz_steps(n, acc)
  if n == 1 then return acc end
  if n % 2 == 0 then return collatz_steps(n / 2, acc + 1) end
  return collatz_steps(3 * n + 1, acc + 1)
end
local function sum_steps(i, n, acc)
  if i > n then return acc end
  return sum_steps(i + 1, n, acc + collatz_steps(i, 0))
end
if sum_steps(1, 1000, 0) ~= 59542 then os.exit(1) end
