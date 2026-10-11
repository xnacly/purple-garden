// 13 queens have 73712 solutions (OEIS A000170). Same encoding as the garden
// version: placed queens packed into one number as base-16 digits (column + 1),
// newest lowest, peeled with Math.floor and %. 13 digits of 4 bits stay below
// 2^53, so plain doubles are exact; bit operators would truncate to 32 bits.
function safe(placed, col, dist) {
  while (placed !== 0) {
    const q = (placed % 16) - 1;
    if (q === col || q - col === dist || col - q === dist) return false;
    placed = Math.floor(placed / 16);
    dist += 1;
  }
  return true;
}

function solve(n, row, col, placed) {
  if (row === n) return 1;
  if (col === n) return 0;
  if (safe(placed, col, 1)) {
    return solve(n, row + 1, 0, placed * 16 + col + 1) + solve(n, row, col + 1, placed);
  }
  return solve(n, row, col + 1, placed);
}

if (solve(13, 0, 0, 0) !== 73712) process.exit(1);
