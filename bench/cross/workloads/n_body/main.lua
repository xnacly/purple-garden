-- Simplified 2-body n-body (Sun + Jupiter), 1000 Euler steps, in the exact op
-- order of main.garden; tail-recursive like the source (proper tail calls).
-- Independent reference (Python, IEEE f64): 0.0688067665339183.
local sqrt = math.sqrt
local m1, m2 = 39.478417604357434, 0.03769367487038906
local function advance(x1, y1, z1, vx1, vy1, vz1, x2, y2, z2, vx2, vy2, vz2, n)
  if n == 0 then return x2 + y2 + z2 + vx2 + vy2 + vz2 end
  local dx, dy, dz = x1 - x2, y1 - y2, z1 - z2
  local d2 = dx * dx + dy * dy + dz * dz
  local mag = 0.01 / (d2 * sqrt(d2))
  local nvx1, nvy1, nvz1 = vx1 - dx * m2 * mag, vy1 - dy * m2 * mag, vz1 - dz * m2 * mag
  local nvx2, nvy2, nvz2 = vx2 + dx * m1 * mag, vy2 + dy * m1 * mag, vz2 + dz * m1 * mag
  return advance(
    x1 + 0.01 * nvx1, y1 + 0.01 * nvy1, z1 + 0.01 * nvz1, nvx1, nvy1, nvz1,
    x2 + 0.01 * nvx2, y2 + 0.01 * nvy2, z2 + 0.01 * nvz2, nvx2, nvy2, nvz2,
    n - 1)
end
local r = advance(
  0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
  4.84143144246472090, -1.16032004402742839, -0.103622044471123109,
  0.606326392995832020, 2.81198684491626016, -0.0252183616598876821,
  1000)
if not (r > 0.0688067 and r < 0.0688068) then os.exit(1) end
