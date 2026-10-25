use purple_garden::embed::{FromVm, IntoVm, Value, Vm, VmConfig};
use purple_garden::{GardenOpaque, GardenValue, pg_pkg};

#[derive(GardenOpaque)]
struct Counter {
    value: i64,
}

#[derive(GardenValue)]
struct User {
    name: String,
    age: i64,
}

#[derive(GardenValue)]
struct Profile {
    name: String,
}

#[derive(GardenValue)]
struct Account {
    profile: Profile,
    active: bool,
}

#[pg_pkg]
mod str {
    /// Returns the length of s in bytes.
    #[purple_garden::pg_fn(pure)]
    pub fn len(s: &str) -> i64 {
        s.len() as i64
    }

    /// Repeats s n times.
    pub fn repeat(s: &str, n: i64) -> String {
        s.repeat(n as usize)
    }

    /// Writes nothing.
    pub fn noop() {}
}

#[pg_pkg]
mod counters {
    use purple_garden::embed::Vm;

    use super::Counter;

    pub fn new_counter(value: i64) -> Counter {
        Counter { value }
    }

    #[purple_garden::pg_fn(pure)]
    pub fn value(counter: &Counter) -> i64 {
        counter.value
    }

    #[purple_garden::pg_fn(unsafe)]
    pub fn add_register_zero(vm: &mut Vm, value: i64) -> i64 {
        // SAFETY: r0 exists
        unsafe { vm.r(0) }.as_int() + value
    }
}

#[pg_pkg]
mod users {
    use super::{Account, Profile, User};

    pub fn user_name(user: User) -> String {
        user.name
    }

    pub fn make_user(name: String, age: i64) -> User {
        User { name, age }
    }

    pub fn profile_name(account: Account) -> String {
        account.profile.name
    }

    pub fn make_account(name: String) -> Account {
        Account {
            profile: Profile { name },
            active: true,
        }
    }
}

#[pg_pkg]
mod t {
    use purple_garden::embed::Slot;

    /// lx.x
    #[purple_garden::pg_fn(with_slots)]
    pub fn id(x: Slot<"T">) -> Slot<"T"> {
        x
    }
}

#[pg_pkg]
mod native {
    use purple_garden::embed::Slot;

    pub fn add(a: i64, b: i64) -> i64 {
        a + b
    }

    pub fn scale(x: f64, by: i64, y: f64) -> f64 {
        x * by as f64 + y
    }

    pub fn greet(name: &str) -> String {
        format!("hi {name}")
    }

    #[purple_garden::pg_fn(with_slots)]
    pub fn id(x: Slot<"T">) -> Slot<"T"> {
        x
    }
}

#[pg_pkg]
mod tools {
    pub fn root() -> i64 {
        1
    }

    #[purple_garden::pg_pkg]
    pub mod math {
        pub fn double(n: i64) -> i64 {
            n * 2
        }
    }

    #[purple_garden::pg_pkg]
    pub mod r#unsafe {}
}

#[test]
fn pg_pkg_generates_package_metadata() {
    assert_eq!(str::PACKAGE.name, "str");
    assert_eq!(str::PACKAGE.fns.len(), 3);
    assert_eq!(str::PACKAGE.fns[0].name, "len");
    assert!(str::PACKAGE.fns[0].pure);
    assert_eq!(str::PACKAGE.fns[0].arg_names, &["s"]);
    assert!(!str::PACKAGE.fns[1].pure);
    assert_eq!(str::PACKAGE.fns[1].arg_names, &["s", "n"]);
}

#[test]
fn pg_pkg_generates_subpackage_metadata() {
    assert_eq!(tools::PACKAGE.name, "tools");
    assert_eq!(tools::PACKAGE.fns.len(), 1);
    assert_eq!(tools::PACKAGE.pkgs.len(), 2);
    assert_eq!(tools::PACKAGE.pkgs[0].name, "math");
    assert_eq!(tools::PACKAGE.pkgs[0].fns[0].name, "double");
    assert_eq!(tools::PACKAGE.pkgs[1].name, "unsafe");
}

#[test]
fn pg_pkg_wrapper_decodes_args_and_encodes_return() {
    let mut vm = Vm::new(VmConfig::default());
    let s = vm.new_string("hello".to_owned());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = s;

    // SAFETY: valid vm, r0 holds the Str len expects
    unsafe { (str::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists
    assert_eq!(unsafe { vm.r(0) }.as_int(), 5);
}

#[test]
fn pg_pkg_wrapper_allocates_return_strings() {
    let mut vm = Vm::new(VmConfig::default());
    let s = vm.new_string("ha".to_owned());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = s;
    // SAFETY: r1 exists
    *unsafe { vm.r_mut(1) } = Value::from(3_i64);

    // SAFETY: valid vm, r0 and r1 hold repeat's Str and Int
    unsafe { (str::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists and holds a Str
    assert_eq!(unsafe { vm.r(0).as_str() }, "hahaha");
}

#[test]
fn pg_pkg_supports_garden_opaque_types() {
    assert_eq!(
        counters::PACKAGE.fns[0].ret,
        purple_garden::embed::Type::Foreign("Counter")
    );
    assert_eq!(
        counters::PACKAGE.fns[1].args,
        &[purple_garden::embed::Type::Foreign("Counter")]
    );

    let mut vm = Vm::new(VmConfig::default());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = Value::from(7_i64);

    // SAFETY: valid vm, r0 holds new_counter's Int, then its Counter
    unsafe { (counters::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };
    unsafe { (counters::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists
    assert_eq!(unsafe { vm.r(0) }.as_int(), 7);
}

#[test]
fn garden_opaque_encodes_borrowed_handles() {
    let counter = Counter { value: 9 };
    let mut vm = Vm::new(VmConfig::default());
    let value = (&counter).into_vm(&mut vm);

    let decoded: &Counter = FromVm::from_vm(&vm, value);
    assert_eq!(decoded.value, 9);
}

#[test]
fn pg_fn_unsafe_passes_vm_and_exposes_remaining_signature() {
    let fun = &counters::PACKAGE.fns[2];
    assert_eq!(fun.name, "add_register_zero");
    assert_eq!(fun.arg_names, &["value"]);
    assert_eq!(fun.args, &[purple_garden::embed::Type::Int]);
    assert_eq!(fun.ret, purple_garden::embed::Type::Int);

    let mut vm = Vm::new(VmConfig::default());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = Value::from(40_i64);

    // SAFETY: valid vm, r0 holds add_register_zero's Int
    unsafe { (fun.ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists
    assert_eq!(unsafe { vm.r(0) }.as_int(), 80);
}

#[test]
fn pg_pkg_exposes_record_metadata() {
    assert_eq!(
        users::PACKAGE.fns[0].args,
        &[purple_garden::embed::Type::Record(&[
            purple_garden::embed::Field {
                name: "name",
                ty: purple_garden::embed::Type::Str,
            },
            purple_garden::embed::Field {
                name: "age",
                ty: purple_garden::embed::Type::Int,
            },
        ])]
    );
    assert!(
        users::PACKAGE
            .extern_source()
            .contains("fn user_name(user: Record<name: Str age: Int>) Str")
    );
}

#[test]
fn pg_pkg_wrapper_decodes_record_arg() {
    let mut vm = Vm::new(VmConfig::default());
    let record = User {
        name: "teo".to_owned(),
        age: 42,
    }
    .into_vm(&mut vm);
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = record;

    // SAFETY: valid vm, r0 holds a User record
    unsafe { (users::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists and holds a Str
    assert_eq!(unsafe { vm.r(0).as_str() }, "teo");
}

#[test]
fn pg_pkg_wrapper_encodes_record_return() {
    let mut vm = Vm::new(VmConfig::default());
    let name = vm.new_string("ada".to_owned());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = name;
    // SAFETY: r1 exists
    *unsafe { vm.r_mut(1) } = Value::from(37_i64);

    // SAFETY: valid vm, r0 and r1 hold make_user's Str and Int
    unsafe { (users::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists
    let user = User::from_vm(&vm, *unsafe { vm.r(0) });
    assert_eq!(user.name, "ada");
    assert_eq!(user.age, 37);
}

#[test]
fn pg_pkg_supports_nested_records() {
    let mut vm = Vm::new(VmConfig::default());
    let account = Account {
        profile: Profile {
            name: "grace".to_owned(),
        },
        active: true,
    }
    .into_vm(&mut vm);
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = account;

    // SAFETY: valid vm, r0 holds an Account record
    unsafe { (users::PACKAGE.fns[2].ptr)((&mut vm as *mut Vm).cast()) };

    // SAFETY: r0 exists and holds a Str
    assert_eq!(unsafe { vm.r(0).as_str() }, "grace");

    let name = vm.new_string("lin".to_owned());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = name;
    // SAFETY: valid vm, r0 holds make_account's Str
    unsafe { (users::PACKAGE.fns[3].ptr)((&mut vm as *mut Vm).cast()) };
    // SAFETY: r0 exists
    let account = Account::from_vm(&vm, *unsafe { vm.r(0) });
    assert_eq!(account.profile.name, "lin");
    assert!(account.active);
}

#[test]
fn pg_fn_with_slots_declares_slot_types_and_moves_the_word() {
    use purple_garden::embed::Type;

    let id = &t::PACKAGE.fns[0];
    assert!(id.with_slots);
    assert_eq!(id.args, &[Type::Slot("T")]);
    assert_eq!(id.ret, Type::Slot("T"));

    let mut vm = Vm::new(VmConfig::default());
    // SAFETY: r0 exists
    *unsafe { vm.r_mut(0) } = Value::from(42_i64);
    // SAFETY: valid vm, r0 holds the word id moves
    unsafe { (id.ptr)((&mut vm as *mut Vm).cast()) };
    // SAFETY: r0 exists
    assert_eq!(unsafe { vm.r(0) }.as_int(), 42);
}

#[test]
fn with_slots_identity_round_trips_word_sized_values() {
    let mut program = purple_garden::Pg::new()
        .with_stdlib()
        .with_lib(&t::PACKAGE)
        .compile(
            br#"
            import ("t" "str")
            let a = t.id(3)
            let b = t.id("hello")
            let c = t.id(true)
            let d = t.id(1.5)
            str.concat(str.concat(str.from(a) t.id(b)) str.from(t.id(d) as Int))
        "#,
        )
        .expect("compiles");
    let out: String = program.run_take().expect("runs");
    assert_eq!(out, "3hello1");
}

/// The native entries take the vm first, `f64` for Double and a `Value` word for everything else,
/// SysV placing them in registers, see `NativeFn`.
#[test]
fn pg_fn_native_entries_take_and_return_hardware_values() {
    use purple_garden::embed::NativeFn;

    fn native(name: &str) -> Option<NativeFn> {
        native::PACKAGE
            .fns
            .iter()
            .find(|f| f.name == name)
            .unwrap()
            .native
    }
    let mut vm = Vm::new(VmConfig::default());
    let name = vm.new_string("garden".to_owned());
    let vm_ptr = (&mut vm as *mut Vm).cast::<std::ffi::c_void>();

    // SAFETY: add's native entry has this signature
    let add = unsafe {
        std::mem::transmute::<
            NativeFn,
            unsafe extern "C" fn(*mut std::ffi::c_void, Value, Value) -> Value,
        >(native("add").unwrap())
    };
    // SAFETY: vm_ptr is the live vm, args match add
    assert_eq!(
        unsafe { add(vm_ptr, Value::from(40_i64), Value::from(2_i64)) }.as_int(),
        42
    );

    // SAFETY: scale's native entry has this signature
    let scale = unsafe {
        std::mem::transmute::<
            NativeFn,
            unsafe extern "C" fn(*mut std::ffi::c_void, f64, Value, f64) -> f64,
        >(native("scale").unwrap())
    };
    // SAFETY: vm_ptr is the live vm, args match scale
    assert_eq!(
        unsafe { scale(vm_ptr, 1.5, Value::from(2_i64), 0.25) },
        3.25
    );

    // SAFETY: greet's native entry has this signature
    let greet = unsafe {
        std::mem::transmute::<NativeFn, unsafe extern "C" fn(*mut std::ffi::c_void, Value) -> Value>(
            native("greet").unwrap(),
        )
    };
    // SAFETY: vm_ptr is the live vm, name is a Str
    let greeting = unsafe { greet(vm_ptr, name) };
    assert_eq!(<&str as FromVm>::from_vm(&vm, greeting), "hi garden");

    // SAFETY: id's native entry has this signature
    let id = unsafe {
        std::mem::transmute::<NativeFn, unsafe extern "C" fn(*mut std::ffi::c_void, Value) -> Value>(
            native("id").unwrap(),
        )
    };
    // SAFETY: vm_ptr is the live vm, args match id
    assert_eq!(unsafe { id(vm_ptr, Value::from(7_i64)) }.as_int(), 7);
}

/// `wide` has more params than argument registers and stays bytecode, its tail call to the native
/// Double function `square` reads the result from xmm0.
#[test]
fn bytecode_reads_double_results_of_native_calls() {
    let mut program = purple_garden::Pg::new()
        .compile(
            b"fn square(x:Double) Double { x * x }
fn wide(x:Double a:Int b:Int c:Int d:Int e:Int f:Int) Double { square(x) }
wide(1.5 1 2 3 4 5 6)",
        )
        .expect("compiles");
    let squared: f64 = program.run_take().expect("runs");
    assert_eq!(squared, 2.25);
}

/// A native entry hands its last value to the vm in rax, Doubles too.
#[test]
fn native_entry_returns_doubles_to_the_vm() {
    let mut program = purple_garden::Pg::new()
        .compile(b"fn half(x:Double) Double { x / 2.0 }\nlet h = half(5.0)\nh * 2.0")
        .expect("compiles");
    let value: f64 = program.run_take().expect("runs");
    assert_eq!(value, 5.0);
}
