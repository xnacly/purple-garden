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
    Dot,
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
    Import,
    True,
    False,
    Let,
    Fn,
    Match,
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
    /// Byte offset into the source where this token starts. Line/column
    /// numbers are computed lazily on the error path from this offset; see
    /// `PgError::render`.
    pub start: usize,
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
        self.start == other.start && self.t == other.t
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
            Type::Dot => ".",
            Type::Colon => ":",
            Type::BraketLeft => "[",
            Type::BraketRight => "]",
            Type::CurlyLeft => "{",
            Type::CurlyRight => "}",
            Type::S(s) => s,
            Type::D(d) => d,
            Type::I(i) => i,
            Type::Ident(i) => i,
            Type::Import => "import",
            Type::True => "true",
            Type::False => "false",
            Type::Let => "let",
            Type::Fn => "fn",
            Type::Match => "match",
            Type::As => "as",
            Type::Str => "str",
            Type::Int => "int",
            Type::Double => "double",
            Type::Bool => "bool",
            Type::Void => "void",
        }
    }
}
