use purple_garden_ir::ptype::Type;

use super::{FromVm, IntoVm, PgType, Value};
use crate::vm::Vm;

/// A garden `Str` as builtins handle it. Builtins only construct and read it, the representation
/// is private, so small strings or ropes can be added without touching them.
///
/// Every `Str` a builtin receives is a string interned in the vm, passed through as the [`Value`]
/// it already is: neither copied out of the vm nor interned again on the way back.
///
/// A `Str` a builtin builds without the vm is interned once it is returned. Up to [`Str::SMALL`]
/// bytes are packed into one register word and interned from it, so building one the vm already
/// holds neither allocates nor touches memory.
pub struct Str(Repr);

enum Repr {
    Interned(Value),
    /// bytes little endian, zero above `len`, see [`crate::string::small_word`]
    Small {
        len: u8,
        word: u64,
    },
    Heap(String),
}

impl Str {
    pub const SMALL: usize = 8;

    /// `s` interned in `vm`.
    pub fn new(vm: &mut Vm, s: &str) -> Self {
        Self(Repr::Interned(vm.new_string_from_str(s)))
    }

    #[must_use]
    pub fn concat(parts: &[&str]) -> Self {
        let len: usize = parts.iter().map(|p| p.len()).sum();
        if len > Self::SMALL {
            return Self(Repr::Heap(parts.concat()));
        }
        let mut word = 0;
        let mut at = 0;
        for p in parts.iter().filter(|p| !p.is_empty()) {
            word |= crate::string::small_word(p.as_bytes()) << (8 * at);
            at += p.len();
        }
        Self(Repr::Small {
            len: len as u8,
            word,
        })
    }

    /// # Safety
    ///
    /// The low `len` bytes of `word` are utf-8, little endian, zero above, `len` at most
    /// [`Str::SMALL`].
    #[must_use]
    pub unsafe fn from_small(len: usize, word: u64) -> Self {
        debug_assert!(len <= Self::SMALL);
        Self(Repr::Small {
            len: len as u8,
            word,
        })
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        const _: () = assert!(cfg!(target_endian = "little"));
        match &self.0 {
            Repr::Interned(value) => value.as_str(),
            // SAFETY: the word holds whole strs back to back, little endian like memory
            Repr::Small { len, word } => unsafe {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                    std::ptr::from_ref(word).cast::<u8>(),
                    usize::from(*len),
                ))
            },
            Repr::Heap(s) => s,
        }
    }
}

impl From<&str> for Str {
    fn from(s: &str) -> Self {
        Self::concat(&[s])
    }
}

impl PgType for Str {
    const TYPE: Type<'static> = Type::Str;
}

impl FromVm<'_> for Str {
    fn from_vm(_: &Vm, value: Value) -> Self {
        Self(Repr::Interned(value))
    }
}

impl IntoVm for Str {
    fn into_vm(self, vm: &mut Vm) -> Value {
        match self.0 {
            Repr::Interned(value) => value,
            Repr::Small { len, word } => vm.new_string_from_small(usize::from(len), word),
            Repr::Heap(s) => vm.new_string_from_str(&s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Str;
    use crate::{FromVm, IntoVm, Vm, VmConfig};

    #[test]
    fn round_trips_the_interned_value() {
        let mut vm = Vm::new(VmConfig::default());
        let interned = vm.new_string_from_str("garden");

        let s = Str::new(&mut vm, "garden");
        assert_eq!(s.as_str(), "garden");
        assert_eq!(s.into_vm(&mut vm), interned);

        let s = Str::from_vm(&vm, interned);
        assert_eq!(s.into_vm(&mut vm), interned);
    }

    #[test]
    fn concat_interns_small_and_heap_alike() {
        let mut vm = Vm::new(VmConfig::default());
        let at = "g".repeat(Str::SMALL - 1);
        let past = "g".repeat(Str::SMALL);
        for (parts, joined) in [
            (["", ""], String::new()),
            ([at.as_str(), "!"], at.clone() + "!"),
            ([past.as_str(), "!"], past.clone() + "!"),
        ] {
            let interned = vm.new_string_from_str(&joined);
            let s = Str::concat(&parts);
            assert_eq!(s.as_str(), joined);
            assert_eq!(s.into_vm(&mut vm), interned);
        }
    }
}
