macro_rules! define_keywords {
    (
        keywords {
            $($keyword_variant:ident => {
                name: $keyword_name:literal,
                kind: $keyword_kind:literal,
                doc: $keyword_doc:expr
            }),+ $(,)?
        }
        types {
            $($type_variant:ident => {
                name: $type_name:literal,
                doc: $type_doc:expr
            }),+ $(,)?
        }
    ) => {
        #[derive(Debug, Clone, PartialEq, Eq, Copy)]
        pub enum Type<'t> {
            Eof,
            BraceLeft,
            BraceRight,
            Plus,
            Minus,
            Asteriks,
            Slash,
            Percent,
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
            /// Double
            D(&'t str),
            /// integer
            I(&'t str),
            /// literal identifier
            Ident(&'t str),
            /// documentation comment
            Doc(&'t str),

            $(
                $keyword_variant,
            )+
            $(
                $type_variant,
            )+
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct KeywordDoc {
            pub name: &'static str,
            pub kind: &'static str,
            pub doc: &'static str,
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct TypeDoc {
            pub name: &'static str,
            pub doc: &'static str,
        }

        pub const KEYWORD_DOCS: &[KeywordDoc] = &[
            $(
                KeywordDoc {
                    name: $keyword_name,
                    kind: $keyword_kind,
                    doc: $keyword_doc,
                },
            )+
        ];

        pub const TYPE_DOCS: &[TypeDoc] = &[
            $(
                TypeDoc {
                    name: $type_name,
                    doc: $type_doc,
                },
            )+
        ];

        #[must_use]
        pub fn keyword_doc(name: &str) -> Option<&'static KeywordDoc> {
            KEYWORD_DOCS.iter().find(|doc| doc.name == name)
        }

        #[must_use]
        pub fn type_doc(name: &str) -> Option<&'static TypeDoc> {
            TYPE_DOCS.iter().find(|doc| doc.name == name)
        }

        impl<'t> Type<'t> {
            #[must_use]
            pub fn from_keyword(inner: &str) -> Option<Self> {
                Some(match inner {
                    $(
                        $keyword_name => Type::$keyword_variant,
                    )+
                    $(
                        $type_name => Type::$type_variant,
                    )+
                    _ => return None,
                })
            }

            #[must_use]
            pub fn as_str(&self) -> &'t str {
                match self {
                    Type::Eof => "eof",
                    Type::BraceLeft => "(",
                    Type::BraceRight => ")",
                    Type::Plus => "+",
                    Type::Minus => "-",
                    Type::Asteriks => "*",
                    Type::Slash => "/",
                    Type::Percent => "%",
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
                    Type::Doc(doc) => doc,
                    Type::D(d) => d,
                    Type::I(i) | Type::Ident(i) => i,
                    $(
                        Type::$keyword_variant => $keyword_name,
                    )+
                    $(
                        Type::$type_variant => $type_name,
                    )+
                }
            }
        }
    };
}

define_keywords! {
    keywords {
        Import => {
            name: "import",
            kind: "keyword",
            doc: concat!(
                "Imports a standard library package into the current file.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "import \"testing\"\n",
                "\n",
                "import (\"testing\" \"strings\")\n",
                "let label = strings.concat(\"steps: \" strings.from(42))\n",
                "testing.assert(strings.contains(label \"steps\"))\n",
                "```"
            )
        },
        Extern => {
            name: "extern",
            kind: "keyword",
            doc: concat!(
                "Declares external package signatures for typechecking and tooling.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "extern \"counter\" {\n",
                "    fn new(value: Int) Foreign<Counter>\n",
                "}\n",
                "```"
            )
        },
        Let => {
            name: "let",
            kind: "keyword",
            doc: concat!(
                "Binds the result of an expression to a local name.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let answer = 42\n",
                "\n",
                "let base = 100\n",
                "let offset = 5\n",
                "base + offset\n",
                "```"
            )
        },
        Fn => {
            name: "fn",
            kind: "keyword",
            doc: concat!(
                "Declares a function with typed arguments, an optional return type, and a body.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "fn inc(x:Int) Int { x + 1 }\n",
                "\n",
                "fn factorial(n:Int acc:Int) Int {\n",
                "    match {\n",
                "        n == 0 { acc }\n",
                "        { factorial(n-1 n*acc) }\n",
                "    }\n",
                "}\n",
                "```"
            )
        },
        Match => {
            name: "match",
            kind: "keyword",
            doc: concat!(
                "Selects the first branch whose condition is true, or the default branch.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let ready = true\n",
                "match {\n",
                "    ready { 1 }\n",
                "    { 0 }\n",
                "}\n",
                "\n",
                "let n = -7\n",
                "let sign = match {\n",
                "    n > 0 { 1 }\n",
                "    n < 0 { -1 }\n",
                "    { 0 }\n",
                "}\n",
                "```"
            )
        },
        As => {
            name: "as",
            kind: "keyword",
            doc: concat!(
                "Casts an expression to another type when the conversion is supported.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let x = 10 as Double\n",
                "\n",
                "let pixel = 42\n",
                "let x = (pixel as Double) / 100.0 - 1.5\n",
                "```"
            )
        },
        True => {
            name: "true",
            kind: "literal",
            doc: concat!(
                "Boolean true literal.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let enabled = true\n",
                "\n",
                "let len = 12\n",
                "let non_empty = match {\n",
                "    len > 0 { true }\n",
                "    { false }\n",
                "}\n",
                "```"
            )
        },
        False => {
            name: "false",
            kind: "literal",
            doc: concat!(
                "Boolean false literal.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let done = false\n",
                "\n",
                "let i = 10\n",
                "let limit = 10\n",
                "let exhausted = match {\n",
                "    i < limit { false }\n",
                "    { true }\n",
                "}\n",
                "```"
            )
        },
    }
    types {
        Str => {
            name: "Str",
            doc: concat!(
                "A UTF-8 string value.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let name = \"garden\"\n",
                "\n",
                "import \"strings\"\n",
                "\n",
                "let name = \"garden\"\n",
                "let greeting = strings.concat(\"hello \" name)\n",
                "```"
            )
        },
        Int => {
            name: "Int",
            doc: concat!(
                "A signed 64-bit integer value.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let attempts = 3\n",
                "\n",
                "let n = 11\n",
                "let next = match {\n",
                "    n % 2 == 0 { n / 2 }\n",
                "    { n * 3 + 1 }\n",
                "}\n",
                "```"
            )
        },
        Double => {
            name: "Double",
            doc: concat!(
                "A 64-bit floating point value.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let scale = 0.5\n",
                "\n",
                "let x = -0.75\n",
                "let y = 0.25\n",
                "let magnitude2 = x*x + y*y\n",
                "```"
            )
        },
        Bool => {
            name: "Bool",
            doc: concat!(
                "A boolean value, either true or false.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let ready = true\n",
                "\n",
                "let i = 4\n",
                "let in_range = match {\n",
                "    i < 0 { false }\n",
                "    i > 10 { false }\n",
                "    { true }\n",
                "}\n",
                "```"
            )
        },
        Void => {
            name: "Void",
            doc: concat!(
                "The empty return type used by expressions that do not produce a value.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "fn noop() Void {}\n",
                "\n",
                "import \"io\"\n",
                "\n",
                "fn log(msg:Str) Void {\n",
                "    io.println(msg)\n",
                "}\n",
                "```"
            )
        },
        Option => {
            name: "Option",
            doc: concat!(
                "A type that can either contain a value or be empty.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "fn maybe_name() Option<Str> {}\n",
                "```"
            )
        },
        Array => {
            name: "Array",
            doc: concat!(
                "A sequence of values that all have the same type.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "let values = [1 2 3] as Array<Int>\n",
                "```"
            )
        },
        Foreign => {
            name: "Foreign",
            doc: concat!(
                "An opaque value owned by an embedded Rust package. Purple Garden can pass it back to the package that created it, but cannot inspect its fields.\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "extern \"counter\" {\n",
                "    fn new(value: Int) Foreign<Counter>\n",
                "    fn increment(counter: Foreign<Counter>) Int\n",
                "}\n",
                "\n",
                "import \"counter\"\n",
                "let c = counter.new(0)\n",
                "counter.increment(c)\n",
                "```"
            )
        },
        Record => {
            name: "Record",
            doc: concat!(
                "A type marrying multiple uniqely adressable fields with types into a single value\n\n",
                "## Examples:\n\n",
                "```garden\n",
                "import \"io\"\n",
                "\n",
                "#! inferred as Record<name: Str age: Int>\n",
                "let x = {\n",
                "    name: \"teo\"\n",
                "    age: 23\n",
                "} as Record<name: Str age: Int>\n",
                "\n",
                "io.println(x.name)\n",
                "io.println(x.age)\n",
                "```"
            )
        },
    }
}

#[derive(Debug, Clone, Eq)]
pub struct Token<'t> {
    /// Byte offset into the source where this token starts. Line/column
    /// numbers are computed lazily on the diagnostic render path.
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
