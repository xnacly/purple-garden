use purple_garden_runtime::{IntoVm, PgType, Type, Value, Vm};

/// Enables passing non homogeneous arguments when invoking a purple garden function with
/// `Program::call`, by default the trait is implemented for a non homogeneous argument tuple of
/// size 0 to 6
///
/// Using this abstraction can be circumvented by using `Programm::call_unchecked` and converting
/// the arguments into purple_garden_runtime::Value beforehand
pub trait CallArgs {
    type Values: AsRef<[Value]>;
    const TYPES: &'static [Type<'static>];
    fn inner(self, vm: &mut Vm) -> Self::Values;
}

macro_rules! impl_call_args {
    // Match the arity, then one or more (index, type) pairs.
    ( $arity:literal; $( ($idx:tt, $ty:ident) ),+ $(,)? ) => {
        impl< $($ty: IntoVm+PgType),+ > CallArgs for ($($ty,)+) {
            type Values = [Value; $arity];

            const TYPES: &'static [Type<'static>] = &[$( <$ty as PgType>::TYPE, )+];

            fn inner(self, vm: &mut Vm) -> Self::Values {
                [$( self.$idx.into_vm(vm), )+]
            }
        }
    };
}

impl_call_args!(1; (0, T1));
impl_call_args!(2; (0, T1), (1, T2));
impl_call_args!(3; (0, T1), (1, T2), (2, T3));
impl_call_args!(4; (0, T1), (1, T2), (2, T3), (3, T4));
impl_call_args!(5; (0, T1), (1, T2), (2, T3), (3, T4), (4, T5));
impl_call_args!(6; (0, T1), (1, T2), (2, T3), (3, T4), (4, T5), (5, T6));
