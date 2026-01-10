#[derive(Debug, Clone)]
pub enum Type<'t> {
    Eof,
    DelimitLeft,
    DelimitRight,
    Plus,
    Minus,
    Asteriks,
    Slash,
    Equal,
    LessThan,
    GreaterThan,
    Exlaim,
    DoubleColon,
    BraketLeft,
    BraketRight,
    CurlyLeft,
    CurlyRight,
    String(&'t str),
    Ident(&'t str),
    Double(&'t str),
    Integer(&'t str),
    True,
    False,
    Let,
    Fn,
    Match,
    Std,
    For,
}

#[derive(Debug)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}

pub struct Lexer<'l> {
    input: &'l [u8],
    pos: usize,
    line: usize,
    col: usize,
}
