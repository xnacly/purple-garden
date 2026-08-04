#[cfg(not(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden currently supports only Linux or macOS on x86_64 or aarch64");

use std::collections::HashMap;

use purple_garden_bc::{self as bc, CcCallTarget};
use purple_garden_frontend::{
    diagnostic::{Diagnostic, Span},
    lex, lower, parser,
};
use purple_garden_runtime::{Anomaly, BuiltinFn, Value};
pub use purple_garden_shared::config;
use purple_garden_shared::trace;
use purple_garden_std::PgType;
use purple_garden_typecheck::{FunctionType, Typechecker};
mod rust_to_pg;
pub use rust_to_pg::CallArgs;

pub use purple_garden_macros::{GardenOpaque, GardenValue, pg_fn, pg_pkg};

/// Types and traits used when embedding Rust values and packages.
///
/// Most applications only need [`Pg`], the derive macros, and `#[pg_pkg]`.
/// Import this module when implementing an unsafe package function or manual
/// value conversion.
pub mod embed {
    pub use purple_garden_runtime::{
        Anomaly, Field, Fn, FromVm, IntoVm, PgType, Pkg, RecordFields, Type, Value, Vm, VmConfig,
    };

    #[doc(hidden)]
    pub use purple_garden_runtime::{
        alloc_record, copy_record, decode_record_field, encode_record_field,
    };
}

use embed::{FromVm, Pkg, Vm, VmConfig};

type JitFn = purple_garden_jit::JitFn;

/// Configures and compiles a Purple Garden program.
///
/// Start with [`Pg::new`], add the packages the source is allowed to import,
/// and finish with [`Pg::compile`]. The resulting [`Program`] owns its VM and
/// can be run repeatedly.
///
/// # Examples
///
/// ```
/// use purple_garden::Pg;
///
/// let mut program = Pg::new().compile(br#"40 + 2"#)?;
/// assert_eq!(program.run_take::<i64>()?, 42);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct Pg<'pg> {
    config: config::Config,
    libs: Vec<&'pg Pkg>,
    stdlib: bool,
    unsafe_stdlib: bool,
}

impl<'pg> Pg<'pg> {
    /// Creates a compiler configuration with no standard-library packages.
    ///
    /// Use [`Pg::with_stdlib`] to enable the safe standard library, and add
    /// application packages with [`Pg::with_lib`].
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let program = Pg::new().compile(br#"1"#)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self {
            config: config::Config::default(),
            libs: Vec::new(),
            stdlib: false,
            unsafe_stdlib: false,
        }
    }

    /// Uses `config` for optimisation, JIT, garbage-collection, and
    /// backtrace settings.
    ///
    /// # Examples
    ///
    /// Disable JIT compilation when deterministic bytecode-only execution is
    /// useful for debugging or benchmarking:
    ///
    /// ```
    /// use purple_garden::{Pg, config};
    ///
    /// let mut config = config::Config::default();
    /// config.no_jit = true;
    /// let program = Pg::new().config(config).compile(br#"1"#)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn config(mut self, config: config::Config) -> Self {
        self.config = config;
        self
    }

    /// Enables the safe standard-library packages for this compilation.
    ///
    /// This makes packages such as `math`, `strings`, `io`, and `testing`
    /// available to `import` statements in the source.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new()
    ///     .with_stdlib()
    ///     .compile(br#"import "math"
    /// math.abs(-42)"#)?;
    /// assert_eq!(program.run_take::<i64>()?, 42);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn with_stdlib(mut self) -> Self {
        self.stdlib = true;
        self
    }

    /// Enables the unsafe standard-library packages in addition to the safe
    /// standard library.
    ///
    /// This is required for source that imports `unsafe` packages, which can
    /// expose runtime allocation data and platform syscalls. Only enable it
    /// for scripts you trust.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let program = Pg::new()
    ///     .with_stdlib()
    ///     .with_unsafe_stdlib()
    ///     .compile(br#"import "unsafe/runtime"
    /// runtime.used()"#)?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn with_unsafe_stdlib(mut self) -> Self {
        self.unsafe_stdlib = true;
        self
    }

    /// Registers an embedded Rust package for this compilation.
    ///
    /// The package can then be imported by its package name in Garden source.
    /// Use [`pg_pkg`] to create package metadata from an ordinary Rust module.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::{Pg, pg_pkg};
    ///
    /// #[pg_pkg]
    /// mod numbers {
    ///     pub fn twice(value: i64) -> i64 {
    ///         value * 2
    ///     }
    /// }
    ///
    /// let mut program = Pg::new()
    ///     .with_lib(&numbers::PACKAGE)
    ///     .compile(br#"import "numbers"
    /// numbers.twice(21)"#)?;
    /// assert_eq!(program.run_take::<i64>()?, 42);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn with_lib(mut self, lib: &'pg Pkg) -> Self {
        self.libs.push(lib);
        self
    }

    /// Parses, type-checks, optimises, and compiles `input` into a program.
    ///
    /// The first parser, type-checker, lowering, or backend diagnostic is
    /// returned as an error. Package availability is fixed by the builder
    /// configuration used to create this [`Pg`].
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"6 * 7"#)?;
    /// assert_eq!(program.run_take::<i64>()?, 42);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn compile(&self, input: &'pg [u8]) -> Result<Program<'pg>, Diagnostic> {
        compile(
            &self.config,
            input,
            &self.libs,
            self.stdlib,
            self.unsafe_stdlib,
        )
    }
}

impl Default for Pg<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// A compiled Purple Garden program.
///
/// A program owns the VM, its bytecode, globals, native JIT pages, and
/// embedded syscall table. Reusing one `Program` preserves its VM allocations
/// while every [`Program::run`] starts a fresh top-level invocation.
///
/// # Examples
///
/// ```
/// use purple_garden::Pg;
///
/// let mut program = Pg::new().compile(br#"40 + 2"#)?;
/// assert_eq!(program.run_take::<i64>()?, 42);
/// assert_eq!(program.run_take::<i64>()?, 42);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug)]
pub struct Program<'p> {
    vm: Vm,
    entry: usize,
    entry_native: Option<BuiltinFn>,
    syscalls: Vec<BuiltinFn>,
    jit: Vec<JitFn>,
    funcs: HashMap<&'p str, (CcCallTarget, FunctionType<'p>)>,
}

/// A handle to a purple garden function extracted from [`Program`] via [`Program::function`],
/// invokable using [`Program::call`] or [`Program::call_unchecked`]
#[derive(Debug)]
pub struct Function<'f> {
    handle: CcCallTarget,
    signature: FunctionType<'f>,
}

impl<'p> Program<'p> {
    fn from_vm(vm: Vm, syscalls: Vec<BuiltinFn>) -> Self {
        let entry = vm.pc;
        Self {
            vm,
            entry,
            entry_native: None,
            syscalls,
            jit: Vec::new(),
            funcs: HashMap::new(),
        }
    }

    fn with_entry_native(mut self, entry_native: Option<BuiltinFn>) -> Self {
        self.entry_native = entry_native;
        self
    }

    /// Executes the top-level script and ignores its return register.
    ///
    /// Call this for scripts whose observable behavior is side effects, such
    /// as logging through an embedded package. Use [`Program::run_take`] when
    /// the final expression is a value needed by Rust.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::{Pg, config};
    ///
    /// let mut config = config::Config::default();
    /// config.no_jit = true;
    /// let mut program = Pg::new().config(config).compile(br#"let answer = 42"#)?;
    /// program.run()?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn run(&mut self) -> Result<(), Anomaly> {
        self.vm.reset();
        if let Some(entry) = self.entry_native {
            unsafe { entry((&mut self.vm as *mut Vm).cast()) };
            if let Some(anomaly) = self.vm.take_trap() {
                return Err(anomaly);
            }
            return Ok(());
        }
        self.vm.pc = self.entry;
        if self.vm.config.backtrace {
            self.vm.run::<true>(&self.syscalls)
        } else {
            self.vm.run::<false>(&self.syscalls)
        }
    }

    /// Runs the program and decodes the entry return value
    ///
    /// Top-level scripts return their final value-producing expression. If the
    /// script has no final value, use [`Program::run`] instead. Borrowed
    /// return values, such as `&str` and opaque handles, remain valid until
    /// the next mutable use of this program.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#""violet""#)?;
    /// let value: &str = program.run_take()?;
    /// assert_eq!(value, "violet");
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn run_take<'pg, T: FromVm<'pg>>(&'pg mut self) -> Result<T, Anomaly> {
        self.run()?;
        Ok(T::from_vm(&self.vm, *self.vm.r(0)))
    }

    /// Looks up a named Garden function for later invocation with
    /// [`Program::call`].
    ///
    /// The name is the function declaration name. Returns `None` when the compiled source does not
    /// define that function.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new()
    ///     .compile(br#"fn identity(value: Int) Int { value }"#)?;
    /// let identity = program.function("identity").expect("function exists");
    /// assert_eq!(program.call::<_, i64>(&identity, (42i64,))?, 42);
    ///
    /// assert!(program.function("not_declared").is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// A handle stays valid for the lifetime of the program and can be reused across calls:
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"fn double(value: Int) Int { value * 2 }"#)?;
    /// let double = program.function("double").expect("function exists");
    /// for i in 0..4i64 {
    ///     assert_eq!(program.call::<_, i64>(&double, (i,))?, i * 2);
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn function(&self, name: &str) -> Option<Function<'p>> {
        self.funcs.get(name).cloned().map(|(handle, signature)| {
            let f = Function { handle, signature };
            trace!("extracted `{}` handle: {:?}", name, f);
            return f;
        })
    }

    /// Invokes a function from a handle returned by [`Program::function`] with already encoded
    /// arguments and returns its raw return register.
    ///
    /// Does NOT check the rust types matching the purple garden types, use [`Program::call`] for
    /// the checked variant.
    ///
    /// # Safety
    ///
    /// - The caller guarantees that `args` holds exactly the number of values the function declares
    /// - Each already encoded as the matching Garden type,
    /// - The returned [`Value`] is decoded as the declared return type
    ///
    /// Violating this reinterprets raw VM words, so a mismatch on a
    /// pointer-backed type such as `Str` dereferences an arbitrary address.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::{Pg, embed::Value};
    ///
    /// let mut program = Pg::new().compile(br#"fn add(a: Int b: Int) Int { a + b }"#)?;
    /// let add = program.function("add").expect("function exists");
    ///
    /// let args: [Value; 2] = [40i64.into(), 2i64.into()];
    /// let ret = unsafe { program.call_unchecked(&add, &args)? };
    /// assert_eq!(ret.as_int(), 42);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Skipping the type check is only sound while the encoding matches the declaration, decoding
    /// the same `Int` as a `Double` silently reads the word as an IEEE 754 bit pattern:
    ///
    /// ```
    /// use purple_garden::{Pg, embed::Value};
    ///
    /// let mut program = Pg::new().compile(br#"fn identity(value: Int) Int { value }"#)?;
    /// let identity = program.function("identity").expect("function exists");
    ///
    /// let ret = unsafe { program.call_unchecked(&identity, &[42i64.into()])? };
    /// assert_eq!(ret.as_int(), 42);
    /// assert_ne!(ret.as_f64(), 42.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub unsafe fn call_unchecked<'vm>(
        &'vm mut self,
        function: &Function,
        args: &[Value],
    ) -> Result<Value, Anomaly> {
        self.vm.reset();

        for (i, a) in args.iter().enumerate() {
            *self.vm.r_mut(i) = *a;
        }

        match function.handle {
            CcCallTarget::Bc { pc } => {
                self.vm.pc = pc;
                self.vm.run(&self.syscalls)?;
            }
            CcCallTarget::Native { idx } => {
                unsafe { self.syscalls[idx as usize]((&mut self.vm as *mut Vm).cast()) };
            }
        };

        Ok(*self.vm.r(0))
    }

    /// Invokes a function from a handle returned by [`Program::function`] with arguments and
    /// decodes its return value into a Rust type.
    ///
    /// Arguments are passed through [`IntoVm`] and the result through [`FromVm`]. The function must
    /// accept the supplied number and type of arguments.
    ///
    /// Arguments are a tuple, a single argument is therefore `(T,)`. Argument count, argument
    /// types and the return type are checked against the declared signature before the call
    /// happens, a mismatch produces Err([`Anomaly`])
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"fn add(a: Int b: Int) Int { a + b }"#)?;
    /// let add = program.function("add").expect("function exists");
    /// assert_eq!(program.call::<_, i64>(&add, (40i64, 2i64))?, 42);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// A function without a return type is called with `()`:
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new()
    ///     .with_stdlib()
    ///     .compile(br#"
    /// import "io"
    /// fn log(value: Int) { io.println(value) }
    /// "#)?;
    /// let log = program.function("log").expect("function exists");
    /// program.call::<_, ()>(&log, (42i64,))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Every part of the signature is checked:
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"fn add(a: Int b: Int) Int { a + b }"#)?;
    /// let add = program.function("add").expect("function exists");
    ///
    /// assert!(program.call::<_, f64>(&add, (40i64, 2i64)).is_err());
    /// assert!(program.call::<_, i64>(&add, (40i64,)).is_err());
    /// assert!(program.call::<_, i64>(&add, (40i64, 2.0f64)).is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn call<'vm, Args, Ret>(
        &'vm mut self,
        function: &Function,
        args: Args,
    ) -> Result<Ret, Anomaly>
    where
        Args: CallArgs,
        Ret: PgType + FromVm<'vm>,
    {
        let signature = &function.signature;
        let pc_or_fallback = match function.handle {
            CcCallTarget::Bc { pc } => pc,
            CcCallTarget::Native { .. } => 0,
        };

        if Ret::TYPE != signature.ret {
            return Err(Anomaly::Msg {
                msg: "Return type provided does not match the purple garden functions return type",
                pc: pc_or_fallback,
            });
        }

        let as_types = Args::TYPES;
        if as_types.len() != signature.args.len() {
            return Err(Anomaly::Msg {
                msg: "Provided argument count does not match the purple garden functions argument count",
                pc: pc_or_fallback,
            });
        }

        for (i, arg) in as_types.iter().enumerate() {
            let expected_type = &signature.args[i].1;
            if arg != expected_type {
                return Err(Anomaly::Msg {
                    msg: "Provided arguments type does not match the type of the purple garden functions argument",
                    // TODO: i need to improve the errors here
                    // msg: &format!("argument {i} is {expected_type}, not {arg}"),
                    pc: pc_or_fallback,
                });
            }
        }

        let args = args.inner(&mut self.vm);
        let ret = unsafe { self.call_unchecked(function, args.as_ref())? };
        let result = Ret::from_vm(&self.vm, ret);
        Ok(result)
    }
}

fn compile<'i>(
    config: &config::Config,
    input: &'i [u8],
    libs: &[&'i Pkg],
    stdlib: bool,
    unsafe_stdlib: bool,
) -> Result<Program<'i>, Diagnostic> {
    let parse = parser::Parser::new(lex::Lexer::new(input)).parse_collect();
    if let Some(diagnostic) = parse.diagnostics.into_iter().next() {
        return Err(diagnostic);
    }
    let ast = parse
        .ast
        .expect("parser returned no diagnostics and no AST");

    let stdlib = stdlib_packages(stdlib, unsafe_stdlib);

    let typecheck = Typechecker::new(&ast)
        .with_libs(libs.to_vec())
        .with_stdlib(stdlib)
        .check();
    if let Some(diagnostic) = typecheck.diagnostics.into_iter().next() {
        return Err(diagnostic);
    }

    let mut ir = lower::Lower::new()
        .with_libs(libs.to_vec())
        .with_stdlib(stdlib)
        .ir_from_types(&ast, typecheck.types)?;
    if config.opt >= 1 {
        purple_garden_opt::ir(&mut ir);
    }

    let mut cc = bc::Cc::new();
    let native_pages = cc
        .compile(config, &ir)
        .map_err(|msg| Diagnostic::new(msg, Span::new(0, 0)))?;
    if config.opt >= 1 {
        purple_garden_opt::bc(&mut cc.buf);
        cc.compact_nops();
    }

    let funcs: HashMap<_, _> = cc
        .functions
        .values()
        .filter_map(|f| {
            let (name, ft) = typecheck.functions.get_key_value(f.name())?;
            Some((*name, (CcCallTarget::from(f), ft.clone())))
        })
        .collect();

    let (vm, syscalls, _debug, entry_native_idx) = cc.finalize(VmConfig {
        backtrace: config.backtrace,
        no_gc: config.no_gc,
        stack_size: config.stack_size,
    });
    let entry_native = entry_native_idx.map(|idx| syscalls[idx as usize]);
    let mut program = Program::from_vm(vm, syscalls).with_entry_native(entry_native);
    program.funcs = funcs;
    if !config.no_jit {
        program.jit = native_pages;
    }
    Ok(program)
}

fn stdlib_packages(enabled: bool, unsafe_enabled: bool) -> &'static [Pkg] {
    if !enabled {
        return &[];
    }

    if unsafe_enabled {
        purple_garden_std::STD
    } else {
        purple_garden_std::SAFE_STD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_can_run_bytecode_twice() {
        let mut config = config::Config::default();
        config.no_jit = true;
        let mut program = Pg::new()
            .config(config)
            .compile(b"42")
            .expect("program should compile");

        assert_eq!(program.run_take::<i64>().unwrap(), 42);
        assert_eq!(program.run_take::<i64>().unwrap(), 42);
    }

    #[test]
    fn compile_collects_signatures_for_bytecode_and_native_targets() {
        for (no_jit, want_native) in [(true, false), (false, true)] {
            let mut config = config::Config::default();
            config.no_jit = no_jit;
            let program = Pg::new()
                .config(config)
                .compile(b"fn used(value:Int) Int { value }\nfn unused(a:Str) Str { a }\nused(1)")
                .expect("program should compile");

            let (target, signature) = program.funcs.get("used").expect("`used` is collected");
            assert_eq!(
                matches!(target, CcCallTarget::Native { .. }),
                want_native,
                "no_jit={no_jit}"
            );
            assert_eq!(signature.args, vec![("value", embed::Type::Int)]);
            assert_eq!(signature.ret, embed::Type::Int);
            assert!(program.funcs.contains_key("unused"));
            assert!(!program.funcs.contains_key("entry"));
        }
    }
}
