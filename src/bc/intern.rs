use std::collections::HashMap;
use std::mem::MaybeUninit;

#[derive(Debug, Clone)]
pub struct Interner<T> {
    pub map: HashMap<T, u32>,
    next_id: u32,
}

impl<T: std::hash::Hash + Eq> Interner<T> {
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
            next_id: 0,
        }
    }

    pub fn intern(&mut self, v: T) -> u32 {
        if let Some(&id) = self.map.get(&v) {
            id
        } else {
            let id = self.next_id;
            self.map.insert(v, id);
            self.next_id += 1;
            id
        }
    }

    pub fn into_vec(self) -> Vec<T> {
        let len = self.next_id as usize;
        let mut vec: Vec<MaybeUninit<T>> = Vec::with_capacity(len);

        unsafe {
            vec.set_len(len);
        }

        for (v, i) in self.map {
            vec[i as usize].write(v);
        }

        unsafe { std::mem::transmute::<Vec<MaybeUninit<T>>, Vec<T>>(vec) }
    }

    pub fn into_vec_fn<O>(self, modify: fn(T) -> O) -> Vec<O> {
        let len = self.next_id as usize;
        let mut vec: Vec<MaybeUninit<O>> = Vec::with_capacity(len);

        unsafe {
            vec.set_len(len);
        }

        for (v, i) in self.map {
            vec[i as usize].write(modify(v));
        }

        unsafe { std::mem::transmute::<Vec<MaybeUninit<O>>, Vec<O>>(vec) }
    }
}

#[cfg(test)]
mod tests {
    use crate::ir::Const;

    use super::*;

    #[test]
    fn intern_str_happy_path() {
        let mut i = Interner::new();

        let a = i.intern("hello");
        let b = i.intern("world");

        assert_eq!(a, 0);
        assert_eq!(b, 1);
    }

    #[test]
    fn intern_str_deduplicates() {
        let mut i = Interner::new();

        let a1 = i.intern("hello");
        let a2 = i.intern("hello");

        assert_eq!(a1, 0);
        assert_eq!(a2, 0);
        assert_eq!(i.map.len(), 1);
    }

    #[test]
    fn intern_const_happy_path() {
        let mut i = Interner::new();

        let a = i.intern(Const::Int(10));
        let b = i.intern(Const::Str("abc"));

        assert_eq!(a, 0);
        assert_eq!(b, 1);
    }

    #[test]
    fn intern_const_deduplicates() {
        let mut i = Interner::new();

        let a = i.intern(Const::Int(42));
        let b = i.intern(Const::Int(42));

        assert_eq!(a, 0);
        assert_eq!(b, 0);
        assert_eq!(i.map.len(), 1);
    }

    #[test]
    fn to_vec_preserves_indices_for_str() {
        let mut i = Interner::new();

        i.intern("a");
        i.intern("b");
        i.intern("c");

        let v = i.into_vec();

        assert_eq!(v[0], "a");
        assert_eq!(v[1], "b");
        assert_eq!(v[2], "c");
    }

    #[test]
    fn to_vec_preserves_indices_for_const() {
        let mut i = Interner::new();

        i.intern(Const::Int(1));
        i.intern(Const::Int(2));
        i.intern(Const::Str("x"));

        let v = i.into_vec();

        assert_eq!(v[0], Const::Int(1));
        assert_eq!(v[1], Const::Int(2));
        assert_eq!(v[2], Const::Str("x"));
    }

    #[test]
    fn to_vec_fn_transforms_values() {
        let mut i = Interner::new();

        i.intern(Const::Int(1));
        i.intern(Const::Int(2));

        let v = i.into_vec_fn(|c| match c {
            Const::Int(v) => v,
            _ => 0,
        });

        assert_eq!(v[0], 1);
        assert_eq!(v[1], 2);
    }

    #[test]
    fn edge_case_empty_interner() {
        let i: Interner<&str> = Interner::new();

        let v = i.into_vec();

        assert!(v.is_empty());
    }

    #[test]
    fn edge_case_many_strings() {
        let mut i = Interner::new();

        for n in 0..1000 {
            let s = format!("str{n}");
            let leaked: &'static str = Box::leak(s.into_boxed_str());
            assert_eq!(i.intern(leaked), n);
        }

        let v = i.into_vec();

        assert_eq!(v.len(), 1000);
    }
}
