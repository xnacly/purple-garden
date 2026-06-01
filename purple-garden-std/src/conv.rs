use purple_garden_runtime::IntoVm;

crate::builtin! {
    pub fn from_int(vm) {
        let s = vm.r(0).as_int().to_string();
        let ret = s.into_vm(vm);
        *vm.r_mut(0) = ret;
    }

    pub fn from_double(vm) {
        let s = vm.r(0).as_f64().to_string();
        let ret = s.into_vm(vm);
        *vm.r_mut(0) = ret;
    }
}
