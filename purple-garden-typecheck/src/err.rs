use purple_garden_frontend::{
    ast::NodeId,
    diagnostic::{Diagnostic, Help, Span},
    lex,
};
use purple_garden_ir::ptype::Type;

use crate::{FunctionType, Typechecker};

impl<'a, 't> Typechecker<'a, 't> {
    pub(crate) fn report(&mut self, diagnostic: Diagnostic) {
        self.diagnostics.push(diagnostic);
    }

    pub(crate) fn redundant_conversion_note(
        &self,
        args: &[NodeId],
        candidates: &[FunctionType<'t>],
    ) -> Option<String> {
        if args.len() != 1 {
            return None;
        }

        let provided_ty = self.resolved_arg_ty(args[0]);
        if !candidates
            .iter()
            .all(|c| c.args.len() == 1 && &c.ret == provided_ty)
        {
            return None;
        }

        let arg = self.node_label(args[0]).unwrap_or("the argument");
        Some(format!("`{arg}` is already {provided_ty}"))
    }

    pub(crate) fn redundant_cast_error(at: &lex::Token, ty: &Type<'t>) -> Diagnostic {
        Diagnostic::at_token(format!("Can not cast {ty} to {ty}"), at)
            .with_primary_message("unnecessary cast")
            .with_note(format!("the expression is already {ty}"))
            .with_help(Help::new("remove the cast"))
    }

    pub(crate) fn missing_package_error(
        &mut self,
        pkg_name: &'t str,
        pkg_tok: &lex::Token,
    ) -> Diagnostic {
        let mut err = Diagnostic::at_token(format!("Can't find package `{pkg_name}`"), pkg_tok)
            .with_primary_message("package used here");
        if self.resolve_pkg(pkg_name).is_some() {
            err = err
                .with_note(format!("package `{pkg_name}` exists but is not imported"))
                .with_help(
                    Help::new(format!("add `import \"{pkg_name}\"`"))
                        .with_replacement(Span::new(0, 0), format!("import \"{pkg_name}\"\n")),
                );
        }
        err
    }

    pub(crate) fn specialisation_miss_error(
        &self,
        pkg_name: &str,
        inner_name: &str,
        name: &lex::Token,
        args: &[NodeId],
        candidates: &[FunctionType<'t>],
    ) -> Diagnostic {
        fn sig<'a, 'b: 'a>(types: impl Iterator<Item = &'a Type<'b>>) -> String {
            types
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        }

        let provided = || args.iter().map(|&a| self.resolved_arg_ty(a));
        let avail = candidates
            .iter()
            .map(|c| sig(c.args.iter().map(|(_, t)| t)))
            .collect::<Vec<_>>()
            .join(" | ");
        let mut err = Diagnostic::at_token(
            format!(
                "no specialisation of `{pkg_name}.{inner_name}` accepts ({}); available: {avail}",
                sig(provided())
            ),
            name,
        );
        if let Some(note) = self.redundant_conversion_note(args, candidates) {
            err = err
                .with_note(note)
                .with_help(Help::new(format!("remove `{pkg_name}.{inner_name}`")));
        }
        err
    }

    pub(crate) fn common_return(candidates: &[FunctionType<'t>]) -> Option<Type<'t>> {
        let first = candidates.first()?.ret.clone();
        candidates.iter().all(|c| c.ret == first).then_some(first)
    }
}
