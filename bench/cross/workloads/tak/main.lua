-- Knuth's y-variant Takeuchi (returns y in the base case), ported literally
-- from main.garden: tak(18, 12, 6) = 18. About 12.6M calls; shallow depth.
local function tak(x, y, z)
  if y < x then
    return tak(tak(x - 1, y, z), tak(y - 1, z, x), tak(z - 1, x, y))
  end
  return y
end
if tak(18, 12, 6) ~= 18 then os.exit(1) end
