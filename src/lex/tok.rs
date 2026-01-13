#[derive(Debug, Clone)]
pub enum Type<'t> {
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

#[derive(Debug)]
pub struct Token<'t> {
    pub line: usize,
    pub col: usize,
    pub t: Type<'t>,
}
