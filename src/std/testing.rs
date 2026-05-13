use crate::vm::{Anomaly, Value, Vm};

pub fn assert(vm: &mut Vm) -> Result<Value, Anomaly> {
    if !vm.r(0).as_bool() {
        Err(Anomaly::Msg {
            msg: "test.assert: assertion failed",
            pc: vm.pc,
        })
    } else {
        Ok(Value(0))
    }
}
