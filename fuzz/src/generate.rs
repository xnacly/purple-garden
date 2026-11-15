//! Generator of well-typed, terminating Purple Garden programs.
//!
//! Every program is a list of functions followed by top-level bindings and a final `Int`
//! checksum over all of them, so [`crate::check`] can compare a single value (or the anomaly
//! that stopped the program) between backends and optimisation levels.
//!
//! Termination: Purple Garden has no loops, and a function can only call itself or functions
//! declared before it, so every cycle in the call graph is a self call. Each function takes a
//! `fuel` parameter as its first argument, every call passes a strictly smaller fuel, and a
//! function whose body calls anything returns a call-free expression once its fuel is used up.
//! With at most [`MAX_CALLS`] calls in a body and a top-level fuel of at most [`MAX_FUEL`], a
//! top-level call runs at most `MAX_CALLS^MAX_FUEL` function bodies.
//!
//! Every choice comes from an [`Unstructured`], so libFuzzer mutates programs structurally and
//! the same input reproduces a program exactly. An exhausted `Unstructured` yields the
//! smallest choice everywhere, which keeps the program small instead of failing.

use arbitrary::Unstructured;

/// Upper bound of the fuel a top-level call starts with.
const MAX_FUEL: i64 = 4;
/// Upper bound of calls in one function body, including the ones in its arguments.
const MAX_CALLS: usize = 3;
const MAX_FUNCS: usize = 7;
const MAX_PARAMS: usize = 8;
const MAX_DEPTH: u32 = 3;
/// Expressions in one program, past it every expression is a leaf and blocks are just a tail.
const BUDGET: usize = 600;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Ty {
    Int,
    Double,
    Bool,
    Str,
    /// The one record shape of a program, see [`Gen::record`].
    Record,
    Array(Box<Ty>),
}

impl Ty {
    fn array(of: Ty) -> Ty {
        Ty::Array(Box::new(of))
    }
}

const INTS: &[i64] = &[
    0,
    1,
    -1,
    2,
    -2,
    3,
    7,
    -7,
    10,
    31,
    32,
    63,
    64,
    65,
    127,
    128,
    255,
    256,
    32767,
    -32768,
    32768,
    -32769,
    65535,
    65536,
    1 << 20,
    -(1 << 20),
    (1 << 20) - 1,
    i32::MAX as i64,
    i32::MIN as i64,
    i32::MAX as i64 + 1,
    i32::MIN as i64 - 1,
    u32::MAX as i64,
    1 << 32,
    1 << 62,
    i64::MAX,
    i64::MIN,
    i64::MIN + 1,
    i64::MAX - 1,
];

const DOUBLES: &[f64] = &[
    0.0,
    -0.0,
    0.5,
    -0.5,
    1.0,
    1.5,
    -2.5,
    3.0,
    100.25,
    1e10,
    9223372036854775807.0,
    9223372036854774784.0,
    -9223372036854775808.0,
    -9.3e18,
    1e300,
    -1e300,
    1e-300,
    5e-324,
    f64::MAX,
    f64::MIN_POSITIVE,
    f64::NAN,
    f64::INFINITY,
    f64::NEG_INFINITY,
];

/// Short and long strings around the 8 byte small string boundary, multi-byte utf-8 included.
const STRS: &[&str] = &[
    "",
    "a",
    "b",
    "ab",
    "alpha",
    "beta",
    "gamma",
    "delta",
    "garden",
    "12345678",
    "123456789",
    "\u{fc}",
    "\u{fc}\u{fc}",
    "x y",
    "purple garden language",
];

const SHIFTS: &[i64] = &[
    0,
    1,
    31,
    32,
    63,
    64,
    65,
    127,
    128,
    -1,
    -63,
    -64,
    i64::MIN,
    i64::MAX,
];

/// Renders `v` as a Purple Garden expression evaluating to `v`. Literals are unsigned, so
/// negative values are a negation and `i64::MIN` a subtraction.
#[must_use]
fn int_lit(v: i64) -> String {
    if v == i64::MIN {
        "(-9223372036854775807 - 1)".into()
    } else if v < 0 {
        format!("(-{})", v.unsigned_abs())
    } else {
        v.to_string()
    }
}

/// Renders `v` as a Purple Garden expression evaluating to `v` bit for bit, NaN aside: there
/// is one NaN, `0.0 / 0.0`.
#[must_use]
fn double_lit(v: f64) -> String {
    if v.is_nan() {
        return "(0.0 / 0.0)".into();
    }
    if v.is_infinite() {
        return if v > 0.0 {
            "(1.0 / 0.0)".into()
        } else {
            "((-1.0) / 0.0)".into()
        };
    }
    // Display never uses an exponent and is the shortest representation that round-trips.
    let mut s = format!("{}", v.abs());
    if !s.contains('.') {
        s.push_str(".0");
    }
    if v.is_sign_negative() {
        format!("(-{s})")
    } else {
        s
    }
}

/// `name(args)`, the arguments separated as by [`join`].
fn apply(name: &str, args: &[String]) -> String {
    format!("{name}({})", join(args))
}

/// Space separated list elements. Statements and list elements have no separator, so an element
/// ending in an identifier followed by one starting with `(` parses as a call: the identifier
/// gets an identity around it, `(x)` doesn't do, the parser unwraps it.
fn join(elems: &[String]) -> String {
    let mut out = String::new();
    for (i, elem) in elems.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let ident_end = elem.ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
            && !elem.starts_with(|c: char| c.is_ascii_digit());
        match elems.get(i + 1) {
            Some(next) if ident_end && next.starts_with('(') => {
                out.push_str(&format!("opt.or(opt.some({elem}) {elem})"));
            }
            _ => out.push_str(elem),
        }
    }
    out
}

#[derive(Debug)]
struct Sig {
    params: Vec<Ty>,
    ret: Ty,
}

struct Gen<'a, 'b> {
    u: &'a mut Unstructured<'b>,
    /// Field `f{i}` of the program's record has type `record[i]`.
    record: Vec<Ty>,
    /// Arrays have a fixed length per element type, so an index known to be in bounds can be
    /// generated without a length function.
    lens: Vec<(Ty, usize)>,
    funcs: Vec<Sig>,
    /// The function being generated, `None` at the top level.
    current: Option<usize>,
    scopes: Vec<Vec<(String, Ty)>>,
    vars: usize,
    calls: usize,
    budget: usize,
}

/// Generates a program from `u`, see the module documentation.
///
/// # Errors
///
/// Never in practice: exhausted input produces the smallest choices instead. The `Result`
/// matches `arbitrary`'s conventions so the generator composes with `Arbitrary` impls.
pub fn program(u: &mut Unstructured<'_>) -> arbitrary::Result<String> {
    let mut g = Gen {
        u,
        record: Vec::new(),
        lens: Vec::new(),
        funcs: Vec::new(),
        current: None,
        scopes: Vec::new(),
        vars: 0,
        calls: 0,
        budget: BUDGET,
    };
    Ok(g.program())
}

impl Gen<'_, '_> {
    /// Uniform in `0..=max`, 0 once the input is exhausted.
    fn n(&mut self, max: usize) -> usize {
        self.u.int_in_range(0..=max).unwrap_or(0)
    }

    /// True with `percent`% probability, false once the input is exhausted.
    fn p(&mut self, percent: u8) -> bool {
        !self.u.is_empty() && self.u.int_in_range(0..=99u8).unwrap_or(99) < percent
    }

    /// Takes `cost` from the budget, false if it doesn't have that much left.
    fn spend(&mut self, cost: usize) -> bool {
        if self.budget < cost {
            return false;
        }
        self.budget -= cost;
        true
    }

    fn pick<'s, T>(&mut self, from: &'s [T]) -> &'s T {
        &from[self.n(from.len() - 1)]
    }

    fn program(&mut self) -> String {
        let mut out = String::from("import (\"bit\" \"math\" \"opt\" \"str\" \"testing\")\n\n");

        let fields = 1 + self.n(3);
        for _ in 0..fields {
            let ty = self.scalar_or_int_array();
            self.record.push(ty);
        }
        for ty in [
            Ty::Int,
            Ty::Double,
            Ty::Bool,
            Ty::Str,
            Ty::Record,
            Ty::array(Ty::Int),
        ] {
            let len = 1 + self.n(7);
            self.lens.push((ty, len));
        }

        let funcs = self.n(MAX_FUNCS);
        for i in 0..funcs {
            out.push_str(&self.function(i));
            out.push('\n');
        }

        self.current = None;
        self.scopes = vec![Vec::new()];
        let lets = 1 + self.n(7);
        let mut tops = Vec::new();
        for _ in 0..lets {
            let ty = self.ty();
            let rhs = match self.call(&ty, MAX_DEPTH) {
                Some(call) if self.p(60) => call,
                _ => self.expr(&ty, MAX_DEPTH),
            };
            let name = self.bind(ty.clone());
            out.push_str(&format!("let {name} = {rhs}\n"));
            tops.push((name, ty));
        }
        if self.p(10) {
            let cond = self.expr(&Ty::Bool, 2);
            out.push_str(&format!("testing.assert({cond})\n"));
        }

        let mut checksum = String::from("0");
        for (name, ty) in &tops {
            checksum = format!("(({checksum} * 31) + {})", self.hash(name, ty));
        }
        out.push_str(&format!("let checksum = {checksum}\nchecksum\n"));
        out
    }

    /// `tail` as the last expression of a block. Statements have no separator, so a tail
    /// starting like a call's argument list, an index, a record or a binary operator would
    /// continue the statement before it: those go through a binding.
    fn tail(&mut self, ty: &Ty, tail: String, pad: &str) -> String {
        if tail.starts_with(['(', '[', '{', '-']) {
            let name = self.bind(ty.clone());
            format!("{pad}let {name} = {tail}\n{pad}{name}\n")
        } else {
            format!("{pad}{tail}\n")
        }
    }

    fn function(&mut self, index: usize) -> String {
        let params: Vec<Ty> = (0..self.n(MAX_PARAMS)).map(|_| self.ty()).collect();
        let ret = self.ty();
        self.current = Some(index);
        // registered first so the body may call it, with a smaller fuel
        self.funcs.push(Sig {
            params: params.clone(),
            ret: ret.clone(),
        });
        self.scopes = vec![
            params
                .iter()
                .enumerate()
                .map(|(i, ty)| (format!("p{i}"), ty.clone()))
                .collect(),
        ];
        self.calls = 0;

        let mut header = format!("fn f{index}(fuel:Int");
        for (i, ty) in params.iter().enumerate() {
            header.push_str(&format!(" p{i}:{}", self.render(ty)));
        }
        header.push_str(&format!(") {} {{\n", self.render(&ret)));

        let body = if self.p(15) {
            self.pressure(&ret)
        } else {
            self.block(&ret, MAX_DEPTH, 1)
        };
        if self.calls == 0 && self.p(60) {
            return format!("{header}{body}}}\n");
        }
        // out of fuel: a call-free value, the calls budget is spent so expr makes none
        self.calls = MAX_CALLS;
        let base = self.expr(&ret, 1);
        let guard = *self.pick(&["fuel < 1", "fuel <= 0", "1 > fuel", "0 >= fuel"]);
        format!(
            "{header}    match {{\n        {guard} {{ {base} }}\n        {{\n{body}        }}\n    }}\n}}\n"
        )
    }

    /// Many values live at once, more than any backend has registers: they spill, and calls
    /// in between make the allocator save them.
    fn pressure(&mut self, ret: &Ty) -> String {
        self.scopes.push(Vec::new());
        let mut out = String::new();
        let lets = 16 + self.n(32).min(self.budget / 4);
        let mut live = Vec::new();
        for _ in 0..lets {
            let ty = if self.p(70) { Ty::Int } else { Ty::Double };
            let rhs = self.expr(&ty, 1);
            let name = self.bind(ty.clone());
            out.push_str(&format!("            let {name} = {rhs}\n"));
            live.push((name, ty));
        }
        let mut sum = String::from("0");
        for (name, ty) in &live {
            sum = format!("({sum} + {})", self.hash(name, ty));
        }
        let name = self.bind(Ty::Int);
        out.push_str(&format!("            let {name} = {sum}\n"));
        let tail = self.expr(ret, 2);
        out.push_str(&self.tail(ret, tail, "            "));
        self.scopes.pop();
        out
    }

    fn block(&mut self, ty: &Ty, depth: u32, indent: usize) -> String {
        let pad = "    ".repeat(indent + 2);
        self.scopes.push(Vec::new());
        let mut out = String::new();
        for _ in 0..self.n(6) {
            if !self.spend(4) {
                break;
            }
            if self.p(5) {
                let cond = self.expr(&Ty::Bool, depth.min(2));
                out.push_str(&format!("{pad}testing.assert({cond})\n"));
                continue;
            }
            let t = self.ty();
            let rhs = if depth > 0 && self.p(15) && self.spend(40) {
                self.match_(&t, depth - 1, indent + 1)
            } else {
                self.expr(&t, depth)
            };
            let name = self.bind(t);
            out.push_str(&format!("{pad}let {name} = {rhs}\n"));
        }
        let tail = if depth > 0 && self.p(20) && self.spend(40) {
            self.match_(ty, depth - 1, indent + 1)
        } else if self.p(25) {
            // a call in tail position, the tailcall pass turns it into a jump
            self.call(ty, depth).unwrap_or_else(|| self.expr(ty, depth))
        } else {
            self.expr(ty, depth)
        };
        out.push_str(&self.tail(ty, tail, &pad));
        self.scopes.pop();
        out
    }

    fn match_(&mut self, ty: &Ty, depth: u32, indent: usize) -> String {
        let pad = "    ".repeat(indent + 2);
        let mut out = String::from("match {\n");
        let switch = self.switch_subject();
        let arms = if switch.is_some() {
            4 + self.n(4)
        } else {
            1 + self.n(3)
        };
        for _ in 0..arms {
            let cond = match &switch {
                Some((subject, Ty::Int)) => {
                    // mostly a dense range, the switch lowering's jump table
                    let key = if self.p(60) {
                        self.n(8) as i64 - 2
                    } else {
                        *self.pick(INTS)
                    };
                    format!("{subject} == {}", int_lit(key))
                }
                Some((subject, _)) => format!("{subject} == \"{}\"", self.pick(STRS)),
                // a record literal would open the arm's block
                None => match self.expr(&Ty::Bool, depth.min(2)) {
                    cond if cond.starts_with('{') => format!("({cond})"),
                    cond => cond,
                },
            };
            let body = self.block(ty, depth, indent + 1);
            out.push_str(&format!("{pad}    {cond} {{\n{body}{pad}    }}\n"));
        }
        let body = self.block(ty, depth, indent + 1);
        out.push_str(&format!("{pad}    {{\n{body}{pad}    }}\n{pad}}}"));
        out
    }

    /// An Int or Str variable to compare against several constants, the shape the switch
    /// passes fold into one lookup.
    fn switch_subject(&mut self) -> Option<(String, Ty)> {
        if !self.p(35) {
            return None;
        }
        let ty = if self.p(70) { Ty::Int } else { Ty::Str };
        self.var(&ty).map(|v| (v, ty))
    }

    fn bind(&mut self, ty: Ty) -> String {
        let name = format!("v{}", self.vars);
        self.vars += 1;
        self.scopes
            .last_mut()
            .expect("a scope is always open")
            .push((name.clone(), ty));
        name
    }

    fn var(&mut self, ty: &Ty) -> Option<String> {
        let candidates: Vec<&String> = self
            .scopes
            .iter()
            .flatten()
            .filter(|(_, t)| t == ty)
            .map(|(n, _)| n)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let i = self.u.int_in_range(0..=candidates.len() - 1).unwrap_or(0);
        Some(candidates[i].clone())
    }

    fn scalar(&mut self) -> Ty {
        self.pick(&[Ty::Int, Ty::Int, Ty::Double, Ty::Bool, Ty::Str])
            .clone()
    }

    fn scalar_or_int_array(&mut self) -> Ty {
        if self.p(15) {
            Ty::array(Ty::Int)
        } else {
            self.scalar()
        }
    }

    fn ty(&mut self) -> Ty {
        match self.n(9) {
            0..=5 => self.scalar(),
            6 => Ty::Record,
            7 => Ty::array(self.scalar()),
            8 => Ty::array(Ty::Record),
            _ => Ty::array(Ty::array(Ty::Int)),
        }
    }

    fn render(&self, ty: &Ty) -> String {
        match ty {
            Ty::Int => "Int".into(),
            Ty::Double => "Double".into(),
            Ty::Bool => "Bool".into(),
            Ty::Str => "Str".into(),
            Ty::Record => {
                let fields: Vec<String> = self
                    .record
                    .iter()
                    .enumerate()
                    .map(|(i, t)| format!("f{i}:{}", self.render(t)))
                    .collect();
                format!("Record<{}>", fields.join(" "))
            }
            Ty::Array(of) => format!("Array<{}>", self.render(of)),
        }
    }

    fn len(&self, of: &Ty) -> usize {
        self.lens
            .iter()
            .find(|(t, _)| t == of)
            .map(|(_, len)| *len)
            .expect("every element type has a length")
    }

    /// `ty` as an Int, mixing in as many of its bits as the language lets a program observe.
    fn hash(&self, v: &str, ty: &Ty) -> String {
        match ty {
            Ty::Int => v.into(),
            Ty::Bool => format!("({v} as Int)"),
            Ty::Double => format!("(({v} as Int) + (({v} * 1024.0) as Int))"),
            Ty::Str => format!("((str.len({v}) * 7) + opt.or(str.get({v} 0) (-1)))"),
            Ty::Record => {
                let mut sum = String::from("0");
                for (i, t) in self.record.iter().enumerate() {
                    sum = format!("({sum} + {})", self.hash(&format!("{v}.f{i}"), t));
                }
                sum
            }
            Ty::Array(of) => {
                let last = self.len(of) - 1;
                format!(
                    "({} + {})",
                    self.hash(&format!("{v}.[0]"), of),
                    self.hash(&format!("{v}.[{last}]"), of)
                )
            }
        }
    }

    fn literal(&mut self, ty: &Ty) -> String {
        match ty {
            Ty::Int => {
                let v = match self.n(3) {
                    0 => self.n(16) as i64,
                    1 => self.u.arbitrary::<i64>().unwrap_or(0),
                    _ => *self.pick(INTS),
                };
                int_lit(v)
            }
            Ty::Double => {
                let v = match self.n(3) {
                    0 => self.n(16) as f64 * 0.25,
                    1 => f64::from_bits(self.u.arbitrary::<u64>().unwrap_or(0)),
                    _ => *self.pick(DOUBLES),
                };
                double_lit(v)
            }
            Ty::Bool => if self.p(50) { "true" } else { "false" }.into(),
            Ty::Str => format!("\"{}\"", self.pick(STRS)),
            Ty::Record => {
                let fields: Vec<String> = self
                    .record
                    .clone()
                    .iter()
                    .enumerate()
                    .map(|(i, t)| format!("f{i}: {}", self.leaf(t)))
                    .collect();
                format!("{{{}}}", fields.join(" "))
            }
            Ty::Array(of) => {
                let elems: Vec<String> = (0..self.len(of)).map(|_| self.leaf(of)).collect();
                format!("[{}]", join(&elems))
            }
        }
    }

    fn leaf(&mut self, ty: &Ty) -> String {
        if self.p(70)
            && let Some(v) = self.var(ty)
        {
            return v;
        }
        self.literal(ty)
    }

    fn expr(&mut self, ty: &Ty, depth: u32) -> String {
        if depth == 0 || self.p(20) || !self.spend(1) {
            return self.leaf(ty);
        }
        let d = depth - 1;
        if self.p(20)
            && let Some(e) = self.generic(ty, depth)
        {
            return e;
        }
        match ty {
            Ty::Int => self.int(d),
            Ty::Double => self.double(d),
            Ty::Bool => self.bool(d),
            Ty::Str => self.str(d),
            Ty::Record | Ty::Array(_) => self.leaf(ty),
        }
    }

    /// Expressions any type has: an element, a field, a call, an option round trip.
    fn generic(&mut self, ty: &Ty, depth: u32) -> Option<String> {
        let d = depth - 1;
        match self.n(4) {
            0 if self.lens.iter().any(|(t, _)| t == ty) => {
                let arr = self.expr(&Ty::array(ty.clone()), d);
                let idx = self.index(ty, d);
                Some(format!("{arr}.[{idx}]"))
            }
            1 => {
                let fields: Vec<usize> = (0..self.record.len())
                    .filter(|&i| self.record[i] == *ty)
                    .collect();
                let field = *fields.get(self.n(fields.len().max(1) - 1))?;
                let rec = self.expr(&Ty::Record, d);
                Some(format!("{rec}.f{field}"))
            }
            2 => {
                let some = self.expr(ty, d);
                let default = self.expr(ty, d);
                Some(format!("opt.or(opt.some({some}) {default})"))
            }
            _ => self.call(ty, depth),
        }
    }

    /// An index into `Array<of>`, mostly in bounds by construction, sometimes anything.
    fn index(&mut self, of: &Ty, depth: u32) -> String {
        let len = self.len(of) as i64;
        match self.n(9) {
            0..=3 => self.n(len as usize - 1).to_string(),
            4 | 5 => format!("math.clamp({} 0 {})", self.expr(&Ty::Int, depth), len - 1),
            6 => format!(
                "math.min(math.abs({}) {})",
                self.expr(&Ty::Int, depth),
                len - 1
            ),
            7 => int_lit(*self.pick(&[-1, len, len + 1, i64::MIN, i64::MAX, 1 << 32])),
            _ => self.expr(&Ty::Int, depth),
        }
    }

    fn call(&mut self, ty: &Ty, depth: u32) -> Option<String> {
        if self.calls >= MAX_CALLS {
            return None;
        }
        let last = self.current.unwrap_or(self.funcs.len().saturating_sub(1));
        let candidates: Vec<usize> = (0..self.funcs.len().min(last + 1))
            .filter(|&i| self.funcs[i].ret == *ty)
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let f = candidates[self.n(candidates.len() - 1)];
        self.calls += 1;
        let fuel = if self.current.is_some() {
            if self.p(80) {
                "(fuel - 1)"
            } else {
                "(fuel - 2)"
            }
            .to_string()
        } else {
            self.n(MAX_FUEL as usize).to_string()
        };
        let d = depth.saturating_sub(1);
        let mut args = vec![fuel];
        for t in self.funcs[f].params.clone() {
            args.push(self.expr(&t, d));
        }
        Some(apply(&format!("f{f}"), &args))
    }

    fn int(&mut self, d: u32) -> String {
        match self.n(13) {
            0..=2 => {
                let op = *self.pick(&["+", "-", "*", "+", "-"]);
                format!(
                    "({} {op} {})",
                    self.expr(&Ty::Int, d),
                    self.expr(&Ty::Int, d)
                )
            }
            3 => {
                let op = *self.pick(&["/", "%"]);
                let rhs = if self.p(60) {
                    int_lit(*self.pick(&[
                        1,
                        -1,
                        2,
                        -2,
                        3,
                        4,
                        7,
                        8,
                        16,
                        1024,
                        -1024,
                        1 << 40,
                        i64::MIN,
                        i64::MAX,
                    ]))
                } else {
                    self.expr(&Ty::Int, d)
                };
                format!("({} {op} {rhs})", self.expr(&Ty::Int, d))
            }
            4 => format!("(-{})", self.expr(&Ty::Int, d)),
            5 => {
                if self.p(50) {
                    format!("({} as Int)", self.expr(&Ty::Double, d))
                } else {
                    format!("({} as Int)", self.expr(&Ty::Bool, d))
                }
            }
            6 | 7 => {
                let f = *self.pick(&[
                    "and", "or", "xor", "not", "shl", "shr", "ushr", "popcount", "ctz", "clz",
                ]);
                let a = self.expr(&Ty::Int, d);
                match f {
                    "not" | "popcount" | "ctz" | "clz" => format!("bit.{f}({a})"),
                    "shl" | "shr" | "ushr" => {
                        let by = if self.p(60) {
                            int_lit(*self.pick(SHIFTS))
                        } else {
                            self.expr(&Ty::Int, d)
                        };
                        apply(&format!("bit.{f}"), &[a, by])
                    }
                    _ => apply(&format!("bit.{f}"), &[a, self.expr(&Ty::Int, d)]),
                }
            }
            8 => {
                let a = self.expr(&Ty::Int, d);
                match self.n(4) {
                    0 => format!("math.abs({a})"),
                    1 => apply("math.min", &[a, self.expr(&Ty::Int, d)]),
                    2 => apply("math.max", &[a, self.expr(&Ty::Int, d)]),
                    3 => format!("math.round({a})"),
                    _ => {
                        let lo = self.expr(&Ty::Int, d);
                        let hi = self.expr(&Ty::Int, d);
                        apply("math.clamp", &[a, lo, hi])
                    }
                }
            }
            9 => {
                let gcd = apply(
                    "math.gcd",
                    &[self.expr(&Ty::Int, d), self.expr(&Ty::Int, d)],
                );
                format!("opt.or({gcd} {})", self.expr(&Ty::Int, d))
            }
            10 => format!("str.len({})", self.expr(&Ty::Str, d)),
            11 => {
                let get = apply("str.get", &[self.expr(&Ty::Str, d), self.expr(&Ty::Int, d)]);
                format!("opt.or({get} {})", self.expr(&Ty::Int, d))
            }
            12 => {
                // a nested array, two bounds checks
                let arr = self.expr(&Ty::array(Ty::array(Ty::Int)), d);
                let i = self.index(&Ty::array(Ty::Int), d);
                let j = self.index(&Ty::Int, d);
                format!("{arr}.[{i}].[{j}]")
            }
            _ => self.leaf(&Ty::Int),
        }
    }

    fn double(&mut self, d: u32) -> String {
        match self.n(7) {
            0..=2 => {
                let op = *self.pick(&["+", "-", "*", "/"]);
                format!(
                    "({} {op} {})",
                    self.expr(&Ty::Double, d),
                    self.expr(&Ty::Double, d)
                )
            }
            3 => format!("(-{})", self.expr(&Ty::Double, d)),
            4 => format!("({} as Double)", self.expr(&Ty::Int, d)),
            5 | 6 => {
                let a = self.expr(&Ty::Double, d);
                match self.n(5) {
                    0 => format!("math.abs({a})"),
                    1 => apply("math.min", &[a, self.expr(&Ty::Double, d)]),
                    2 => apply("math.max", &[a, self.expr(&Ty::Double, d)]),
                    3 => format!("math.round({a})"),
                    4 => format!("math.sqrt({a})"),
                    _ => {
                        let lo = self.expr(&Ty::Double, d);
                        let hi = self.expr(&Ty::Double, d);
                        apply("math.clamp", &[a, lo, hi])
                    }
                }
            }
            _ => self.leaf(&Ty::Double),
        }
    }

    fn bool(&mut self, d: u32) -> String {
        match self.n(8) {
            0 | 1 => {
                let op = *self.pick(&["==", "!=", "<", ">", "<=", ">="]);
                format!(
                    "({} {op} {})",
                    self.expr(&Ty::Int, d),
                    self.expr(&Ty::Int, d)
                )
            }
            2 => {
                let op = *self.pick(&["<", ">", "<=", ">="]);
                format!(
                    "({} {op} {})",
                    self.expr(&Ty::Double, d),
                    self.expr(&Ty::Double, d)
                )
            }
            3 => {
                let op = *self.pick(&["==", "!="]);
                let ty = if self.p(50) { Ty::Bool } else { Ty::Str };
                format!("({} {op} {})", self.expr(&ty, d), self.expr(&ty, d))
            }
            4 | 5 => {
                let op = *self.pick(&["&", "|"]);
                format!(
                    "({} {op} {})",
                    self.expr(&Ty::Bool, d),
                    self.expr(&Ty::Bool, d)
                )
            }
            6 => format!("({} as Bool)", self.expr(&Ty::Int, d)),
            7 => apply(
                "str.contains",
                &[self.expr(&Ty::Str, d), self.expr(&Ty::Str, d)],
            ),
            _ => self.leaf(&Ty::Bool),
        }
    }

    fn str(&mut self, d: u32) -> String {
        match self.n(6) {
            0 | 1 => apply(
                "str.concat",
                &[self.expr(&Ty::Str, d), self.expr(&Ty::Str, d)],
            ),
            2 => format!("str.from({})", self.expr(&Ty::Int, d)),
            3 => format!("str.from({})", self.expr(&Ty::Double, d)),
            4 => {
                let slice = apply(
                    "str.slice",
                    &[
                        self.expr(&Ty::Str, d),
                        self.expr(&Ty::Int, d),
                        self.expr(&Ty::Int, d),
                    ],
                );
                format!("opt.or({slice} {})", self.expr(&Ty::Str, d))
            }
            5 => {
                // Small counts, zero and -1, or extreme ones (negative, results past the 4 GiB
                // string limit) that have to fail cleanly. Never a count in between whose result
                // fits but takes gigabytes.
                let n = if self.p(80) {
                    apply(
                        "math.clamp",
                        &[self.expr(&Ty::Int, d), "(-1)".into(), "3".into()],
                    )
                } else {
                    int_lit(*self.pick(&[i64::MIN, i64::MAX, 1 << 62, 1 << 33]))
                };
                apply("str.repeat", &[self.expr(&Ty::Str, d), n])
            }
            _ => self.leaf(&Ty::Str),
        }
    }
}
