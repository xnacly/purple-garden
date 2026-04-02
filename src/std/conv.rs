use crate::vm::{Anomaly, Value, Vm};

pub fn from_int(vm: &mut Vm) -> Value {
    let arg0 = vm.r(0).as_int();
    let as_string = arg0.to_string();
    Value::from(vm.new_string(as_string))
}

pub fn from_double(vm: &mut Vm) -> Value {
    let arg0 = vm.r(0).as_f64();
    let as_string = arg0.to_string();
    Value::from(vm.new_string(as_string))
}
