// The public API returns diagnostics by value so callers can handle them
// without an additional allocation at every compiler stage.
#![allow(clippy::result_large_err)]

#[cfg(not(all(
    any(target_os = "linux", target_os = "macos"),
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
compile_error!("purple-garden currently supports only Linux or macOS on x86_64 or aarch64");

use std::{collections::HashMap, marker::PhantomData};

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
    /// This makes packages such as `math`, `str`, `io`, and `testing`
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

/// A typed handle to a purple garden function extracted from [`Program`] via
/// [`Program::function`].
///
/// `Args` and `Ret` are the Rust types expected when invoking this function.
/// [`Program::call`] verifies that they match the Garden declaration before
/// executing it.
#[derive(Debug)]
pub struct Function<'f, Args, Ret> {
    handle: CcCallTarget,
    signature: FunctionType<'f>,
    types: PhantomData<fn(Args) -> Ret>,
}

/// Error returned by [`Program::call`].
///
/// Signature variants describe a mismatch between the Rust types selected on
/// [`Program::function`] and the Garden function declaration. [`CallError::Runtime`]
/// contains a trap raised while the function was executing.
#[derive(Debug)]
pub enum CallError {
    /// The requested Rust return type does not match the Garden return type.
    ReturnType { expected: String, actual: String },
    /// The requested Rust argument count does not match the Garden declaration.
    ArgumentCount { expected: usize, actual: usize },
    /// One requested Rust argument type does not match the Garden declaration.
    ArgumentType {
        index: usize,
        expected: String,
        actual: String,
    },
    /// Execution trapped in the Garden VM.
    Runtime(Anomaly),
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ReturnType { expected, actual } => {
                write!(f, "return type mismatch: expected {expected}, got {actual}")
            }
            Self::ArgumentCount { expected, actual } => {
                write!(
                    f,
                    "argument count mismatch: expected {expected}, got {actual}"
                )
            }
            Self::ArgumentType {
                index,
                expected,
                actual,
            } => write!(
                f,
                "argument type mismatch at index {index}: expected {expected}, got {actual}"
            ),
            Self::Runtime(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for CallError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Runtime(error) => Some(error),
            _ => None,
        }
    }
}

impl<Args, Ret> Function<'_, Args, Ret>
where
    Args: CallArgs,
    Ret: PgType,
{
    fn verify_signature(&self) -> Result<(), CallError> {
        if Ret::TYPE != self.signature.ret {
            return Err(CallError::ReturnType {
                expected: self.signature.ret.to_string(),
                actual: Ret::TYPE.to_string(),
            });
        }

        let as_types = Args::TYPES;
        if as_types.len() != self.signature.args.len() {
            return Err(CallError::ArgumentCount {
                expected: self.signature.args.len(),
                actual: as_types.len(),
            });
        }

        for (i, arg) in as_types.iter().enumerate() {
            let expected_type = &self.signature.args[i].1;
            if arg != expected_type {
                return Err(CallError::ArgumentType {
                    index: i,
                    expected: expected_type.to_string(),
                    actual: arg.to_string(),
                });
            }
        }

        Ok(())
    }
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
    /// let identity = program.function::<_, i64>("identity").expect("function exists");
    /// assert_eq!(program.call(&identity, (42i64,))?, 42);
    ///
    /// assert!(program.function::<(i64,), i64>("not_declared").is_none());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// A handle stays valid for the lifetime of the program and can be reused across calls:
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"fn double(value: Int) Int { value * 2 }"#)?;
    /// let double = program.function::<_, i64>("double").expect("function exists");
    /// for i in 0..4i64 {
    ///     assert_eq!(program.call(&double, (i,))?, i * 2);
    /// }
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn function<Args, Ret>(&self, name: &str) -> Option<Function<'p, Args, Ret>> {
        self.funcs.get(name).cloned().map(|(handle, signature)| {
            let f = Function {
                handle,
                signature,
                types: PhantomData,
            };
            trace!("extracted `{}` handle: {:?}", name, &f.handle);
            f
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
    /// let add = program
    ///     .function::<(i64, i64), i64>("add")
    ///     .expect("function exists");
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
    /// let identity = program
    ///     .function::<(i64,), i64>("identity")
    ///     .expect("function exists");
    ///
    /// let ret = unsafe { program.call_unchecked(&identity, &[42i64.into()])? };
    /// assert_eq!(ret.as_int(), 42);
    /// assert_ne!(ret.as_f64(), 42.0);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub unsafe fn call_unchecked<Args, Ret>(
        &mut self,
        function: &Function<'_, Args, Ret>,
        args: &[Value],
    ) -> Result<Value, Anomaly> {
        self.vm.reset();

        for (i, a) in args.iter().enumerate() {
            *self.vm.r_mut(i) = *a;
        }

        match function.handle {
            CcCallTarget::Bc { pc } => {
                self.vm.pc = pc;
                if self.vm.config.backtrace {
                    self.vm.run::<true>(&self.syscalls)?;
                } else {
                    self.vm.run::<false>(&self.syscalls)?;
                }
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
    /// happens, a mismatch produces Err([`CallError`]). The Rust signature is
    /// selected when retrieving the function with [`Program::function`], so it
    /// is inferred here.
    ///
    /// # Examples
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"fn add(a: Int b: Int) Int { a + b }"#)?;
    /// let add = program.function::<_, i64>("add").expect("function exists");
    /// assert_eq!(program.call(&add, (40i64, 2i64))?, 42);
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
    /// let log = program.function::<_, ()>("log").expect("function exists");
    /// program.call(&log, (42i64,))?;
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// Every part of the signature is checked:
    ///
    /// ```
    /// use purple_garden::Pg;
    ///
    /// let mut program = Pg::new().compile(br#"fn add(a: Int b: Int) Int { a + b }"#)?;
    /// let wrong_return = program.function::<_, f64>("add").expect("function exists");
    /// let wrong_arity = program.function::<(i64,), i64>("add").expect("function exists");
    /// let wrong_argument = program
    ///     .function::<(i64, f64), i64>("add")
    ///     .expect("function exists");
    ///
    /// assert!(program.call(&wrong_return, (40i64, 2i64)).is_err());
    /// assert!(program.call(&wrong_arity, (40i64,)).is_err());
    /// assert!(program.call(&wrong_argument, (40i64, 2.0f64)).is_err());
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn call<'vm, Args, Ret>(
        &'vm mut self,
        function: &Function<'_, Args, Ret>,
        args: Args,
    ) -> Result<Ret, CallError>
    where
        Args: CallArgs,
        Ret: PgType + FromVm<'vm>,
    {
        function.verify_signature()?;

        let args = args.inner(&mut self.vm);
        let ret =
            unsafe { self.call_unchecked(function, args.as_ref()) }.map_err(CallError::Runtime)?;
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

    #[test]
    fn call_error_reports_signature_mismatches_and_runtime_traps() {
        let mut config = config::Config::default();
        config.no_jit = true;
        let mut program = Pg::new()
            .config(config)
            .compile(b"fn divide(a: Int b: Int) Int { a / b }")
            .expect("program should compile");

        let wrong_return = program
            .function::<(i64, i64), f64>("divide")
            .expect("function exists");
        assert!(matches!(
            program.call(&wrong_return, (1i64, 1i64)),
            Err(CallError::ReturnType { expected, actual })
                if expected == "Int" && actual == "Double"
        ));

        let wrong_arity = program
            .function::<(i64,), i64>("divide")
            .expect("function exists");
        assert!(matches!(
            program.call(&wrong_arity, (1i64,)),
            Err(CallError::ArgumentCount {
                expected: 2,
                actual: 1
            })
        ));

        let wrong_argument = program
            .function::<(i64, f64), i64>("divide")
            .expect("function exists");
        assert!(matches!(
            program.call(&wrong_argument, (1i64, 1.0f64)),
            Err(CallError::ArgumentType {
                index: 1,
                expected,
                actual
            }) if expected == "Int" && actual == "Double"
        ));

        let divide = program
            .function::<_, i64>("divide")
            .expect("function exists");
        assert!(matches!(
            program.call(&divide, (1i64, 0i64)),
            Err(CallError::Runtime(Anomaly::DivisionByZero { .. }))
        ));
    }
}
