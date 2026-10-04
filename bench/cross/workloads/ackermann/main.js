// ack(3, 8) = 2045. Inherently recursive, ~2k frames deep; V8's default
// stack handles that comfortably.
function ack(m, n) {
  if (m === 0) return n + 1;
  if (n === 0) return ack(m - 1, 1);
  return ack(m - 1, ack(m, n - 1));
}
if (ack(3, 8) !== 2045) process.exit(1);
