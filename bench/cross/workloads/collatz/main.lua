-- Sum of Collatz step counts for 1..30000 = 2864311. Integer /, %, * mix.
-- Loop form, deliberately not tail-recursive like the garden source: LuaJIT's
-- trace compiler handles `while` loops well but recursion poorly even through
-- proper tail calls — the tail-recursive port ran ~4.6x slower with ~8% run-to-
-- run scatter versus 0.2% here. garden's tailcall pass compiles its recursion
-- to the same loop, so the machine-level work matches. is_even inlined as the
-- idiomatic n % 2. n / 2 is exact for even n in doubles; the largest value
-- reached is ~106M, well inside 2^53.
local function collatz_steps(n)
  local acc = 0
  while n ~= 1 do
    if n % 2 == 0 then n = n / 2 else n = 3 * n + 1 end
    acc = acc + 1
  end
  return acc
end
local s = 0
for i = 1, 30000 do s = s + collatz_steps(i) end
if s ~= 2864311 then os.exit(1) end
