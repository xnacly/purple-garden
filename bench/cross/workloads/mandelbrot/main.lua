-- 300 samples on the real axis [-1.5, 1.5), 50 max iters; sum the escape
-- counts. Same check as main.garden (sum > 0). Tail-recursive like the garden
-- source; Lua's proper tail calls make both functions loops. Check order
-- matches exactly: escape test on the current z, then advance.
local function mandel_iter(zr, zi, cr, ci, i, max)
  if i == max then return max end
  if zr * zr + zi * zi > 4.0 then return i end
  return mandel_iter(zr * zr - zi * zi + cr, 2.0 * zr * zi + ci, cr, ci, i + 1, max)
end
local function slice_sum(i, n, max, acc)
  if i == n then return acc end
  return slice_sum(i + 1, n, max, acc + mandel_iter(0.0, 0.0, i / 100.0 - 1.5, 0.0, 0, max))
end
if not (slice_sum(0, 300, 50, 0) > 0) then os.exit(1) end
