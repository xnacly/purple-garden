use std::marker::PhantomData;

#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Gc<T: ?Sized> {
    // TODO:
    _phantom: PhantomData<T>,
}
