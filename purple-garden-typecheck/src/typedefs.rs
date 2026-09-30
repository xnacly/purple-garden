use std::{collections::HashMap, fmt::Display};

use purple_garden_frontend::{ast::Ast, diagnostic::Diagnostic};
use purple_garden_ir::ptype::Type;

#[derive(Debug, Clone)]
pub struct FunctionType<'t> {
    pub args: Vec<(&'t str, Type<'t>)>,
    pub ret: Type<'t>,
    /// Signature contains `Type::Slot`s, gates the generic binding path in call checking
    pub with_slots: bool,
}

#[derive(Debug)]
pub struct TypecheckOutput<'t> {
    /// Node value id -> inferred type. Poisoned nodes stay `None`.
    ///
    /// This lets analysis clients use all types that were still knowable after
    /// errors without pretending the whole file typechecked successfully.
    pub types: Vec<Option<Type<'t>>>,
    pub diagnostics: Vec<Diagnostic>,
    pub functions: HashMap<&'t str, FunctionType<'t>>,
}

/// Internal typechecking result for one AST node.
///
/// Types live in the typechecker's map only, a result just names the slot, so consumers read the
/// type back by reference instead of cloning it out per node. We keep this separate from
/// `purple_garden_ir::Type` so the IR/runtime type vocabulary does not need an error sentinel.
#[derive(Debug, Clone, Copy)]
pub(super) enum TcType {
    /// value id whose map slot holds the type, later nodes can safely use it
    Known(usize),
    /// means the node already produced, or depends on, an error and should not cause cascading
    /// follow-up diagnostics.
    Poison,
}

impl TcType {
    pub(super) fn known(self) -> Option<usize> {
        match self {
            Self::Known(id) => Some(id),
            Self::Poison => None,
        }
    }
}

/// Everything a call check writes to. A signature is borrowed straight out of the function tables
/// while the call is checked, which rules out `&mut Typechecker`, so the written fields are split
/// off instead of cloning the signature.
pub(super) struct CallSink<'s, 'a, 't> {
    pub(super) ast: &'a Ast<'t>,
    pub(super) map: &'s mut Vec<Option<Type<'t>>>,
    pub(super) diagnostics: &'s mut Vec<Diagnostic>,
}

/// `pkg.name` or `name`, only formatted when a diagnostic needs it
pub(super) struct CallName<'n> {
    pub(super) pkg: Option<&'n str>,
    pub(super) name: &'n str,
}

impl Display for CallName<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(pkg) = self.pkg {
            write!(f, "{pkg}.")?;
        }
        f.write_str(self.name)
    }
}
