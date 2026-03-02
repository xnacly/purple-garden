#[derive(Debug, Clone, PartialEq, Eq, Copy)]
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
    Exclaim,
    NotEqual,
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
    As,

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

impl<'t> Type<'t> {
    pub fn as_str(&self) -> &'t str {
        match self {
            Type::Eof => "eof",
            Type::BraceLeft => "(",
            Type::BraceRight => ")",
            Type::Plus => "+",
            Type::Minus => "-",
            Type::Asteriks => "*",
            Type::Slash => "/",
            Type::Equal => "=",
            Type::DoubleEqual => "==",
            Type::LessThan => "<",
            Type::GreaterThan => ">",
            Type::Exclaim => "!",
            Type::NotEqual => "!=",
            Type::Question => "?",
            Type::Colon => ":",
            Type::DoubleColon => "::",
            Type::BraketLeft => "[",
            Type::BraketRight => "]",
            Type::CurlyLeft => "{",
            Type::CurlyRight => "}",
            Type::S(s) => s,
            Type::D(d) => d,
            Type::I(i) => i,
            Type::Ident(i) => i,
            Type::True => "true",
            Type::False => "false",
            Type::Let => "let",
            Type::Fn => "fn",
            Type::Match => "match",
            Type::For => "for",
            Type::As => "as",
            Type::Str => "str",
            Type::Int => "int",
            Type::Double => "double",
            Type::Bool => "bool",
            Type::Void => "void",
        }
    }
}
