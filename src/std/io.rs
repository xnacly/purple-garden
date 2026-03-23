use crate::vm::{Anomaly, Value, Vm};

pub fn println(vm: &mut Vm) -> Value {
    println!("{}", vm.r[0].as_str(&vm.strings));
    Value(0)
}

pub fn print(vm: &mut Vm) -> Value {
    print!("{}", vm.r[0].as_str(&vm.strings));
    Value(0)
}
