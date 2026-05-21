use purple_garden_runtime::{Value, Vm};

pub fn repeat(vm: &mut Vm) {
    let arg0 = vm.r(0).as_str(&vm.strings);
    let arg1 = vm.r(1).as_int();
    let repeated = arg0.repeat(arg1 as usize);
    *vm.r_mut(0) = Value::from(vm.new_string(repeated));
}

pub fn contains(vm: &mut Vm) {
    let arg0 = vm.r(0).as_str(&vm.strings);
    let arg1 = vm.r(1).as_str(&vm.strings);
    *vm.r_mut(0) = Value::from(arg0.contains(arg1));
}

pub fn len(vm: &mut Vm) {
    *vm.r_mut(0) = Value::from(vm.r(0).as_str(&vm.strings).len());
}
