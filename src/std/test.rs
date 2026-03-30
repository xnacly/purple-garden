use crate::vm::{Anomaly, Value, Vm};

pub fn assert(vm: &mut Vm) -> Value {
    if !vm.r[0].as_bool() {
        // TODO: replace this with some kind of error as a value? No idea
        panic!("test.assert: assertion failed")
    }
    Value(0)
}
