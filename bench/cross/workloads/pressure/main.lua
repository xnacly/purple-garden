-- 32 ints and 16 doubles live per call to `mix`, in the op order of main.garden. Integer
-- division and modulo truncate like Rust; every value stays far below 2^53, so doubles hold
-- them exactly. Loop form.
local floor, ceil, abs, sqrt = math.floor, math.ceil, math.abs, math.sqrt
local function trunc(x) if x >= 0 then return floor(x) else return ceil(x) end end
local function div(a, b) return trunc(a / b) end
local function mod(a, b) return a - b * trunc(a / b) end
local function weight(n)
  if n == 0 then return 3 elseif n == 1 then return 5 elseif n == 2 then return 7
  elseif n == 3 then return 11 else return 13 end
end
local function mix(i)
  local a1 = i * 3 + 1
  local a2 = a1 - (7 * i)
  local a3 = div(a1, 7)
  local a4 = div(a2, 3)
  local a5 = mod(a2, 5)
  local a6 = div(a1, 4)
  local a7 = div(a2, 8)
  local a8 = mod(a2, 16)
  local a9 = a1 * a2
  local a10 = div(a9, a3 + 1)
  local a11 = mod(a9, a6 + 2)
  local a12 = weight(mod(a1, 5))
  local a13 = abs(a2)
  local a14 = a13 - a12
  local a15 = a14 * 3
  local a16 = div(a15, 5)
  local a17 = mod(a16, 9)
  local a18 = a17 + a1
  local a19 = a18 * a5
  local a20 = a19 - a4
  local a21 = div(a20, a12 + 0)
  local a22 = a21 + a7
  local a23 = a22 * 2
  local a24 = a23 - a8
  local a25 = mod(a24, 11)
  local a26 = a25 + a10
  local a27 = a26 - a11
  local a28 = a27 * 5
  local a29 = div(a28, 6)
  local a30 = a29 + a3
  local a31 = a30 - a6
  local a32 = a31 + a2
  local d1 = a1 * 0.5
  local d2 = a2 / 3.0
  local d3 = d1 - d2
  local d4 = d1 * d2
  local d5 = sqrt(d1 + 1.0)
  local d6 = d3 / (d5 + 1.0)
  local d7 = d4 + d6
  local d8 = a5 - d7
  local d9 = d8 * 0.25
  local d10 = d9 + d1
  local d11 = d10 - d5
  local d12 = d11 / 7.0
  local d13 = d12 + d3
  local d14 = d13 + a17
  local d15 = d14 - d2
  local d16 = d15 * 0.125
  local pa, pb = a10, d8
  local c1 = d3 > d4 and 1 or 2
  local c2 = d1 < d5 and 4 or 8
  local c3 = a2 < a3 and 16 or 32
  local ints = a1 + a2 + a3 + a4 + a5 + a6 + a7 + a8 + a9 + a10 + a11 + a12 + a13 + a14 + a15 + a16
    + a17 + a18 + a19 + a20 + a21 + a22 + a23 + a24 + a25 + a26 + a27 + a28 + a29 + a30 + a31 + a32
  local doubles = d1 + d2 + d3 + d4 + d5 + d6 + d7 + d8 + d9 + d10 + d11 + d12 + d13 + d14 + d15 + d16
  return mod(ints, 1000003) + mod(trunc(doubles), 1009) + mod(pa, 101) + mod(trunc(pb), 13) + c1 + c2 + c3
end
local acc = 0
for i = 0, 5000000 - 1 do acc = acc + mix(i) end
if acc ~= -2502755696872 then os.exit(1) end
