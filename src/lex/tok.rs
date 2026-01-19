#[derive(Debug, Clone, PartialEq, Eq)]
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
    Colon,
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
    For,
}

#[derive(Debug, Clone)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}
