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

    RawString(&'t str),
    RawIdent(&'t str),
    RawDouble(&'t str),
    RawInteger(&'t str),

    // keywords
    True,
    False,
    Let,
    Fn,
    Match,
    For,

    // type keywords
    Str,
    Int,
    Double,
    Bool,
    Void,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}
