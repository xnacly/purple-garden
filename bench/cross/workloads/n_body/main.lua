-- Simplified 2-body n-body (Sun + Jupiter), 200000 semi-implicit Euler steps
-- in the exact op order of main.garden (velocities first, then positions, so
-- the integrator is symplectic and long runs stay bounded). Independent
-- reference (Python, IEEE f64): -1.968360027403725. Loop form.
local sqrt = math.sqrt
local m1, m2 = 39.478417604357434, 0.03769367487038906
local function advance(x1, y1, z1, vx1, vy1, vz1, x2, y2, z2, vx2, vy2, vz2, n)
  for _ = 1, n do
    local dx, dy, dz = x1 - x2, y1 - y2, z1 - z2
    local d2 = dx * dx + dy * dy + dz * dz
    local mag = 0.01 / (d2 * sqrt(d2))
    vx1 = vx1 - dx * m2 * mag
    vy1 = vy1 - dy * m2 * mag
    vz1 = vz1 - dz * m2 * mag
    vx2 = vx2 + dx * m1 * mag
    vy2 = vy2 + dy * m1 * mag
    vz2 = vz2 + dz * m1 * mag
    x1 = x1 + 0.01 * vx1
    y1 = y1 + 0.01 * vy1
    z1 = z1 + 0.01 * vz1
    x2 = x2 + 0.01 * vx2
    y2 = y2 + 0.01 * vy2
    z2 = z2 + 0.01 * vz2
  end
  return x2 + y2 + z2 + vx2 + vy2 + vz2
end
local r = advance(
  0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
  4.84143144246472090, -1.16032004402742839, -0.103622044471123109,
  0.606326392995832020, 2.81198684491626016, -0.0252183616598876821,
  200000)
if not (r > -1.968361 and r < -1.968359) then os.exit(1) end
