use purple_garden_runtime::{Value, Vm};

pub fn from_int(vm: &mut Vm) {
    let s = vm.r(0).as_int().to_string();
    *vm.r_mut(0) = Value::from(vm.new_string(s));
}

pub fn from_double(vm: &mut Vm) {
    let s = vm.r(0).as_f64().to_string();
    *vm.r_mut(0) = Value::from(vm.new_string(s));
}
