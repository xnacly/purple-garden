-- 6000 samples on the real axis [-1.5, 1.5), 500 max iters; sum of the escape
-- counts = 1761213, asserted exactly: every runtime performs the identical
-- IEEE op sequence. Loop form (LuaJIT traces loops far better than recursion,
-- even tail calls). Check order matches main.garden exactly: escape test on
-- the current z, then advance.
local function mandel_iter(cr, ci, max)
  local zr, zi = 0.0, 0.0
  for i = 0, max - 1 do
    if zr * zr + zi * zi > 4.0 then return i end
    zr, zi = zr * zr - zi * zi + cr, 2.0 * zr * zi + ci
  end
  return max
end
local acc = 0
for i = 0, 5999 do
  acc = acc + mandel_iter(i / 2000.0 - 1.5, 0.0, 500)
end
if acc ~= 1761213 then os.exit(1) end
