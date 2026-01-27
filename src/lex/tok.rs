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

    /// compile time known string
    S(&'t str),
    /// double
    D(&'t str),
    /// integer
    I(&'t str),
    /// literal identifier
    Ident(&'t str),

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

#[derive(Debug, Clone, Eq)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}

#[cfg(test)]
impl PartialEq for Token<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.t == other.t
    }
}

#[cfg(not(test))]
impl PartialEq for Token<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.line == other.line && self.col == other.col && self.t == other.t
    }
}
