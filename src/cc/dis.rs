use crate::{cc::Cc, op::Op::LoadGlobal};

impl Cc<'_> {
    // TODO: disconnect this from the compiler so finalize doesnt annoy us
    pub fn dis(&self) {
        for (i, b) in self.buf.iter().enumerate() {
            print!("[{:04}] {:?}", i, b);
            match b {
                LoadGlobal { idx, .. } => {
                    println!("; {:?}", self.ctx.globals_vec[*idx as usize])
                }
                _ => println!(),
            }
        }
    }
}
