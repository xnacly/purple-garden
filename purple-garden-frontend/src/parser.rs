use crate::{
    ast::{Ast, ExternFn, Node, NodeId, TypeExpr, TypeExprId},
    diagnostic::Diagnostic,
    lex::{Lexer, Token, Type},
};

/// Parsing the token stream one token at a time into the abstract syntax tree, see
/// [ast.rs](./ast.rs) for documentation regarding each node and the way those should be parsed.
pub struct Parser<'p> {
    lex: Lexer<'p>,
    id: usize,
    cur: Token<'p>,
    ast: Ast<'p>,
    diagnostics: Vec<Diagnostic>,
    pending_docs: Vec<Token<'p>>,
}

#[derive(Debug)]
pub struct ParseOutput<'p> {
    pub ast: Option<Ast<'p>>,
    pub diagnostics: Vec<Diagnostic>,
}

impl<'p> Parser<'p> {
    #[must_use]
    pub fn new(mut lex: Lexer<'p>) -> Self {
        let cur = lex.one();
        let diagnostics = std::mem::take(&mut lex.diagnostics);
        Self {
            cur,
            lex,
            id: 0,
            ast: Ast::new(),
            diagnostics,
            pending_docs: Vec::new(),
        }
    }

    fn next_id(&mut self) -> usize {
        let id = self.id;
        self.id += 1;
        id
    }

    fn cur(&self) -> &Token<'p> {
        &self.cur
    }

    fn at_end(&self) -> bool {
        self.cur.t == Type::Eof
    }

    fn expect(&mut self, ty: Type) -> Result<(), Diagnostic> {
        if self.cur.t == ty {
            self.advance()?;
        } else {
            return Err(Diagnostic::at_token(
                format!(
                    "Expected `{:?}`, got {}({:?})",
                    ty,
                    self.cur.t.as_str(),
                    self.cur.t
                ),
                &self.cur,
            ));
        }

        Ok(())
    }

    fn expect_ident(&mut self) -> Result<Token<'p>, Diagnostic> {
        if let Type::Ident(_) = self.cur.t {
            let matched = self.cur.clone();
            self.advance()?;
            Ok(matched)
        } else {
            Err(Diagnostic::at_token(
                format!("Expected an identifier, got {:?}", self.cur.t),
                &self.cur,
            ))
        }
    }

    fn advance(&mut self) -> Result<(), Diagnostic> {
        self.cur = self.lex.one();
        self.diagnostics
            .extend(std::mem::take(&mut self.lex.diagnostics));
        Ok(())
    }

    fn push_node(&mut self, node: Node<'p>) -> NodeId {
        self.ast.push_node(node)
    }

    fn push_type(&mut self, ty: TypeExpr<'p>) -> TypeExprId {
        self.ast.push_type(ty)
    }

    /// program = prefix*
    pub fn parse(mut self) -> Result<Ast<'p>, Diagnostic> {
        self.parse_roots()?;
        Ok(self.ast)
    }

    fn parse_roots(&mut self) -> Result<(), Diagnostic> {
        while !self.at_end() {
            let node = self.parse_prefix()?;
            self.ast.roots.push(node);
        }
        Ok(())
    }

    fn parse_roots_collect(&mut self) {
        while !self.at_end() {
            match self.parse_prefix() {
                Ok(node) => self.ast.roots.push(node),
                Err(diagnostic) => {
                    self.diagnostics.push(diagnostic);
                    self.synchronize_root();
                }
            }
        }
    }

    fn is_root_boundary(ty: Type<'_>) -> bool {
        matches!(
            ty,
            Type::Let
                | Type::Fn
                | Type::Import
                | Type::Extern
                | Type::Match
                | Type::CurlyRight
                | Type::Eof
        )
    }

    fn synchronize_root(&mut self) {
        self.pending_docs.clear();
        while !Self::is_root_boundary(self.cur.t) {
            self.advance().expect("lexing is infallible");
        }
        if self.cur.t == Type::CurlyRight {
            self.advance().expect("lexing is infallible");
        }
    }

    /// Parse while collecting diagnostics. Recovery is intentionally shallow
    /// for now: a malformed root is dropped, then parsing resumes at the next
    /// obvious root boundary.
    pub fn parse_collect(mut self) -> ParseOutput<'p> {
        self.parse_roots_collect();
        self.diagnostics.extend(self.lex.into_diagnostics());
        ParseOutput {
            ast: Some(self.ast),
            diagnostics: self.diagnostics,
        }
    }

    /// prefix = import | let | fn | match | expr
    fn parse_prefix(&mut self) -> Result<NodeId, Diagnostic> {
        while matches!(self.cur().t, Type::Doc(_)) {
            self.pending_docs.push(self.cur().clone());
            self.advance()?;
        }

        match self.cur().t {
            Type::Import => self.parse_import(),
            Type::Let => self.parse_let(),
            Type::Fn => self.parse_fn(),
            Type::Extern => self.parse_extern(),
            Type::Eof if !self.pending_docs.is_empty() => Err(Diagnostic::at_token(
                "documentation is not attached to anything",
                self.pending_docs.last().expect("pending docs not empty"),
            )),
            _ if !self.pending_docs.is_empty() => Err(Diagnostic::at_token(
                "documentation can only be attached to `let`, `fn` or `extern`",
                self.pending_docs.first().expect("pending docs not empty"),
            )),
            Type::Match => self.parse_match(),
            _ => self.parse_expr(0),
        }
    }

    fn take_docs(&mut self) -> Vec<Token<'p>> {
        std::mem::take(&mut self.pending_docs)
    }

    /// import = "import" string | "import" "(" string* ")"
    fn parse_import(&mut self) -> Result<NodeId, Diagnostic> {
        let src = self.cur.clone();
        // skip Type::Import
        self.advance()?;

        // single package import:
        // import "io"
        if let Type::S(_) = self.cur().t {
            let pkgs = vec![self.cur().clone()];
            // skip pkg name
            self.advance()?;

            let id = self.next_id();
            return Ok(self.push_node(Node::Import { src, id, pkgs }));
        }

        // multiple package import:
        // import ("io" "runtime")

        self.expect(Type::BraceLeft)?;

        let mut pkgs = Vec::new();

        while !self.at_end() && self.cur().t != Type::BraceRight {
            let &Token { t: Type::S(_), .. } = self.cur() else {
                return Err(Diagnostic::at_token(
                    "Only strings are allowed as import paths",
                    &self.cur,
                ));
            };
            pkgs.push(self.cur().clone());
            self.advance()?;
        }

        self.expect(Type::BraceRight)?;
        let id = self.next_id();
        Ok(self.push_node(Node::Import { src, id, pkgs }))
    }

    /// let = "let" ident "=" prefix
    fn parse_let(&mut self) -> Result<NodeId, Diagnostic> {
        let docs = self.take_docs();
        self.expect(Type::Let)?;
        let name = self.expect_ident()?;
        self.expect(Type::Equal)?;
        let rhs = self.parse_prefix()?;
        let id = self.next_id();

        Ok(self.push_node(Node::Let {
            id,
            docs,
            name,
            rhs,
        }))
    }

    /// fn = "fn" ident "(" (ident ":" type)* ")" type? "{" prefix* "}"
    fn parse_fn(&mut self) -> Result<NodeId, Diagnostic> {
        let docs = self.take_docs();
        self.advance()?;
        let name = self.expect_ident()?;

        let args = self.parse_args()?;

        let return_type = if self.cur().t == Type::CurlyLeft {
            self.push_type(TypeExpr::Atom(Token {
                start: self.cur().start,
                t: Type::Void,
            }))
        } else {
            self.parse_type()?
        };

        let mut body = vec![];
        self.expect(Type::CurlyLeft)?;
        while !self.at_end() && self.cur().t != Type::CurlyRight {
            body.push(self.parse_prefix()?);
        }
        self.expect(Type::CurlyRight)?;

        Ok(self.push_node(Node::Fn {
            docs,
            name,
            args,
            return_type,
            body,
        }))
    }

    fn parse_args(&mut self) -> Result<Vec<(Token<'p>, TypeExprId)>, Diagnostic> {
        self.expect(Type::BraceLeft)?;
        let mut args = vec![];
        while !self.at_end() && self.cur().t != Type::BraceRight {
            let arg_name = self.expect_ident()?;
            self.expect(Type::Colon)?;
            let arg_type = self.parse_type()?;
            args.push((arg_name, arg_type));
        }
        self.expect(Type::BraceRight)?;
        Ok(args)
    }

    /// extern = "extern" string "{" (doc* "fn" ident args type?)* "}"
    fn parse_extern(&mut self) -> Result<NodeId, Diagnostic> {
        let docs = self.take_docs();
        let src = self.cur.clone();
        self.expect(Type::Extern)?;

        let name = if matches!(self.cur().t, Type::S(_)) {
            let name = self.cur().clone();
            self.advance()?;
            name
        } else {
            return Err(Diagnostic::at_token(
                "Expected a string package name after `extern`",
                self.cur(),
            ));
        };

        self.expect(Type::CurlyLeft)?;
        let mut fns = Vec::new();
        while !self.at_end() && self.cur().t != Type::CurlyRight {
            while matches!(self.cur().t, Type::Doc(_)) {
                self.pending_docs.push(self.cur().clone());
                self.advance()?;
            }

            if self.cur().t == Type::Extern {
                return Err(Diagnostic::at_token(
                    "nested extern declarations are not supported",
                    self.cur(),
                ));
            }

            let fun_docs = self.take_docs();
            self.expect(Type::Fn)?;
            let fun_name = self.expect_ident()?;
            let args = self.parse_args()?;
            let return_type = if self.cur().t == Type::CurlyRight || self.cur().t == Type::Fn {
                self.push_type(TypeExpr::Atom(Token {
                    start: self.cur().start,
                    t: Type::Void,
                }))
            } else {
                self.parse_type()?
            };
            fns.push(ExternFn {
                docs: fun_docs,
                name: fun_name,
                args,
                return_type,
            });
        }
        self.expect(Type::CurlyRight)?;

        Ok(self.push_node(Node::Extern {
            src,
            docs,
            name,
            fns,
        }))
    }

    /// match = "match" "{" (expr "{" prefix* "}")* "{" prefix* "}" "}"
    fn parse_match(&mut self) -> Result<NodeId, Diagnostic> {
        self.advance()?;
        let mut cases = vec![];
        let mut default = None;
        let tok = self.cur().clone();

        self.expect(Type::CurlyLeft)?;
        while !self.at_end() && self.cur().t != Type::CurlyRight {
            // default case
            if self.cur().t == Type::CurlyLeft {
                let default_token = self.cur().clone();
                self.expect(Type::CurlyLeft)?;
                let mut default_body = vec![];
                while !self.at_end() && self.cur().t != Type::CurlyRight {
                    default_body.push(self.parse_prefix()?);
                }
                self.expect(Type::CurlyRight)?;
                default = Some((default_token, default_body));
            } else {
                let condition_token = self.cur().clone();
                let condition = self.parse_expr(0)?;
                self.expect(Type::CurlyLeft)?;
                let mut body = vec![];
                while !self.at_end() && self.cur().t != Type::CurlyRight {
                    body.push(self.parse_prefix()?);
                }
                self.expect(Type::CurlyRight)?;
                cases.push(((condition_token, condition), body));
            }
        }
        self.expect(Type::CurlyRight)?;

        let Some(default) = default else {
            return Err(Diagnostic::at_token(
                "A match statement requires a default branch",
                &tok,
            ));
        };

        let id = self.next_id();
        Ok(self.push_node(Node::Match { id, cases, default }))
    }

    /// expr =
    ///     atom |
    ///     ident |
    ///     "(" expr ")" |
    ///     prefix-op expr |
    ///     expr postfix-op |
    ///     expr infix-op expr |
    ///     { ident ":" expr } |
    ///     "[" expr "]"
    fn parse_expr(&mut self, min_bp: u8) -> Result<NodeId, Diagnostic> {
        let mut lhs = match self.cur().t {
            Type::CurlyLeft => {
                let src = self.cur.clone();
                // skip Type::CurlyLeft
                self.advance()?;

                let mut fields = vec![];

                // <key>: <value>
                while !self.at_end() && self.cur().t != Type::CurlyRight {
                    let key = self.expect_ident()?;
                    self.expect(Type::Colon)?;
                    let value = self.parse_expr(0)?;
                    fields.push((key, value))
                }

                self.expect(Type::CurlyRight)?;
                let id = self.next_id();
                self.push_node(Node::Record { src, id, fields })
            }
            Type::BraketLeft => {
                let src = self.cur.clone();
                // skip Type::BraketLeft
                self.advance()?;

                let mut members = vec![];
                while !self.at_end() && self.cur().t != Type::BraketRight {
                    let member = self.parse_expr(0)?;
                    members.push(member);
                }

                self.expect(Type::BraketRight)?;

                let id = self.next_id();
                self.push_node(Node::Array { id, src, members })
            }
            Type::S(_) | Type::I(_) | Type::D(_) | Type::True | Type::False => {
                let raw = self.cur.clone();
                self.advance()?;
                let id = self.next_id();
                self.push_node(Node::Atom { raw, id })
            }
            Type::Ident(_) => {
                let first = self.cur.clone();
                self.advance()?;

                let id = self.next_id();
                self.push_node(Node::Ident { name: first, id })
            }
            Type::BraceLeft => {
                self.advance()?;
                let e = self.parse_expr(0)?;
                self.expect(Type::BraceRight)?;
                e
            }
            Type::Plus | Type::Minus => {
                let op = self.cur().clone();
                let rbp = Parser::prefix_binding_power(&self.cur().t);
                self.advance()?;
                let rhs = self.parse_expr(rbp)?;
                let id = self.next_id();
                self.push_node(Node::Unary { id, op, rhs })
            }
            _ => {
                return Err(Diagnostic::at_token(
                    format!("Expected expression, got {:?}", self.cur().t),
                    self.cur(),
                ));
            }
        };

        // postfix parsing loop
        loop {
            match self.cur().t {
                Type::Dot => {
                    self.advance()?;
                    let field = self.expect_ident()?;

                    let id = self.next_id();
                    lhs = self.push_node(Node::Field {
                        id,
                        target: lhs,
                        name: field,
                    });
                }

                Type::BraceLeft => {
                    if !Self::is_callable_target(self.ast.node(lhs)) {
                        break;
                    }

                    self.advance()?;
                    let mut args = vec![];

                    while !self.at_end() && self.cur().t != Type::BraceRight {
                        args.push(self.parse_prefix()?);
                    }

                    self.expect(Type::BraceRight)?;
                    let id = self.next_id();
                    lhs = self.push_node(Node::Call {
                        id,
                        target: lhs,
                        args,
                    })
                }
                _ => break,
            }
        }

        // infix parsing loop
        while let Type::Plus
        | Type::Minus
        | Type::Asteriks
        | Type::Slash
        | Type::Percent
        | Type::DoubleEqual
        | Type::As
        | Type::LessThan
        | Type::GreaterThan = self.cur().t
        {
            let op = self.cur().clone();

            if let Token { t: Type::As, .. } = op {
                self.advance()?;
                let ty = self.parse_type()?;
                let id = self.next_id();
                lhs = self.push_node(Node::Cast {
                    src: op,
                    id,
                    lhs,
                    rhs: ty,
                });
                continue;
            }

            if let Some((lbp, rbp)) = Parser::infix_binding_power(&op.t) {
                if lbp < min_bp {
                    break;
                }

                self.advance()?;

                let rhs = self.parse_expr(rbp)?;
                let id = self.next_id();
                lhs = self.push_node(Node::Bin { id, op, lhs, rhs });
            }
        }

        Ok(lhs)
    }

    fn is_callable_target(node: &Node<'p>) -> bool {
        matches!(node, Node::Ident { .. } | Node::Field { .. })
    }

    fn prefix_binding_power(op: &Type) -> u8 {
        // TODO: add !<expr>,?<expr>, the latter being the try operator
        match op {
            Type::Plus | Type::Minus => 5,
            _ => unreachable!(),
        }
    }

    fn infix_binding_power(op: &Type) -> Option<(u8, u8)> {
        Some(match op {
            Type::Plus | Type::Minus => (1, 2),
            Type::Asteriks | Type::Slash | Type::Percent => (3, 4),
            Type::DoubleEqual | Type::NotEqual | Type::GreaterThan | Type::LessThan => (0, 1),
            _ => return None,
        })
    }

    /// type = atom-type | "Foreign" "<" ident ">" | "Option" "<" type ">" | "Array" "<" type ">" |
    /// "Record" "<" (ident ":" type)* ">"
    fn parse_type(&mut self) -> Result<TypeExprId, Diagnostic> {
        let Token { t, .. } = self.cur();
        Ok(match t {
            Type::Str | Type::Int | Type::Bool | Type::Void | Type::Double => {
                let tt = self.push_type(TypeExpr::Atom(self.cur().clone()));
                self.advance()?;
                tt
            }
            Type::Foreign => {
                self.advance()?;
                self.expect(Type::LessThan)?;
                if self.cur().t == Type::GreaterThan {
                    return Err(Diagnostic::at_token(
                        "Expected a foreign type name after `Foreign<`",
                        &self.cur,
                    ));
                }
                let inner = self.expect_ident()?;
                self.expect(Type::GreaterThan)?;
                self.push_type(TypeExpr::Foreign(inner))
            }
            // Optionals: Option<type>
            Type::Option => {
                self.advance()?;
                self.expect(Type::LessThan)?;
                if self.cur().t == Type::GreaterThan {
                    return Err(Diagnostic::at_token(
                        "Expected a type after `Option<`",
                        &self.cur,
                    ));
                }
                let inner = self.parse_type()?;
                self.expect(Type::GreaterThan)?;
                self.push_type(TypeExpr::Option(inner))
            }
            Type::Array => {
                self.advance()?;
                self.expect(Type::LessThan)?;
                if self.cur().t == Type::GreaterThan {
                    return Err(Diagnostic::at_token(
                        "Expected a type after `Array<`",
                        &self.cur,
                    ));
                }
                let inner = self.parse_type()?;
                self.expect(Type::GreaterThan)?;
                self.push_type(TypeExpr::Array(inner))
            }
            Type::Record => {
                let src = self.cur().clone();
                self.advance()?;
                self.expect(Type::LessThan)?;
                let mut fields = vec![];
                while !self.at_end() && self.cur().t != Type::GreaterThan {
                    let field_name = self.expect_ident()?;
                    self.expect(Type::Colon)?;
                    let field_type = self.parse_type()?;
                    fields.push((field_name, field_type));
                }
                self.expect(Type::GreaterThan)?;
                self.push_type(TypeExpr::Record { src, fields })
            }
            _ => {
                return Err(Diagnostic::at_token(
                    "Bad type, expected one of: Str, Int, Double, Bool, Void, Foreign<name>, Option<type>, Array<type> or Record<field: type>",
                    self.cur(),
                ));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{ast::Node, lex::Lexer, parser::Parser};

    macro_rules! table_parse_types {
        ($group:ident,$(($name:ident,$input:literal,$expected:literal))*) => {
            mod $group {
                use super::*;

                $(
                    #[test]
                    fn $name() {
                        let l = Lexer::new($input.as_bytes());
                        let mut p = Parser::new(l);
                        let tt = p.parse_type().unwrap();
                        assert_eq!(p.ast.type_display(tt).to_string(), $expected);
                    }
                )*
            }
        };
    }

    table_parse_types! {
        parse_types_atom,
        (int,"Int","Int")
        (double,"Double", "Double")
        (str,"Str", "Str")
        (bool,"Bool", "Bool")
        (void,"Void", "Void")
    }

    table_parse_types! {
        parse_types_option,
        (int,"Option<Int>", "Option<Int>")
        (double,"Option<Double>", "Option<Double>")
        (str,"Option<Str>", "Option<Str>")
        (bool,"Option<Bool>", "Option<Bool>")
        (void,"Option<Void>", "Option<Void>")
        (double_wrapped,"Option<Option<Void>>", "Option<Option<Void>>")
    }

    table_parse_types! {
        parse_types_array,
        (int,"Array<Int>", "Array<Int>")
        (double,"Array<Double>", "Array<Double>")
        (str,"Array<Str>", "Array<Str>")
        (bool,"Array<Bool>", "Array<Bool>")
        (void,"Array<Void>", "Array<Void>")
        (double_wrapped,"Array<Array<Void>>", "Array<Array<Void>>")
    }

    table_parse_types! {
        parse_types_foreign,
        (counter, "Foreign<Counter>", "Foreign<Counter>")
    }

    table_parse_types! {
        parse_types_record,
        (empty, "Record<>", "Record<>")
        (fields, "Record<name: Str age: Int>", "Record<name: Str age: Int>")
        (nested, "Record<name: Str job: Record<title: Str since: Int>>", "Record<name: Str job: Record<title: Str since: Int>>")
        (generic_fields, "Record<names: Array<Str> maybe_age: Option<Int>>", "Record<names: Array<Str> maybe_age: Option<Int>>")
    }

    #[test]
    fn empty_foreign_has_specific_error() {
        let l = Lexer::new(b"Foreign<>");
        let mut p = Parser::new(l);
        let err = p.parse_type().unwrap_err();
        assert_eq!(err.message, "Expected a foreign type name after `Foreign<`");
    }

    #[test]
    fn empty_option_has_specific_error() {
        let l = Lexer::new(b"Option<>");
        let mut p = Parser::new(l);
        let err = p.parse_type().unwrap_err();
        assert_eq!(err.message, "Expected a type after `Option<`");
    }

    #[test]
    fn empty_array_has_specific_error() {
        let l = Lexer::new(b"Array<>");
        let mut p = Parser::new(l);
        let err = p.parse_type().unwrap_err();
        assert_eq!(err.message, "Expected a type after `Array<`");
    }

    macro_rules! table_roots {
        ($group:ident,$(($name:ident,$input:literal,$root_count:literal))*) => {
            mod $group {
                use super::*;

                $(
                    #[test]
                    fn $name() {
                        let l = Lexer::new($input.as_bytes());
                        let p = Parser::new(l);
                        let ast = p.parse().unwrap();
                        assert_eq!(ast.roots.len(), $root_count);
                        assert!(!ast.nodes.is_empty());
                    }
                )*
            }
        };
    }

    table_roots! {
        happy_path,
        (binding, "let variable_name = 5", 1)
        (function_with_explicit_void, "fn zero_args() {}", 1)
        (function, "fn implicit_void() {}", 1)
        (foreign_function, "fn new(value: Foreign<Counter>) Foreign<Counter> {}", 1)
        (record_function, "fn new_user(name: Str age: Int) Record<name: Str age: Int> { { name: name age: age } }", 1)
        (expression, "3+0.1415*5/27", 1)
    }

    #[test]
    fn parses_empty_record_literal() {
        let l = Lexer::new(b"{}");
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        assert_eq!(ast.roots.len(), 1);
        let Node::Record { fields, .. } = ast.node(ast.roots[0]) else {
            panic!("expected record root, got {:?}", ast.node(ast.roots[0]));
        };
        assert!(fields.is_empty());
    }

    #[test]
    fn parses_record_literal_fields() {
        let l = Lexer::new(br#"{ name: "teo" age: 23 }"#);
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        assert_eq!(ast.roots.len(), 1);
        let Node::Record { fields, .. } = ast.node(ast.roots[0]) else {
            panic!("expected record root, got {:?}", ast.node(ast.roots[0]));
        };

        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0.t, crate::lex::Type::Ident("name"));
        assert!(matches!(ast.node(fields[0].1), Node::Atom { .. }));
        assert_eq!(fields[1].0.t, crate::lex::Type::Ident("age"));
        assert!(matches!(ast.node(fields[1].1), Node::Atom { .. }));
    }

    #[test]
    fn parses_nested_record_literal() {
        let l = Lexer::new(br#"{ name: "teo" job: { title: "dev" since: 2024 } }"#);
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        let Node::Record { fields, .. } = ast.node(ast.roots[0]) else {
            panic!("expected record root, got {:?}", ast.node(ast.roots[0]));
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[1].0.t, crate::lex::Type::Ident("job"));

        let Node::Record {
            fields: nested_fields,
            ..
        } = ast.node(fields[1].1)
        else {
            panic!("expected nested record, got {:?}", ast.node(fields[1].1));
        };
        assert_eq!(nested_fields.len(), 2);
        assert_eq!(nested_fields[0].0.t, crate::lex::Type::Ident("title"));
        assert_eq!(nested_fields[1].0.t, crate::lex::Type::Ident("since"));
    }

    #[test]
    fn parses_empty_array_literal() {
        let l = Lexer::new(b"[]");
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        assert_eq!(ast.roots.len(), 1);
        let Node::Array { src, members, .. } = ast.node(ast.roots[0]) else {
            panic!("expected array root, got {:?}", ast.node(ast.roots[0]));
        };
        assert_eq!(src.t, crate::lex::Type::BraketLeft);
        assert!(members.is_empty());
    }

    #[test]
    fn parses_array_literal_members() {
        let l = Lexer::new(br#"[1 "two" true]"#);
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        assert_eq!(ast.roots.len(), 1);
        let Node::Array { members, .. } = ast.node(ast.roots[0]) else {
            panic!("expected array root, got {:?}", ast.node(ast.roots[0]));
        };

        assert_eq!(members.len(), 3);
        assert!(matches!(ast.node(members[0]), Node::Atom { .. }));
        assert!(matches!(ast.node(members[1]), Node::Atom { .. }));
        assert!(matches!(ast.node(members[2]), Node::Atom { .. }));
    }

    #[test]
    fn parses_nested_array_literal() {
        let l = Lexer::new(b"[1 [2 3] 4]");
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        let Node::Array { members, .. } = ast.node(ast.roots[0]) else {
            panic!("expected array root, got {:?}", ast.node(ast.roots[0]));
        };
        assert_eq!(members.len(), 3);

        let Node::Array {
            members: nested_members,
            ..
        } = ast.node(members[1])
        else {
            panic!("expected nested array, got {:?}", ast.node(members[1]));
        };
        assert_eq!(nested_members.len(), 2);
        assert!(matches!(ast.node(nested_members[0]), Node::Atom { .. }));
        assert!(matches!(ast.node(nested_members[1]), Node::Atom { .. }));
    }

    #[test]
    fn parses_record_field_access() {
        let l = Lexer::new(br#"{ name: "teo" age: 23 }.name"#);
        let p = Parser::new(l);
        let ast = p.parse().unwrap();

        let Node::Field { target, name, .. } = ast.node(ast.roots[0]) else {
            panic!(
                "expected field access root, got {:?}",
                ast.node(ast.roots[0])
            );
        };
        assert_eq!(name.t, crate::lex::Type::Ident("name"));
        assert!(matches!(ast.node(*target), Node::Record { .. }));
    }

    #[test]
    fn adjacent_parenthesized_arg_after_atom_is_not_postfix_call() {
        let l = Lexer::new(b"f(0.0 (1.0 + 2.0))");
        let p = Parser::new(l);
        let ast = p.parse().unwrap();
        let root = ast.roots[0];

        let Node::Call { target, args, .. } = ast.node(root) else {
            panic!("expected call root, got {:?}", ast.node(root));
        };
        assert_eq!(args.len(), 2);
        assert!(matches!(ast.node(*target), Node::Ident { .. }));
        assert!(matches!(ast.node(args[0]), Node::Atom { .. }));
        assert!(matches!(ast.node(args[1]), Node::Bin { .. }));
    }

    #[test]
    fn equal_in_expression_terminates() {
        let l = Lexer::new(b"(5 = 6)");
        let p = Parser::new(l);
        let result = p.parse();
        assert!(result.is_err(), "expected parse error, got: {result:?}");
    }

    #[test]
    fn parse_collect_keeps_valid_roots_after_lexer_error() {
        let l = Lexer::new(b"import 1.2.3\nlet b = 1");
        let out = Parser::new(l).parse_collect();
        let ast = out.ast.expect("parse_collect should keep recovered ast");

        assert_eq!(out.diagnostics.len(), 2);
        assert_eq!(out.diagnostics[0].message, "Invalid numeric literal");
        assert_eq!(
            out.diagnostics[1].message,
            "Expected `BraceLeft`, got let(Let)"
        );
        assert_eq!(ast.roots.len(), 1);
        let Node::Let { name, .. } = ast.node(ast.roots[0]) else {
            panic!("expected recovered let root");
        };
        assert_eq!(name.t, crate::lex::Type::Ident("b"));
    }

    #[test]
    fn parse_collect_keeps_valid_roots_after_parser_error() {
        let l = Lexer::new(b"let a = 1 +\nlet b = 2");
        let out = Parser::new(l).parse_collect();
        let ast = out.ast.expect("parse_collect should keep recovered ast");

        assert_eq!(out.diagnostics.len(), 1);
        assert_eq!(out.diagnostics[0].message, "Expected expression, got Let");
        assert_eq!(ast.roots.len(), 1);
        let Node::Let { name, .. } = ast.node(ast.roots[0]) else {
            panic!("expected recovered let root");
        };
        assert_eq!(name.t, crate::lex::Type::Ident("b"));
    }
}
