use std::io::{self, Write};

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

pub struct Repl;

#[derive(Debug, PartialEq, Eq)]
pub enum ReplControl {
    Continue,
    Quit,
}

#[derive(Default)]
pub struct ReplSession {
    snippets: Vec<String>,
}

pub struct EvalOutput {
    pub value: Option<String>,
}

const EVAL_FN: &str = "__pg_repl_eval";

impl Repl {
    pub fn start(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let mut session = ReplSession::new();
        let mut pending = String::new();
        let stdin = io::stdin();

        loop {
            if pending.is_empty() {
                print!("pg> ");
            } else {
                print!("..> ");
            }
            io::stdout().flush()?;

            let mut line = String::new();
            if stdin.read_line(&mut line)? == 0 {
                println!();
                break;
            }

            if pending.is_empty() {
                match session.handle_meta(line.trim()) {
                    Some(ReplControl::Quit) => break,
                    Some(ReplControl::Continue) => continue,
                    None => {}
                }
            }

            if line.trim().is_empty() && pending.is_empty() {
                continue;
            }

            pending.push_str(&line);
            if needs_more_input(&pending) {
                continue;
            }

            match session.eval(config, &pending) {
                Ok(output) => {
                    if let Some(value) = output.value {
                        println!("{value}");
                    }
                }
                Err(e) => {
                    let lines = pending.lines().collect::<Vec<&str>>();
                    print!("{}", e.render("repl", &lines));
                }
            }
            pending.clear();
        }

        Ok(())
    }
}

impl ReplSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn eval(&mut self, config: &Config, input: &str) -> Result<EvalOutput, PgError> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Ok(EvalOutput { value: None });
        }

        let ast = parse(trimmed.as_bytes())?;
        let persist = is_persistent_snippet(&ast);
        let display = should_display_value(&ast);

        let output = if display {
            eval_expression(config, &self.snippets, trimmed)?
        } else {
            let mut source = self.source();
            source.push_str(trimmed);
            source.push('\n');
            eval_source(config, &source, None)?
        };
        if persist {
            self.snippets.push(trimmed.to_string());
        }

        Ok(output)
    }

    pub fn clear(&mut self) {
        self.snippets.clear();
    }

    pub fn state(&self) -> String {
        self.snippets.join("\n")
    }

    pub fn handle_meta(&mut self, input: &str) -> Option<ReplControl> {
        match input {
            ":q" | ":quit" => Some(ReplControl::Quit),
            ":clear" => {
                self.clear();
                Some(ReplControl::Continue)
            }
            ":state" => {
                let state = self.state();
                if !state.is_empty() {
                    println!("{state}");
                }
                Some(ReplControl::Continue)
            }
            ":help" => {
                println!(":q, :quit  exit the repl");
                println!(":clear    clear session definitions");
                println!(":state    print session definitions");
                println!(":help     print this help");
                Some(ReplControl::Continue)
            }
            _ if input.starts_with(':') => {
                println!("unknown command `{input}`");
                Some(ReplControl::Continue)
            }
            _ => None,
        }
    }

    fn source(&self) -> String {
        let mut source = String::new();
        for snippet in &self.snippets {
            source.push_str(snippet);
            source.push('\n');
        }
        source
    }
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
    let mut source = String::new();
    for snippet in snippets {
        source.push_str(snippet);
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

fn needs_more_input(input: &str) -> bool {
    let trimmed = input.trim_end();
    if trimmed.is_empty() {
        return false;
    }

    let mut paren = 0i32;
    let mut curly = 0i32;
    let mut bracket = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for c in trimmed.chars() {
        if in_string {
            escaped = c == '\\' && !escaped;
            if c == '"' && !escaped {
                in_string = false;
            }
            if c != '\\' {
                escaped = false;
            }
            continue;
        }

        match c {
            '"' => in_string = true,
            '(' => paren += 1,
            ')' => paren -= 1,
            '{' => curly += 1,
            '}' => curly -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            _ => {}
        }
    }

    if in_string || paren > 0 || curly > 0 || bracket > 0 {
        return true;
    }

    if trimmed.starts_with("fn ") && !trimmed.contains('{') {
        return true;
    }

    if trimmed.starts_with("let ") && !trimmed.contains('=') {
        return true;
    }

    let last = trimmed.split_whitespace().last().unwrap_or_default();
    matches!(
        last,
        "let" | "fn" | "import" | "match" | "as" | ":" | "=" | "+" | "-" | "*" | "/" | "."
    ) || trimmed.ends_with(['=', '+', '-', '*', '/', '.', ':'])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config::default()
    }

    #[test]
    fn persists_let_bindings() {
        let mut session = ReplSession::new();
        assert_eq!(session.eval(&config(), "let x = 1").unwrap().value, None);
        assert_eq!(
            session.eval(&config(), "x + 2").unwrap().value,
            Some("3".into())
        );
    }

    #[test]
    fn persists_functions() {
        let mut session = ReplSession::new();
        session
            .eval(&config(), "fn inc(x:int) int { x + 1 }")
            .unwrap();
        assert_eq!(
            session.eval(&config(), "inc(4)").unwrap().value,
            Some("5".into())
        );
    }

    #[test]
    fn failed_input_does_not_mutate_session() {
        let mut session = ReplSession::new();
        assert!(session.eval(&config(), "let x = nope").is_err());
        assert!(session.eval(&config(), "x").is_err());
    }

    #[test]
    fn clear_removes_session_state() {
        let mut session = ReplSession::new();
        session.eval(&config(), "let x = 1").unwrap();
        session.clear();
        assert!(session.eval(&config(), "x").is_err());
    }

    #[test]
    fn buffers_multiline_input() {
        assert!(needs_more_input("fn inc(x:int) int {"));
        assert!(needs_more_input("match {\ntrue { 1 }"));
        assert!(!needs_more_input("fn inc(x:int) int { x + 1 }"));
    }
}
