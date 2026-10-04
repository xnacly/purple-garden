// ack(3, 9) = 4093. Inherently recursive, ~4k frames deep; JSC's default
// stack handles that comfortably.
function ack(m, n) {
  if (m === 0) return n + 1;
  if (n === 0) return ack(m - 1, 1);
  return ack(m - 1, ack(m, n - 1));
}
if (ack(3, 9) !== 4093) process.exit(1);
