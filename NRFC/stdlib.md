# Standard library process

- [x] io
  - [x] println
  - [x] print
- [ ] io/fs
  - [ ] read
  - [ ] write
  - [ ] create
  - [ ] mkdir
- [ ] unsafe/runtime
  - [ ] cycles
  - [x] allocs
  - [x] used
- [ ] str
  - [x] contains
  - [x] get
  - [x] slice
  - [ ] lines
  - [ ] find
  - [x] repeat
  - [ ] split
  - [ ] lower
  - [ ] upper
  - [ ] trim
  - [ ] from(Array<Byte>)
  - [x] from(Int)
  - [x] from(Double)
- [ ] map(K,V)
  - [ ] has(K) -> bool
  - [ ] put(K,V)
  - [ ] get(K) -> Option<V>
  - [ ] len
- [ ] arr(T)
  - [ ] range
  - [ ] join
  - [ ] sum
  - [ ] flat
  - [ ] get(idx) -> Option<T>
- [ ] opt(T)
  - [ ] some(T) -> Option<T>
  - [ ] none() -> Option<()>
  - [ ] is_some(Option<T>) -> bool
  - [ ] is_none(Option<T>) -> bool
  - [ ] unwrap(Option<T>) -> T
  - [ ] or(Option<T>, T) -> T
- [ ] cmp
  - [ ] min(T,T) -> T
  - [ ] max(T,T) -> T

    ```rust
      #[pg_pkg]
      pub mod cmp {
          /// Some(value) when cond is true, None otherwise.
          #[pg_fn(pure)]
          pub fn and<T>(cond: bool, value: T) -> Option<T>

          /// None when cond is true, Some(value) otherwise.
          #[pg_fn(pure)]
          pub fn or<T>(cond: bool, value: T) -> Option<T>

          /// t when cond is true, f otherwise.
          #[pg_fn(pure)]
          pub fn either<T>(cond: bool, t: T, f: T) -> T
      }
    ```
- [ ] cmd
    - [ ] run(Array<Str>)
    - [ ] run_with(Record<cmd:Str args:Str env:Record<key:Str val:Str> ...>)
