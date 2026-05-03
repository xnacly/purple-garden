use std::io::{self, IsTerminal, Read, Write};

use super::raw::RawMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditorAction {
    Left,
    Right,
    Up,
    Down,
    Backspace,
    Delete,
    CtrlC,
    CtrlD,
    Enter,
    Insert(char),
    Ignore,
}

#[derive(Default)]
struct EditorState {
    buffer: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    history_cursor: Option<usize>,
    draft: Vec<char>,
}

impl EditorState {
    fn apply(&mut self, action: EditorAction) -> Option<EditorSubmit> {
        match action {
            EditorAction::Left => self.cursor = self.cursor.saturating_sub(1),
            EditorAction::Right => self.cursor = (self.cursor + 1).min(self.buffer.len()),
            EditorAction::Up => self.history_prev(),
            EditorAction::Down => self.history_next(),
            EditorAction::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.buffer.remove(self.cursor);
                }
            }
            EditorAction::Delete => {
                if self.cursor < self.buffer.len() {
                    self.buffer.remove(self.cursor);
                }
            }
            EditorAction::CtrlC => {
                self.clear_buffer();
                return Some(EditorSubmit::Cancelled);
            }
            EditorAction::CtrlD if self.buffer.is_empty() => return Some(EditorSubmit::Eof),
            EditorAction::CtrlD => {}
            EditorAction::Enter => {
                let line = self.buffer.iter().collect::<String>();
                self.clear_buffer();
                return Some(EditorSubmit::Line(line));
            }
            EditorAction::Insert(c) => {
                self.buffer.insert(self.cursor, c);
                self.cursor += 1;
                self.history_cursor = None;
                self.draft.clear();
            }
            EditorAction::Ignore => {}
        }

        None
    }

    fn add_history(&mut self, entry: String) {
        if entry.trim().is_empty() {
            return;
        }

        if self.history.last() == Some(&entry) {
            return;
        }

        self.history.push(entry);
        self.history_cursor = None;
        self.draft.clear();
    }

    fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }

        let next = match self.history_cursor {
            Some(0) => 0,
            Some(idx) => idx - 1,
            None => {
                self.draft = self.buffer.clone();
                self.history.len() - 1
            }
        };

        self.history_cursor = Some(next);
        self.set_buffer(self.history[next].chars().collect());
    }

    fn history_next(&mut self) {
        let Some(idx) = self.history_cursor else {
            return;
        };

        if idx + 1 < self.history.len() {
            let next = idx + 1;
            self.history_cursor = Some(next);
            self.set_buffer(self.history[next].chars().collect());
        } else {
            self.history_cursor = None;
            let draft = std::mem::take(&mut self.draft);
            self.set_buffer(draft);
        }
    }

    fn clear_buffer(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.history_cursor = None;
        self.draft.clear();
    }

    fn set_buffer(&mut self, buffer: Vec<char>) {
        self.buffer = buffer;
        self.cursor = self.buffer.len();
    }

    fn line(&self) -> String {
        self.buffer.iter().collect()
    }
}

enum EditorSubmit {
    Line(String),
    Cancelled,
    Eof,
}

#[derive(Default)]
pub(super) struct LineEditor {
    state: EditorState,
}

impl LineEditor {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn read_line(&mut self, prompt: &str) -> io::Result<Option<String>> {
        if io::stdin().is_terminal() && io::stdout().is_terminal() {
            self.read_interactive(prompt)
        } else {
            self.read_fallback(prompt)
        }
    }

    pub(super) fn add_history(&mut self, entry: String) {
        self.state.add_history(entry);
    }

    fn read_fallback(&mut self, prompt: &str) -> io::Result<Option<String>> {
        print!("{prompt}");
        io::stdout().flush()?;

        let mut line = String::new();
        if io::stdin().read_line(&mut line)? == 0 {
            return Ok(None);
        }
        Ok(Some(line))
    }

    fn read_interactive(&mut self, prompt: &str) -> io::Result<Option<String>> {
        let _raw = RawMode::enter()?;
        self.state.clear_buffer();
        self.redraw(prompt)?;

        loop {
            let action = read_action()?;
            if let Some(submit) = self.state.apply(action) {
                match submit {
                    EditorSubmit::Line(line) => {
                        println!();
                        return Ok(Some(format!("{line}\n")));
                    }
                    EditorSubmit::Cancelled => {
                        println!();
                        return Ok(Some("\n".into()));
                    }
                    EditorSubmit::Eof => return Ok(None),
                }
            }
            self.redraw(prompt)?;
        }
    }

    fn redraw(&self, prompt: &str) -> io::Result<()> {
        let line = self.state.line();
        print!("\r{prompt}{line}\x1b[K");

        let line_len = line.chars().count();
        let right = line_len.saturating_sub(self.state.cursor);
        if right > 0 {
            print!("\x1b[{right}D");
        }

        io::stdout().flush()
    }
}

fn read_action() -> io::Result<EditorAction> {
    let mut byte = [0u8; 1];
    io::stdin().read_exact(&mut byte)?;

    Ok(match byte[0] {
        b'\n' | b'\r' => EditorAction::Enter,
        0x03 => EditorAction::CtrlC,
        0x04 => EditorAction::CtrlD,
        0x7f | 0x08 => EditorAction::Backspace,
        0x1b => read_escape_action()?,
        b if b.is_ascii_graphic() || b == b' ' => EditorAction::Insert(b as char),
        _ => EditorAction::Ignore,
    })
}

fn read_escape_action() -> io::Result<EditorAction> {
    let mut seq = [0u8; 2];
    if io::stdin().read_exact(&mut seq[..1]).is_err() {
        return Ok(EditorAction::Ignore);
    }
    if seq[0] != b'[' {
        return Ok(EditorAction::Ignore);
    }
    io::stdin().read_exact(&mut seq[1..2])?;

    Ok(match seq[1] {
        b'A' => EditorAction::Up,
        b'B' => EditorAction::Down,
        b'C' => EditorAction::Right,
        b'D' => EditorAction::Left,
        b'3' => {
            let mut tail = [0u8; 1];
            io::stdin().read_exact(&mut tail)?;
            if tail[0] == b'~' {
                EditorAction::Delete
            } else {
                EditorAction::Ignore
            }
        }
        _ => EditorAction::Ignore,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_at_cursor() {
        let mut editor = EditorState::default();
        editor.apply(EditorAction::Insert('a'));
        editor.apply(EditorAction::Insert('c'));
        editor.apply(EditorAction::Left);
        editor.apply(EditorAction::Insert('b'));

        assert_eq!(editor.line(), "abc");
        assert_eq!(editor.cursor, 2);
    }

    #[test]
    fn backspace_removes_before_cursor() {
        let mut editor = EditorState::default();
        editor.apply(EditorAction::Insert('a'));
        editor.apply(EditorAction::Insert('b'));
        editor.apply(EditorAction::Insert('c'));
        editor.apply(EditorAction::Left);
        editor.apply(EditorAction::Backspace);

        assert_eq!(editor.line(), "ac");
        assert_eq!(editor.cursor, 1);
    }

    #[test]
    fn left_right_stay_in_bounds() {
        let mut editor = EditorState::default();
        editor.apply(EditorAction::Left);
        assert_eq!(editor.cursor, 0);

        editor.apply(EditorAction::Insert('x'));
        editor.apply(EditorAction::Right);
        assert_eq!(editor.cursor, 1);
    }

    #[test]
    fn history_traverses_and_restores_draft() {
        let mut editor = EditorState::default();
        editor.add_history("let x = 1".into());
        editor.add_history("x + 2".into());
        editor.apply(EditorAction::Insert('d'));

        editor.apply(EditorAction::Up);
        assert_eq!(editor.line(), "x + 2");
        editor.apply(EditorAction::Up);
        assert_eq!(editor.line(), "let x = 1");
        editor.apply(EditorAction::Down);
        assert_eq!(editor.line(), "x + 2");
        editor.apply(EditorAction::Down);
        assert_eq!(editor.line(), "d");
    }

    #[test]
    fn history_ignores_consecutive_duplicates() {
        let mut editor = EditorState::default();
        editor.add_history("x + 1".into());
        editor.add_history("x + 1".into());

        assert_eq!(editor.history, vec!["x + 1"]);
    }
}
