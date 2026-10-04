-- Count primes in [2, 10000] by trial division. Idiomatic Lua: loops.
local function is_composite(n)
  local d = 2
  while d * d <= n do
    if n % d == 0 then return true end
    d = d + 1
  end
  return false
end
local c = 0
for i = 2, 10000 do
  if not is_composite(i) then c = c + 1 end
end
if c ~= 1229 then os.exit(1) end
