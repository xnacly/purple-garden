use crate::{config::Config, err::PgError};

use super::eval::{self, EvalOutput};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReplControl {
    Continue,
    Quit,
}

#[derive(Default)]
pub(super) struct ReplSession {
    snippets: Vec<String>,
}

impl ReplSession {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn eval(&mut self, config: &Config, input: &str) -> Result<EvalOutput, PgError> {
        let trimmed = input.trim();
        let (output, persist) = eval::eval(config, &self.snippets, trimmed)?;
        if persist {
            self.snippets.push(trimmed.to_string());
        }

        Ok(output)
    }

    fn clear(&mut self) {
        self.snippets.clear();
    }

    fn state(&self) -> String {
        self.snippets.join("\n")
    }

    pub(super) fn handle_meta(&mut self, input: &str) -> Option<ReplControl> {
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
}

pub(super) fn should_store_meta(input: &str) -> bool {
    matches!(input, ":clear" | ":state" | ":help")
}

pub(super) fn needs_more_input(input: &str) -> bool {
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

    #[test]
    fn meta_history_policy_excludes_quit() {
        assert!(should_store_meta(":clear"));
        assert!(should_store_meta(":state"));
        assert!(should_store_meta(":help"));
        assert!(!should_store_meta(":q"));
        assert!(!should_store_meta(":quit"));
    }
}
