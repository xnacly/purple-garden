-- Sum of gcd(i, j) for i, j in [1, 100]. Idiomatic Lua: proper tail call.
local function gcd(a, b)
  if b == 0 then return a end
  return gcd(b, a % b)
end
local s = 0
for i = 1, 100 do
  for j = 1, 100 do
    s = s + gcd(i, j)
  end
end
if s ~= 31080 then os.exit(1) end
