# Standard library process

- [x] io
  - [x] println
  - [x] print
- [ ] io/fs
  - [ ] read
  - [ ] write
  - [ ] create
- [ ] runtime/gc
  - [ ] cycle
  - [ ] stop
  - [ ] stats
- [ ] string
  - [x] contains
  - [x] idx
  - [ ] slice
  - [ ] lines
  - [ ] find
  - [x] repeat
  - [ ] split
  - [ ] lower
  - [ ] upper
  - [ ] trim
- [ ] arr (blocked by generics)
  - [ ] range
  - [ ] join
  - [ ] sum
  - [ ] flat
- [ ] opt (blocked by generics)
  - [ ] some
  - [ ] none
  - [ ] is_some
  - [ ] is_none
  - [ ] unwrap
  - [ ] or
- [ ] opt/cmp (blocked by generics)
  - [ ] and
  - [ ] or
  - [ ] either
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
- [ ] cmd (blocked by generics)
    - [ ] run
    - [ ] run_with
