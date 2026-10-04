-- Count primes in [2, 100000] by trial division = 9592. Loops; the inner
-- guard d*d <= n walks divisors up to the square root, as in main.garden.
local function is_composite(n)
  local d = 2
  while d * d <= n do
    if n % d == 0 then return true end
    d = d + 1
  end
  return false
end
local c = 0
for i = 2, 100000 do
  if not is_composite(i) then c = c + 1 end
end
if c ~= 9592 then os.exit(1) end
