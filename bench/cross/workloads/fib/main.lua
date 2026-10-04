-- fib(30) doubly-recursive + fibt(50) accumulator form; same two checks as
-- main.garden. fib stays recursive (the recursion is the workload). fibt is a
-- loop: LuaJIT's trace compiler handles loops well and recursion poorly even
-- through proper tail calls, so a tail-recursive fibt would measure the JIT's
-- blind spot rather than the language.
local function fib(n)
  if n == 0 then return 0 end
  if n == 1 then return 1 end
  return fib(n - 1) + fib(n - 2)
end
local function fibt(n, a, b)
  while n > 1 do
    a, b = b, a + b
    n = n - 1
  end
  if n == 0 then return a end
  return b
end
if fib(30) ~= 832040 then os.exit(1) end
if fibt(50, 0, 1) ~= 12586269025 then os.exit(1) end
