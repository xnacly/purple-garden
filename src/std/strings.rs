use crate::vm::{Anomaly, Value, Vm};

pub fn repeat(vm: &mut Vm) -> Value {
    let arg0 = vm.r[0].as_str(&vm.strings);
    let arg1 = vm.r[1].as_int();
    let repeated = arg0.repeat(arg1 as usize);
    Value::from(vm.new_string(repeated))
}

pub fn contains(vm: &mut Vm) -> Value {
    let arg0 = vm.r[0].as_str(&vm.strings);
    let arg1 = vm.r[1].as_str(&vm.strings);
    Value::from(arg0.contains(arg1))
}

pub fn len(vm: &mut Vm) -> Value {
    Value::from(vm.r[0].as_str(&vm.strings).len())
}
