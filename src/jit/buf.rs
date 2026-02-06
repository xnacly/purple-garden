use crate::vm::Vm;

pub struct ExecBuffer {
    ptr: *mut u8,
    size: usize,
}

impl ExecBuffer {
    pub fn new() -> Self {
        todo!();
    }

    /// after encoding the instructions into the buffer, mark it as write protected
    pub fn protect(&mut self) {}

    /// turn the encoded buffer into a compiled function, arguments are extraced from the vm
    /// pointer
    pub fn as_fn(&mut self, entry_offset: usize) -> extern "C" fn(&mut Vm) {
        unsafe {
            // using extern "C" here should make it so f is callable via the c calling convention
            // which is compatible with the way we compiled our functions
            let f: extern "C" fn(&mut Vm) = std::mem::transmute(self.ptr.add(entry_offset));
            f
        }
    }

    /// destory and deallocate the buffer
    pub fn destroy(&mut self) {}
}
