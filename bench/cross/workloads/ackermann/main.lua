-- ack(3, 8) = 2045. Inherently recursive, ~2k frames deep; Lua-to-Lua calls
-- don't consume C stack, so the default limits are fine.
local function ack(m, n)
  if m == 0 then return n + 1 end
  if n == 0 then return ack(m - 1, 1) end
  return ack(m - 1, ack(m, n - 1))
end
if ack(3, 8) ~= 2045 then os.exit(1) end
