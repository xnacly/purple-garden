-- fib(28) doubly-recursive + fibt(50) accumulator form; same two checks as
-- main.garden. fibt stays tail-recursive like the garden source: Lua has
-- proper tail calls, so it runs as a loop.
local function fib(n)
  if n == 0 then return 0 end
  if n == 1 then return 1 end
  return fib(n - 1) + fib(n - 2)
end
local function fibt(n, a, b)
  if n == 0 then return a end
  if n == 1 then return b end
  return fibt(n - 1, b, a + b)
end
if fib(28) ~= 317811 then os.exit(1) end
if fibt(50, 0, 1) ~= 12586269025 then os.exit(1) end
