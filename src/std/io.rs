use crate::vm::{Anomaly, Value, Vm};

pub fn println(vm: &mut Vm) -> Result<Value, Anomaly> {
    println!("{}", vm.r(0).as_str(&vm.strings));
    Ok(Value(0))
}

pub fn print(vm: &mut Vm) -> Result<Value, Anomaly> {
    print!("{}", vm.r(0).as_str(&vm.strings));
    Ok(Value(0))
}
