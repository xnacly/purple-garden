use crate::lex::Token;

#[derive(Debug)]
pub enum InnerNode<'inner> {
    /// inner value is encoded in super::Node::token
    Atom,
    Ident,

    /// lhs +-*/ rhs
    ///
    /// kind is encoded in super::Node::token
    Bin {
        lhs: Box<Node<'inner>>,
        rhs: Box<Node<'inner>>,
    },

    /// [members]
    Array {
        members: Vec<Node<'inner>>,
    },

    /// { key: value }
    Object {
        pairs: Vec<(Node<'inner>, Node<'inner>)>,
    },

    /// let name = "a string for instance"
    ///
    /// name is encoded in super::Node::token
    Let {
        rhs: Box<Node<'inner>>,
    },

    /// fn square(a) { a * a }
    ///
    /// name is encoded in super::Node::token
    Fn {
        args: Vec<Node<'inner>>,
        body: Vec<Node<'inner>>,
    },

    /// match {
    ///     true && true { false }
    ///     5 == 6 { // impossible }
    ///     5 != 6 { // thats true }
    /// }
    Match {
        /// [(condition, body)]
        cases: Vec<(Node<'inner>, Node<'inner>)>,
        default: Option<Box<Node<'inner>>>,
    },

    /// square(25 5)
    ///
    /// name is encoded in super::Node::token
    Call {
        args: Vec<Node<'inner>>,
    },

    /// std::runtime::gc::cycle()
    Path {
        /// runtime, gc
        members: Vec<Node<'inner>>,
        /// cycle
        ///
        /// always Node::Call, I'd say :^)
        leaf: Box<Node<'inner>>,
    },
}

#[derive(Debug)]
pub struct Node<'node> {
    pub token: Token<'node>,
    pub inner: InnerNode<'node>,
}
