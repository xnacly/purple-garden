mod editor;
mod eval;
mod raw;
mod session;

use crate::config::Config;
use editor::LineEditor;
use session::{ReplControl, ReplSession, should_store_meta};

pub struct Repl;

impl Repl {
    pub fn start(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
        let mut session = ReplSession::new();
        let mut editor = LineEditor::new();
        let mut pending = String::new();

        loop {
            let prompt = if pending.is_empty() { "pg> " } else { "..> " };
            let Some(line) = editor.read_line(prompt)? else {
                println!();
                break;
            };

            if pending.is_empty() {
                match session.handle_meta(line.trim()) {
                    Some(ReplControl::Quit) => break,
                    Some(ReplControl::Continue) => {
                        if should_store_meta(line.trim()) {
                            editor.add_history(line.trim().to_string());
                        }
                        continue;
                    }
                    None => {}
                }
            }

            if line.trim().is_empty() && pending.is_empty() {
                continue;
            }

            pending.push_str(&line);
            if session::needs_more_input(&pending) {
                continue;
            }

            match session.eval(config, &pending) {
                Ok(output) => {
                    editor.add_history(pending.trim().to_string());
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
