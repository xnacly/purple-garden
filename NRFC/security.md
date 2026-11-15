# Security

## Threat model

The attacker controls a `.garden` source file: it is passed to the CLI, or an
editor running `purple-garden lsp` opens it. Nothing else is untrusted. The
embedding API is trusted code, but must not let safe Rust cause undefined
behaviour.

| goal of the attacker                         | severity |
| -------------------------------------------- | -------- |
| memory corruption, native code execution     | critical |
| reading memory outside the program's values  | high     |
| crashing or hanging the compiler or the LSP  | medium   |
| a panic that turns into a clean diagnostic   | low      |

Out of scope: the `unsafe/*` std packages. They are trusted only and the CLI
enables them with `--unsafe`, the library with `Pg::with_unsafe_stdlib`.

The surface that matters is the about 540 `unsafe` occurrences and the code
the JIT emits. The language itself has no file, process or network access, so
memory safety comes down to two questions: does every program the typechecker
accepts keep the invariants the VM and the JIT rely on, and does every stage
agree on what a program means.

## Invariants the VM and the JIT rely on

These are checked at compile time, not at runtime, so each one is a security
boundary:

- Register operands are 5 bits wide. Arguments travel in `r0..r{n-1}` and the
  bytecode compiler's temporaries sit above them, so a function takes at most
  `MAX_ARGS = 30` arguments. The typechecker enforces this for `fn` and
  `extern`, and bc asserts it.
- `Push`/`Pop` are balanced and spill slots are below the frame depth. bc and
  the register allocator guarantee this, and the VM only checks it with
  `debug_assert`. Differential fuzzing runs debug builds for this reason.
- Every count and index narrowed into a bytecode field goes through `Cc::fit`,
  which reports a diagnostic instead of wrapping: jump targets (16-bit
  absolute pc), call targets (26 bits), spill slots (u16), array literals
  (8190 words), record field offsets, native, switch, lookup and global
  indices.
- The parser rejects syntax nested deeper than `MAX_NESTING = 256`, and the
  parser and typechecker reject types of more than `MAX_TYPE_PARTS = 256`
  parts. Every recursive pass relies on the first and record layout on the
  second.
- Every IR value is used only where its definition is visible: in its own block
  or passed as a block parameter. `Func::verify` checks this after lowering and
  after every optimisation pass in debug builds; the VM and the JIT read
  undefined registers otherwise.
- Int arithmetic wraps in every stage (VM, constant folding, JIT), including
  `i64::MIN / -1`; division and modulo by zero trap and are never removed as
  dead code.
- Array indexing goes through an IR bounds check that no optimisation pass may
  remove unless the index is proven in range.
- `Value` reinterpretations (`as_str`, record and array access) rely on the
  typechecker routing only matching types to them.
- JIT memory is never writable and executable at once: `CodeArena` appends with
  `RW`, then seals with a single `mprotect` to `R+X`.
- The GC does not free anything yet (`collect` is a no-op). Once it does, it has
  to treat as roots: values in JIT registers and on the native stack across
  `jit_alloc`, spill slots, interior pointers from `AddrOf` and inline
  records, `StrTable` entries, and `&str` borrowed by builtins across an
  allocation.

## Tooling

| tool                            | command                                                           |
| ------------------------------- | ----------------------------------------------------------------- |
| Miri                            | `cargo miri test -p <crate>`; not the JIT, inline asm or CLI tests. Example programs run end to end only partly (each takes minutes). The GC turns integers into pointers (`gc.rs`), so Miri may miss bugs through GC pointers; `gc_stress` covers those |
| AddressSanitizer                | `RUSTFLAGS=-Zsanitizer=address cargo test --target <host>`        |
| GC stress (collect before every allocation, every object fenced by a guard page) | `cargo test --workspace --features purple-garden-runtime/gc_stress` |
| limits and DoS regressions      | `cargo test -p purple-garden --test limits --release`             |
| VM/JIT differential fuzzing    | see `fuzz/README.md`: `parse`, `compile`, `diff` targets and the seeded `purple-garden-fuzz` runner |
| supply chain                    | `cargo audit` and `cargo deny check` (policy in `deny.toml`)      |
| JIT on Apple Silicon            | `cargo test --target x86_64-apple-darwin` (Rosetta, no AVX)       |

The aarch64 backend compiles nothing yet, so every JIT test needs an x86-64
target.

## Findings

| # | severity | finding | fix |
| - | -------- | ------- | --- |
| 1 | critical | Functions with more than 30 parameters: register numbers past `r31` spilled into the neighbouring 5-bit operand, so a `Str` parameter could receive an `Int` argument as its pointer (`str.len` dereferenced `0x41414162`) | `c1902bf` |
| 2 | critical | A record value nested into a record or array literal stored its pointer instead of its fields, so reading a `Str` field dereferenced unwritten memory | `9bafd19` |
| 3 | critical | More than 65535 values live at once wrapped the u16 spill slot counter in release builds, so two live values shared one slot | `4b2f367` |
| 4 | critical | Lowering used values outside the block that defines them: code after a top-level `match`, `&` or `\|`, operands computed before a `&`/`\|` in a later operand, `&`/`\|` in a match condition. Plain scripts read and wrote arbitrary memory (segfaults in the JIT, null dereferences in the VM). `Func::verify` now checks the IR after lowering and every pass in debug builds | `202dd3a` |
| 5 | critical | The bytecode peephole `mov_merge` deleted register writes that were still live, `-O1` returned wrong results | `731e114` |
| 6 | high | The string constant pool was a `Box<[u32]>` that values pointed into; moving the box invalidated them (Miri) | `71b53a1` |
| 7 | high | Nesting a few hundred (debug) to a few thousand (release) levels overflowed the native stack and aborted the CLI and the LSP | `80f3f61` |
| 8 | high | Types grew exponentially through `let` chains | `cb145dc` |
| 9 | high | The LSP crashed on any non-ASCII character outside a string (`let t = é`) | `4c719ed` |
| 10 | high | The LSP needed 1.27 GiB and 8 s per keystroke for a 100 KB document | `78ffc88` |
| 11 | high | Constant folding built strings without bound: 31 lines of doubling concatenation allocated 4.9 GB at compile time | `b089a0e` |
| 12 | high | Int overflow panicked in the VM, `i64::MIN / -1` panicked in constant folding and raised `SIGFPE` in the JIT. Int arithmetic now wraps in every stage | `9bc1397` |
| 13 | high | The x86 JIT classified a block parameter only from forward edges, so a Double reaching it through a back edge got an integer register and crashed the compiler | `77cb0ef` |
| 14 | medium | Bytecode encoding limits panicked or wrapped (jump targets, array literals, table and constant indices) | `4b2f367` |
| 15 | medium | Error recovery emitted one diagnostic per token, each rendering its whole line | `78ffc88` |
| 16 | medium | Register allocation was quadratic in the number of live values (70k: over 5 minutes) | `7d3390d` |
| 17 | medium | String `match` tables were indexed by pool address without a density check (1 MB of source, 422 MB of tables) | `72b80be` |
| 18 | medium | `str.repeat` aborted on a negative count and allocated without bound on a huge one | `b089a0e` |
| 19 | medium | Embedding API: a `Function` handle outlived its `Program` (use after free) and could be called on another program | `200da7f` |
| 20 | medium | `switch_fold` left unreachable blocks that crashed the JIT; dce now removes blocks the entry cannot reach | `85dc22a` |
| 21 | medium | dce removed divisions whose quotient was unused, and with them the division by zero trap | `69d76b2` |
| 22 | medium | `math.abs` and `math.clamp` aborted the process (and the compiler, through constant folding) on edge inputs | `f2e3e80` |
| 23 | low | A generic call on an argument that already had an error left a free type slot (debug panic) | `aa6e9ca` |
| 24 | low | Advisories in dev and bench dependencies, no dependency policy | `2ef9de1` |
| 25 | low | The CLI mapped the source file. Rust's borrow of the mapping does not stop other processes: anyone with write access can change or truncate the file while it compiles. On Linux the change shows through the private mapping, so the "immutable" `&[u8]` changes under the lexer (undefined behaviour, e.g. `from_utf8_unchecked` over bytes no longer ASCII), and truncation raises `SIGBUS`. On macOS a private mapping behaves like a snapshot and neither happens (tested). Low, because whoever can write the file already chooses the program; it matters for a check-then-run setup. FIFOs (`<(...)`) also reported size 0 and ran as empty programs | `60fcae0` |
| 26 | low | The CLI enabled the `unsafe/` std namespace by default | `a50b2f8` |
| 27 | low | The VM checks bytecode invariants (balanced push/pop, spill slots) only with `debug_assert` | open, covered by debug-build fuzzing |
| 28 | low | GC rooting hazards are latent until `collect` frees memory | open, see the invariants above |
| 29 | low | Building a string over 4 GiB or a failed GC allocation panics at runtime instead of trapping; needs fallible allocation in the runtime | open |
| 30 | low | Malformed JSON-RPC framing ends the LSP (cleanly); file contents cannot cause it | accepted |

Jumps that land past the first 64Ki ops of a program are a compile error.
Pc-relative jumps would lift that limit.
