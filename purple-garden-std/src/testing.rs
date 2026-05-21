use purple_garden_runtime::{Anomaly, Vm};

pub fn assert(vm: &mut Vm) {
    if !vm.r(0).as_bool() {
        vm.trap(Anomaly::Msg {
            msg: "test.assert: assertion failed",
            pc: vm.pc,
        });
    }
}
