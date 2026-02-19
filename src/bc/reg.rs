#[derive(Debug, PartialEq, Eq)]
pub struct Reg {
    id: u8,
    perm: bool,
}

impl From<u8> for Reg {
    fn from(value: u8) -> Self {
        Reg {
            id: value,
            perm: false,
        }
    }
}

impl From<Reg> for u8 {
    fn from(value: Reg) -> Self {
        value.id
    }
}

impl From<&Reg> for u8 {
    fn from(value: &Reg) -> Self {
        value.id
    }
}
