use crate::vm::{Anomaly, Value, Vm};

fn println(vm: &mut Vm) -> Result<Value, Anomaly> {
    println!("{}", vm.r[0].as_str(&vm.strings));
    Ok(Value(0))
}

fn print(vm: &mut Vm) -> Result<Value, Anomaly> {
    print!("{}", vm.r[0].as_str(&vm.strings));
    Ok(Value(0))
}
