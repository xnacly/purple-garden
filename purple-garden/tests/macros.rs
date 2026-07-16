use purple_garden::{FromVm, GardenOpaque, GardenValue, IntoVm, Value, Vm, VmConfig, pg_pkg};

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
mod strings {
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
    use purple_garden::Vm;

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
        vm.r(0).as_int() + value
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
    assert_eq!(strings::PACKAGE.name, "strings");
    assert_eq!(strings::PACKAGE.fns.len(), 3);
    assert_eq!(strings::PACKAGE.fns[0].name, "len");
    assert!(strings::PACKAGE.fns[0].pure);
    assert_eq!(strings::PACKAGE.fns[0].arg_names, &["s"]);
    assert!(!strings::PACKAGE.fns[1].pure);
    assert_eq!(strings::PACKAGE.fns[1].arg_names, &["s", "n"]);
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
    *vm.r_mut(0) = s;

    unsafe { (strings::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_int(), 5);
}

#[test]
fn pg_pkg_wrapper_allocates_return_strings() {
    let mut vm = Vm::new(VmConfig::default());
    let s = vm.new_string("ha".to_owned());
    *vm.r_mut(0) = s;
    *vm.r_mut(1) = Value::from(3_i64);

    unsafe { (strings::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_str(), "hahaha");
}

#[test]
fn pg_pkg_supports_garden_opaque_types() {
    assert_eq!(
        counters::PACKAGE.fns[0].ret,
        purple_garden::Type::Foreign("Counter")
    );
    assert_eq!(
        counters::PACKAGE.fns[1].args,
        &[purple_garden::Type::Foreign("Counter")]
    );

    let mut vm = Vm::new(VmConfig::default());
    *vm.r_mut(0) = Value::from(7_i64);

    unsafe { (counters::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };
    unsafe { (counters::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_int(), 7);
}

#[test]
fn pg_fn_unsafe_passes_vm_and_exposes_remaining_signature() {
    let fun = &counters::PACKAGE.fns[2];
    assert_eq!(fun.name, "add_register_zero");
    assert_eq!(fun.arg_names, &["value"]);
    assert_eq!(fun.args, &[purple_garden::Type::Int]);
    assert_eq!(fun.ret, purple_garden::Type::Int);

    let mut vm = Vm::new(VmConfig::default());
    *vm.r_mut(0) = Value::from(40_i64);

    unsafe { (fun.ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_int(), 80);
}

#[test]
fn pg_pkg_exposes_record_metadata() {
    assert_eq!(
        users::PACKAGE.fns[0].args,
        &[purple_garden::Type::Record(
            purple_garden::RecordFields::Static(&[
                purple_garden::Field {
                    name: "name",
                    ty: purple_garden::Type::Str,
                },
                purple_garden::Field {
                    name: "age",
                    ty: purple_garden::Type::Int,
                },
            ])
        )]
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
    *vm.r_mut(0) = record;

    unsafe { (users::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_str(), "teo");
}

#[test]
fn pg_pkg_wrapper_encodes_record_return() {
    let mut vm = Vm::new(VmConfig::default());
    let name = vm.new_string("ada".to_owned());
    *vm.r_mut(0) = name;
    *vm.r_mut(1) = Value::from(37_i64);

    unsafe { (users::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    let user = User::from_vm(&vm, *vm.r(0));
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
    *vm.r_mut(0) = account;

    unsafe { (users::PACKAGE.fns[2].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_str(), "grace");

    let name = vm.new_string("lin".to_owned());
    *vm.r_mut(0) = name;
    unsafe { (users::PACKAGE.fns[3].ptr)((&mut vm as *mut Vm).cast()) };
    let account = Account::from_vm(&vm, *vm.r(0));
    assert_eq!(account.profile.name, "lin");
    assert!(account.active);
}
