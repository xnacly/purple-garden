use purple_garden::{FromVm, IntoVm, PgType, Value, Vm, VmConfig, pg_pkg};

#[derive(PgType, FromVm, IntoVm)]
struct Counter {
    value: i64,
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
    use super::Counter;

    pub fn new_counter(value: i64) -> Counter {
        Counter { value }
    }

    #[purple_garden::pg_fn(pure)]
    pub fn value(counter: &Counter) -> i64 {
        counter.value
    }
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
fn pg_pkg_wrapper_decodes_args_and_encodes_return() {
    let mut vm = Vm::new(VmConfig::default());
    let idx = vm.new_string("hello".to_owned());
    *vm.r_mut(0) = Value::from(idx);

    unsafe { (strings::PACKAGE.fns[0].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_int(), 5);
}

#[test]
fn pg_pkg_wrapper_allocates_return_strings() {
    let mut vm = Vm::new(VmConfig::default());
    let idx = vm.new_string("ha".to_owned());
    *vm.r_mut(0) = Value::from(idx);
    *vm.r_mut(1) = Value::from(3_i64);

    unsafe { (strings::PACKAGE.fns[1].ptr)((&mut vm as *mut Vm).cast()) };

    assert_eq!(vm.r(0).as_str(vm.strings(), vm.string_data()), "hahaha");
}

#[test]
fn pg_pkg_supports_foreign_derived_types() {
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
