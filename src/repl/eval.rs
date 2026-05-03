use crate::{
    ast::Node,
    bc,
    config::Config,
    err::PgError,
    ir::{self, ptype::Type, typecheck::Typechecker},
    lex::Lexer,
    opt,
    parser::Parser,
    vm::Value,
};

const EVAL_FN: &str = "__pg_repl_eval";

pub(super) struct EvalOutput {
    pub(super) value: Option<String>,
}

pub(super) fn eval(
    config: &Config,
    snippets: &[String],
    input: &str,
) -> Result<(EvalOutput, bool), PgError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok((EvalOutput { value: None }, false));
    }

    let ast = parse(trimmed.as_bytes())?;
    let persist = is_persistent_snippet(&ast);
    let display = should_display_value(&ast);

    let output = if display {
        eval_expression(config, snippets, trimmed)?
    } else {
        let mut source = snippets.join("\n");
        if !source.is_empty() {
            source.push('\n');
        }
        source.push_str(trimmed);
        source.push('\n');
        eval_source(config, &source, None)?
    };

    Ok((output, persist))
}

fn parse(input: &[u8]) -> Result<Vec<Node<'_>>, PgError> {
    Parser::new(Lexer::new(input))?.parse()
}

fn eval_expression(
    config: &Config,
    snippets: &[String],
    input: &str,
) -> Result<EvalOutput, PgError> {
    let ty = repl_block_type(snippets, input)?;
    let source = eval_function_source(snippets, input, &ty);
    eval_source(config, &source, Some(ty))
}

fn repl_block_type(snippets: &[String], input: &str) -> Result<Type, PgError> {
    let mut source = snippets.join("\n");
    if !source.is_empty() {
        source.push('\n');
    }
    source.push_str(input);

    let ast = parse(source.as_bytes())?;
    Typechecker::block(&ast)
}

fn eval_function_source(snippets: &[String], input: &str, ty: &Type) -> String {
    let mut source = String::new();
    source.push_str("fn ");
    source.push_str(EVAL_FN);
    source.push_str("() ");
    source.push_str(&type_source(ty));
    source.push_str(" {\n");
    for snippet in snippets {
        source.push_str(snippet);
        source.push('\n');
    }
    source.push_str(input);
    source.push_str("\n}\n");
    source.push_str(EVAL_FN);
    source.push_str("()\n");
    source
}

fn type_source(ty: &Type) -> String {
    match ty {
        Type::Void => "void".into(),
        Type::Bool => "bool".into(),
        Type::Int => "int".into(),
        Type::Double => "double".into(),
        Type::Str => "str".into(),
        Type::Option(inner) => format!("?{}", type_source(inner)),
        Type::Array(inner) => format!("[{}]", type_source(inner)),
        Type::Foreign(id) => panic!("foreign type `{id}` can not be represented in repl source"),
    }
}

fn eval_source(config: &Config, source: &str, ty: Option<Type>) -> Result<EvalOutput, PgError> {
    let ast = parse(source.as_bytes())?;
    let lower = ir::lower::Lower::new();
    let mut ir = lower.ir_from(&ast)?;

    if config.opt >= 1 {
        opt::ir(&mut ir);
    }

    let mut cc = bc::Cc::new();
    cc.compile(config, &ir)?;

    if config.opt >= 1 {
        opt::bc(&mut cc.buf);
    }

    let mut vm = cc.finalize(config);
    vm.run()?;

    let value = if let Some(ty) = &ty {
        format_value(ty, *vm.r(0), &vm.strings)
    } else {
        None
    };

    Ok(EvalOutput { value })
}

fn format_value(ty: &Type, value: Value, strings: &[Box<str>]) -> Option<String> {
    Some(match ty {
        Type::Void => return None,
        Type::Bool => value.as_bool().to_string(),
        Type::Int => value.as_int().to_string(),
        Type::Double => value.as_f64().to_string(),
        Type::Str => value.as_str(strings).to_string(),
        Type::Option(_) | Type::Array(_) | Type::Foreign(_) => format!("{value:?}"),
    })
}

fn is_persistent_snippet(ast: &[Node<'_>]) -> bool {
    !ast.is_empty()
        && ast.iter().all(|node| {
            matches!(
                node,
                Node::Let { .. } | Node::Fn { .. } | Node::Import { .. }
            )
        })
}

fn should_display_value(ast: &[Node<'_>]) -> bool {
    ast.last().is_some_and(|node| {
        !matches!(
            node,
            Node::Let { .. } | Node::Fn { .. } | Node::Import { .. }
        )
    })
}
