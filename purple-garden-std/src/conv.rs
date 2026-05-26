use purple_garden_runtime::Value;

crate::builtin! {
    pub fn from_int(vm) {
        let s = vm.r(0).as_int().to_string();
        *vm.r_mut(0) = Value::from(vm.new_string(s));
    }

    pub fn from_double(vm) {
        let s = vm.r(0).as_f64().to_string();
        *vm.r_mut(0) = Value::from(vm.new_string(s));
    }
}
