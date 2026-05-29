use purple_garden_runtime::Anomaly;

crate::builtin! {
    pub fn assert(vm) {
        if !vm.r(0).as_bool() {
            vm.trap(Anomaly::Msg {
                msg: "test.assert: assertion failed",
                pc: vm.pc,
            });
        }
    }
}
