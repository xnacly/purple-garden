use purple_garden_std::{IntoVm, Value, Vm};

pub trait CallArgs {
    fn inner(self, vm: &mut Vm) -> Vec<Value>;
}

// TODO: figure that one out

// macro_rules! impl_call_args {
//     // Match one or more (index, type) pairs.
//     ( $( ($idx:tt, $ty:ident) ),+ $(,)? ) => {
//         impl< $($ty: IntoVm),+ > CallArgs for ($($ty,)+) {
//             fn write_to_vm(self, vm: &mut Vm) {
//                 $( *vm.r_mut($idx) = self.$idx.into_vm(vm); )+
//             }

//             fn arity() -> usize {
//                 // Count the type parameters by summing 1 for each (this sucks @rust)
//                 0 $( + { let _ = stringify!($ty); 1usize } )+
//             }
//         }
//     };
// }

// impl_call_args!((0, T1), (1, T2));
// impl_call_args!((0, T1), (1, T2), (2, T3));
// impl_call_args!((0, T1), (1, T2), (2, T3), (3, T4));
// impl_call_args!((0, T1), (1, T2), (2, T3), (3, T4), (4, T5));
// impl_call_args!((0, T1), (1, T2), (2, T3), (3, T4), (4, T5), (5, T6));

impl<T: IntoVm> CallArgs for T {
    fn inner(self, vm: &mut Vm) -> Vec<Value> {
        vec![self.into_vm(vm)]
    }
}
