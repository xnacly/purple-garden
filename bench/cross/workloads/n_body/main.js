// Simplified 2-body n-body (Sun + Jupiter), 200000 semi-implicit Euler steps
// in the exact op order of main.garden (velocities first, then positions, so
// the integrator is symplectic and long runs stay bounded). Independent
// reference (Python, IEEE f64): -1.968360027403725. Loop form: V8/JSC have no
// TCO.
function advance(x1, y1, z1, vx1, vy1, vz1, x2, y2, z2, vx2, vy2, vz2, n) {
  const m1 = 39.478417604357434;
  const m2 = 0.03769367487038906;
  for (; n > 0; n--) {
    const dx = x1 - x2;
    const dy = y1 - y2;
    const dz = z1 - z2;
    const d2 = dx * dx + dy * dy + dz * dz;
    const mag = 0.01 / (d2 * Math.sqrt(d2));
    vx1 -= dx * m2 * mag;
    vy1 -= dy * m2 * mag;
    vz1 -= dz * m2 * mag;
    vx2 += dx * m1 * mag;
    vy2 += dy * m1 * mag;
    vz2 += dz * m1 * mag;
    x1 += 0.01 * vx1;
    y1 += 0.01 * vy1;
    z1 += 0.01 * vz1;
    x2 += 0.01 * vx2;
    y2 += 0.01 * vy2;
    z2 += 0.01 * vz2;
  }
  return x2 + y2 + z2 + vx2 + vy2 + vz2;
}
const r = advance(
  0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
  4.84143144246472090, -1.16032004402742839, -0.103622044471123109,
  0.606326392995832020, 2.81198684491626016, -0.0252183616598876821,
  200000,
);
if (!(r > -1.968361 && r < -1.968359)) process.exit(1);
