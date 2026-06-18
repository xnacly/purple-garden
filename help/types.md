# Types

Purple Garden has five primitive types:

- `Void`
- `Bool`
- `Int`
- `Double`
- `Str`

It also has non-primitive type forms:

- `Option<T>`
- `Array<T>`
- `Foreign<identifier>`

## Examples

```garden
let attempts = 3
let scale = 0.5
let ready = true
let label = "garden"

fn print_once(message:Str) Void {
    io.println(message)
}
```

## Binary Operators

Both sides of a binary operator must have the same type. There is no implicit promotion; cast explicitly with `as` if you need to mix types.

```text
+  -  *  /              Int      -> Int
                        Double   -> Double

==  !=                  Int      -> Bool
                        Bool     -> Bool

<  >                    Int      -> Bool
                        Double   -> Bool
```

Equality (`==` / `!=`) is not defined on `Double` on purpose: comparing floats for exact equality is almost always a bug. Use `<` / `>` to check ordering, or wrap the comparison yourself when you really mean it.

## Casts

The `as` keyword converts between primitive types. Only these directions are permitted:

```text
Int    as Double        widens to f64
Double as Int           truncates toward zero
Int    as Bool          0 becomes false, anything else true
Bool   as Int           false becomes 0, true becomes 1
```

`Bool` and `Double` cannot be cast in either direction. `Bool` to `Double` has no useful purpose; write `1.0` or `0.0` directly. `Double` to `Bool` has no clean semantics because values such as `-0.0` and `NaN` make truthiness ambiguous. Cast through `Int` if you really need the conversion.

```garden
let pixel = 42
let x = (pixel as Double) / 100.0 - 1.5
```

