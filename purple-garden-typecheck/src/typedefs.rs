use purple_garden_frontend::diagnostic::Diagnostic;
use purple_garden_ir::ptype::Type;

#[derive(Debug, Clone)]
pub(super) struct FunctionType<'t> {
    pub(super) args: Vec<(&'t str, Type<'t>)>,
    pub(super) ret: Type<'t>,
}

#[derive(Debug)]
pub struct TypecheckOutput<'t> {
    /// Node value id -> inferred type. Poisoned nodes stay `None`.
    ///
    /// This lets analysis clients use all types that were still knowable after
    /// errors without pretending the whole file typechecked successfully.
    pub types: Vec<Option<Type<'t>>>,
    pub diagnostics: Vec<Diagnostic>,
}

/// Internal typechecking result for one AST node.
///
/// `Known` means later nodes can safely use the type. `Poison` means the node
/// already produced, or depends on, an error and should not cause cascading
/// follow-up diagnostics. We keep this separate from `purple_garden_ir::Type`
/// so the IR/runtime type vocabulary does not need an error sentinel.
#[derive(Debug, Clone)]
pub(super) enum TcType<'t> {
    Known(Type<'t>),
    Poison,
}

impl<'t> TcType<'t> {
    pub(super) fn known(self) -> Option<Type<'t>> {
        match self {
            Self::Known(ty) => Some(ty),
            Self::Poison => None,
        }
    }

    pub(super) fn as_known(&self) -> Option<&Type<'t>> {
        match self {
            Self::Known(ty) => Some(ty),
            Self::Poison => None,
        }
    }
}
