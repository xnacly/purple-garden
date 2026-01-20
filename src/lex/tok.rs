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
    DoubleEqual,
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

    // keywords
    True,
    False,
    Let,
    Fn,
    Match,
    For,

    // type keywords
    TStr,
    TInt,
    TDouble,
    TBool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}
