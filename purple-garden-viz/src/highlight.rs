//! Syntax highlighting: the editor highlights with the compiler's own lexer, the listings with the
//! [`Class`]es the terminal dumps paint, both resolved through the theme's syntax colors.

use std::ops::Range;

use gpui_kit::{
    component::{
        ActiveTheme as _,
        input::{EditorState, FoldRange, InputEdit, InputHighlighter, Rope},
    },
    *,
};
use purple_garden_inspect::view::source::garden_tokens;
use purple_garden_shared::ansi::Class;

/// The editor's language name for purple garden.
pub const LANGUAGE: &str = "garden";

/// [`InputHighlighter`] lexing the whole document on every change: the lexer runs at hundreds of
/// MB/s, far below a frame for any source the editor holds.
#[derive(Default)]
pub struct GardenHighlighter {
    tokens: Vec<(Range<usize>, &'static str)>,
}

impl InputHighlighter for GardenHighlighter {
    fn language(&self) -> SharedString {
        LANGUAGE.into()
    }

    fn update(
        &mut self,
        _: Option<InputEdit>,
        text: &Rope,
        _: bool,
        _: &mut Window,
        _: &mut Context<EditorState>,
    ) {
        self.tokens = garden_tokens(&text.to_string());
    }

    fn styles(
        &self,
        range: &Range<usize>,
        resolver: &dyn gpui_kit::component::input::HighlightStyleResolver,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let mut out = Vec::new();
        let mut at = range.start;
        let first = self.tokens.partition_point(|(r, _)| r.end <= range.start);
        for (r, name) in &self.tokens[first..] {
            if r.start >= range.end {
                break;
            }
            let start = r.start.max(range.start);
            let end = r.end.min(range.end);
            if at < start {
                out.push((at..start, HighlightStyle::default()));
            }
            out.push((start..end, resolver.style(name).unwrap_or_default()));
            at = end;
        }
        if at < range.end {
            out.push((at..range.end, HighlightStyle::default()));
        }
        out
    }

    fn fold_ranges(&self, _: &Rope) -> Vec<FoldRange> {
        Vec::new()
    }
}

/// The theme's syntax style a listing [`Class`] is painted in.
#[must_use]
pub fn class_style(class: Class, cx: &App) -> HighlightStyle {
    let name = match class {
        Class::Addr | Class::Comment => "comment",
        Class::Mnem => "keyword",
        Class::Flow => "keyword",
        Class::Reg => "variable",
        Class::Imm => "number",
        Class::Label => "label",
        Class::Func => "function",
        Class::Block => "title",
        Class::Section => "title",
        Class::Type => "type",
        Class::Dead => "hint",
    };
    let mut style = cx.theme().highlight_theme.style(name).unwrap_or_default();
    if matches!(class, Class::Flow | Class::Block | Class::Func) {
        style.font_weight = Some(FontWeight::SEMIBOLD);
    }
    style
}

/// `highlights` with their classes resolved to styles.
#[must_use]
pub fn styled(
    highlights: &[(Range<usize>, Class)],
    cx: &App,
) -> Vec<(Range<usize>, HighlightStyle)> {
    highlights
        .iter()
        .map(|(range, class)| (range.clone(), class_style(*class, cx)))
        .collect()
}
