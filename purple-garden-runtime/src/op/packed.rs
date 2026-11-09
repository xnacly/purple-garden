#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
/// Packed version of [Op], since [Op] is 16 bytes and branching on it in the interpreter costs big
/// fetches, making it 4 bytes large results in better locality, more instructions per prefetch and
/// less decoding work, since the vm can simply call [Packed::tag()] to branch on it
pub struct Packed(u32);

impl Packed {
    /// first 6 bit decoded into [Tag]
    pub fn tag(&self) -> super::Tag {
        let tag_raw = (self.0 & 0b0011_1111) as u8;
        unsafe { std::mem::transmute::<u8, super::Tag>(tag_raw) }
    }
}

impl super::Op {
    pub fn encode(self) -> Packed {
        todo!()
    }
}
