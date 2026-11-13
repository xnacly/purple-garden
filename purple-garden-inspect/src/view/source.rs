//! Source highlighting with the compiler's own lexer.

use std::ops::Range;

use purple_garden_frontend::lex::{self, Lexer, Type};

/// The theme's syntax highlight name for a token.
fn token_name(t: &Type<'_>, next: Option<&Type<'_>>) -> Option<&'static str> {
    Some(match t {
        Type::S(_) => "string",
        Type::I(_) | Type::D(_) => "number",
        Type::Doc(_) => "comment.doc",
        Type::Ident(_) if next == Some(&Type::BraceLeft) => "function",
        Type::Ident(_) => "variable",
        Type::BraceLeft
        | Type::BraceRight
        | Type::BraketLeft
        | Type::BraketRight
        | Type::CurlyLeft
        | Type::CurlyRight => "punctuation.bracket",
        Type::Dot | Type::Colon => "punctuation.delimiter",
        Type::Plus
        | Type::Minus
        | Type::Asteriks
        | Type::Slash
        | Type::Percent
        | Type::Equal
        | Type::DoubleEqual
        | Type::LessThan
        | Type::GreaterThan
        | Type::Exclaim
        | Type::NotEqual
        | Type::Question => "operator",
        Type::Eof => return None,
        other => {
            let word = other.as_str();
            if lex::type_doc(word).is_some() {
                "type"
            } else if matches!(word, "true" | "false") {
                "boolean"
            } else {
                "keyword"
            }
        }
    })
}

/// Byte ranges of `source` and their highlight names, ascending and non-overlapping. Comments are
/// whatever the lexer skips starting with `#`.
#[must_use]
pub fn garden_tokens(source: &str) -> Vec<(Range<usize>, &'static str)> {
    let bytes = source.as_bytes();
    let mut lexer = Lexer::new(bytes);
    let mut tokens = Vec::new();
    loop {
        let token = lexer.one();
        if token.t == Type::Eof {
            break;
        }
        tokens.push(token);
    }

    let line_end = |from: usize| {
        bytes[from..]
            .iter()
            .position(|&b| b == b'\n')
            .map_or(bytes.len(), |i| from + i)
    };
    let mut out = Vec::with_capacity(tokens.len());
    let mut at = 0;
    let comments = |out: &mut Vec<(Range<usize>, &'static str)>, gap: Range<usize>| {
        let mut i = gap.start;
        while i < gap.end {
            if bytes[i] == b'#' {
                let end = line_end(i).min(gap.end);
                out.push((i..end, "comment"));
                i = end;
            } else {
                i += 1;
            }
        }
    };
    for (i, token) in tokens.iter().enumerate() {
        let start = token.start.min(bytes.len());
        let len = match token.t {
            // quotes included
            Type::S(s) => s.len() + 2,
            Type::Doc(_) => line_end(start) - start,
            t => t.as_str().len(),
        };
        let end = (start + len).min(bytes.len());
        if start < at {
            continue;
        }
        comments(&mut out, at..start);
        if let Some(name) = token_name(&token.t, tokens.get(i + 1).map(|t| &t.t)) {
            out.push((start..end, name));
        }
        at = end;
    }
    comments(&mut out, at..bytes.len());
    out
}

#[cfg(test)]
mod tests {
    use super::garden_tokens;

    fn names(src: &str) -> Vec<(&str, &'static str)> {
        garden_tokens(src)
            .into_iter()
            .map(|(r, n)| (&src[r], n))
            .collect()
    }

    #[test]
    fn tokens_cover_keywords_strings_numbers_and_comments() {
        let src = "import \"io\" # comment\nfn f(a:Int) Int { io.println(a + 1.5) }";
        let got = names(src);
        assert!(got.contains(&("import", "keyword")), "{got:?}");
        assert!(got.contains(&("\"io\"", "string")), "{got:?}");
        assert!(got.contains(&("# comment", "comment")), "{got:?}");
        assert!(got.contains(&("fn", "keyword")), "{got:?}");
        assert!(got.contains(&("f", "function")), "{got:?}");
        assert!(got.contains(&("Int", "type")), "{got:?}");
        assert!(got.contains(&("println", "function")), "{got:?}");
        assert!(got.contains(&("1.5", "number")), "{got:?}");
        assert!(got.contains(&("+", "operator")), "{got:?}");
    }

    #[test]
    fn tokens_ascend_without_overlap_and_stay_in_bounds() {
        for src in [
            "",
            "#",
            "# only a comment",
            "\"unterminated",
            "#! doc\nfn a() {}",
            "let x = 1 # trailing\n# next\nx",
            "ü = \"ä\" # ö",
        ] {
            let tokens = garden_tokens(src);
            let mut at = 0;
            for (r, _) in &tokens {
                assert!(
                    r.start >= at && r.start <= r.end && r.end <= src.len(),
                    "{src:?}: {tokens:?}"
                );
                assert!(
                    src.is_char_boundary(r.start) && src.is_char_boundary(r.end),
                    "{src:?}"
                );
                at = r.end;
            }
        }
    }

    #[test]
    fn doc_comments_run_to_the_end_of_their_line() {
        let got = names("#! adds one\nfn f() {}");
        assert_eq!(got[0], ("#! adds one", "comment.doc"));
    }
}
