use purple_garden_runtime::IntoVm;

crate::builtin! {
    pub fn repeat(vm) {
        let arg0 = vm.r(0).as_str(vm.strings(), vm.string_data());
        let arg1 = vm.r(1).as_int();
        let repeated = arg0.repeat(arg1 as usize);
        let ret = repeated.into_vm(vm);
        *vm.r_mut(0) = ret;
    }

    pub fn contains(vm) {
        let arg0 = vm.r(0).as_str(vm.strings(), vm.string_data());
        let arg1 = vm.r(1).as_str(vm.strings(), vm.string_data());
        let ret = arg0.contains(arg1).into_vm(vm);
        *vm.r_mut(0) = ret;
    }

    pub fn len(vm) {
        let ret = (vm.r(0).as_str(vm.strings(), vm.string_data()).len() as i64).into_vm(vm);
        *vm.r_mut(0) = ret;
    }
}
