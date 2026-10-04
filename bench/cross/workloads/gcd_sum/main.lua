-- Sum of gcd(i, j) for i, j in [1, 1000] = 4449880. Iterative Euclid inside
-- nested loops. A tail-recursive gcd would hit LuaJIT's weak spot (recursion,
-- even through proper tail calls) rather than measure the arithmetic.
local function gcd(a, b)
  while b ~= 0 do
    a, b = b, a % b
  end
  return a
end
local s = 0
for i = 1, 1000 do
  for j = 1, 1000 do
    s = s + gcd(i, j)
  end
end
if s ~= 4449880 then os.exit(1) end
