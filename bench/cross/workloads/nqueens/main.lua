-- 12 queens have 14200 solutions (OEIS A000170). Same encoding as the garden
-- version: placed queens packed into one number as base-16 digits (column + 1),
-- newest lowest, peeled with math.floor and %. 12 digits of 4 bits stay below
-- 2^53, so LuaJIT's doubles are exact.
local floor = math.floor

local function safe(placed, col, dist)
  while placed ~= 0 do
    local q = placed % 16 - 1
    if q == col or q - col == dist or col - q == dist then return false end
    placed = floor(placed / 16)
    dist = dist + 1
  end
  return true
end

local function solve(n, row, col, placed)
  if row == n then return 1 end
  if col == n then return 0 end
  if safe(placed, col, 1) then
    return solve(n, row + 1, 0, placed * 16 + col + 1) + solve(n, row, col + 1, placed)
  end
  return solve(n, row, col + 1, placed)
end

if solve(12, 0, 0, 0) ~= 14200 then os.exit(1) end
