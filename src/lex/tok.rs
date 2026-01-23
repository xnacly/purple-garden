#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type<'t> {
    Eof,
    BraceLeft,
    BraceRight,
    Plus,
    Minus,
    Asteriks,
    Slash,
    Equal,
    DoubleEqual,
    LessThan,
    GreaterThan,
    Exlaim,
    Question,
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
    TVoid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}
