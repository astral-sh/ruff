//! Generate Python source code from an abstract syntax tree (AST).

use std::ops::Deref;

use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, ConversionFlag, Excepthandler,
    ExcepthandlerKind, Expr, ExprKind, Operator, Stmt, StmtKind, Suite, Withitem,
};

use crate::source_code::stylist::{Indentation, LineEnding, Quote, Stylist};
use crate::vendor::{bytes, str};

mod precedence {
    macro_rules! precedence {
        ($($op:ident,)*) => {
            precedence!(@0, $($op,)*);
        };
        (@$i:expr, $op1:ident, $($op:ident,)*) => {
            pub const $op1: u8 = $i;
            precedence!(@$i + 1, $($op,)*);
        };
        (@$i:expr,) => {};
    }
    precedence!(
        TUPLE, TEST, OR, AND, NOT, CMP, // "EXPR" =
        BOR, BXOR, BAND, SHIFT, ARITH, TERM, FACTOR, POWER, AWAIT, ATOM,
    );
    pub const EXPR: u8 = BOR;
}

pub struct Generator<'a> {
    /// The indentation style to use.
    indent: &'a Indentation,
    /// The quote style to use for string literals.
    quote: &'a Quote,
    /// The line ending to use.
    line_ending: &'a LineEnding,
    buffer: String,
    indent_depth: usize,
    num_newlines: usize,
    initial: bool,
}

impl<'a> From<&'a Stylist<'a>> for Generator<'a> {
    fn from(stylist: &'a Stylist<'a>) -> Self {
        Self {
            indent: stylist.indentation(),
            quote: stylist.quote(),
            line_ending: stylist.line_ending(),
            buffer: String::new(),
            indent_depth: 0,
            num_newlines: 0,
            initial: true,
        }
    }
}

impl<'a> Generator<'a> {
    pub const fn new(
        indent: &'a Indentation,
        quote: &'a Quote,
        line_ending: &'a LineEnding,
    ) -> Self {
        Self {
            // Style preferences.
            indent,
            quote,
            line_ending,
            // Internal state.
            buffer: String::new(),
            indent_depth: 0,
            num_newlines: 0,
            initial: true,
        }
    }

    pub fn generate(self) -> String {
        self.buffer
    }

    fn newline(&mut self) {
        if !self.initial {
            self.num_newlines = std::cmp::max(self.num_newlines, 1);
        }
    }

    fn newlines(&mut self, extra: usize) {
        if !self.initial {
            self.num_newlines = std::cmp::max(self.num_newlines, 1 + extra);
        }
    }

    fn body<U>(&mut self, stmts: &[Stmt<U>]) {
        self.indent_depth += 1;
        for stmt in stmts {
            self.unparse_stmt(stmt);
        }
        self.indent_depth -= 1;
    }

    fn p(&mut self, s: &str) {
        if self.num_newlines > 0 {
            for _ in 0..self.num_newlines {
                self.buffer += self.line_ending;
            }
            self.num_newlines = 0;
        }
        self.buffer += s;
    }

    fn p_if(&mut self, cond: bool, s: &str) {
        if cond {
            self.p(s);
        }
    }

    fn p_delim(&mut self, first: &mut bool, s: &str) {
        self.p_if(!std::mem::take(first), s);
    }

    pub fn unparse_suite<U>(&mut self, suite: &Suite<U>) {
        for stmt in suite {
            self.unparse_stmt(stmt);
        }
    }

    pub fn unparse_stmt<U>(&mut self, ast: &Stmt<U>) {
        macro_rules! statement {
            ($body:block) => {{
                self.newline();
                self.p(&self.indent.deref().repeat(self.indent_depth));
                $body
                self.initial = false;
            }};
        }

        match &ast.node {
            StmtKind::FunctionDef {
                name,
                args,
                body,
                returns,
                decorator_list,
                ..
            } => {
                self.newlines(if self.indent_depth == 0 { 2 } else { 1 });
                statement!({
                    for decorator in decorator_list {
                        statement!({
                            self.p("@");
                            self.unparse_expr(decorator, precedence::EXPR);
                        });
                    }
                    self.newline();
                    self.p("def ");
                    self.p(name);
                    self.p("(");
                    self.unparse_args(args);
                    self.p(")");
                    if let Some(returns) = returns {
                        self.p(" -> ");
                        self.unparse_expr(returns, precedence::TEST);
                    }
                    self.p(":");
                });
                self.body(body);
                if self.indent_depth == 0 {
                    self.newlines(2);
                }
            }
            StmtKind::AsyncFunctionDef {
                name,
                args,
                body,
                returns,
                decorator_list,
                ..
            } => {
                self.newlines(if self.indent_depth == 0 { 2 } else { 1 });
                statement!({
                    for decorator in decorator_list {
                        statement!({
                            self.unparse_expr(decorator, precedence::EXPR);
                        });
                    }
                    self.newline();
                    self.p("async def ");
                    self.p(name);
                    self.p("(");
                    self.unparse_args(args);
                    self.p(")");
                    if let Some(returns) = returns {
                        self.p(" -> ");
                        self.unparse_expr(returns, precedence::TEST);
                    }
                    self.p(":");
                });
                self.body(body);
                if self.indent_depth == 0 {
                    self.newlines(2);
                }
            }
            StmtKind::ClassDef {
                name,
                bases,
                keywords,
                body,
                decorator_list,
                ..
            } => {
                self.newlines(if self.indent_depth == 0 { 2 } else { 1 });
                statement!({
                    for decorator in decorator_list {
                        statement!({
                            self.unparse_expr(decorator, precedence::EXPR);
                        });
                    }
                    self.newline();
                    self.p("class ");
                    self.p(name);
                    let mut first = true;
                    for base in bases {
                        self.p_if(first, "(");
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(base, precedence::EXPR);
                    }
                    for keyword in keywords {
                        self.p_if(first, "(");
                        self.p_delim(&mut first, ", ");
                        if let Some(arg) = &keyword.node.arg {
                            self.p(arg);
                            self.p("=");
                        } else {
                            self.p("**");
                        }
                        self.unparse_expr(&keyword.node.value, precedence::EXPR);
                    }
                    self.p_if(!first, ")");
                    self.p(":");
                });
                self.body(body);
                if self.indent_depth == 0 {
                    self.newlines(2);
                }
            }
            StmtKind::Return { value } => {
                statement!({
                    if let Some(expr) = value {
                        self.p("return ");
                        self.unparse_expr(expr, precedence::TUPLE);
                    } else {
                        self.p("return");
                    }
                });
            }
            StmtKind::Delete { targets } => {
                statement!({
                    self.p("del ");
                    let mut first = true;
                    for expr in targets {
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(expr, precedence::TEST);
                    }
                });
            }
            StmtKind::Assign { targets, value, .. } => {
                statement!({
                    for target in targets {
                        self.unparse_expr(target, precedence::TUPLE);
                        self.p(" = ");
                    }
                    self.unparse_expr(value, precedence::TUPLE);
                });
            }
            StmtKind::AugAssign { target, op, value } => {
                statement!({
                    self.unparse_expr(target, precedence::TUPLE);
                    self.p(" ");
                    self.p(match op {
                        Operator::Add => "+",
                        Operator::Sub => "-",
                        Operator::Mult => "*",
                        Operator::MatMult => "@",
                        Operator::Div => "/",
                        Operator::Mod => "%",
                        Operator::Pow => "**",
                        Operator::LShift => "<<",
                        Operator::RShift => ">>",
                        Operator::BitOr => "|",
                        Operator::BitXor => "^",
                        Operator::BitAnd => "&",
                        Operator::FloorDiv => "//",
                    });
                    self.p("= ");
                    self.unparse_expr(value, precedence::TUPLE);
                });
            }
            StmtKind::AnnAssign {
                target,
                annotation,
                value,
                simple,
            } => {
                statement!({
                    let need_parens = matches!(target.node, ExprKind::Name { .. }) && simple == &0;
                    self.p_if(need_parens, "(");
                    self.unparse_expr(target, precedence::TUPLE);
                    self.p_if(need_parens, ")");
                    self.p(": ");
                    self.unparse_expr(annotation, precedence::TEST);
                    if let Some(value) = value {
                        self.p(" = ");
                        self.unparse_expr(value, precedence::TUPLE);
                    }
                });
            }
            StmtKind::For {
                target,
                iter,
                body,
                orelse,
                ..
            } => {
                statement!({
                    self.p("for ");
                    self.unparse_expr(target, precedence::TUPLE);
                    self.p(" in ");
                    self.unparse_expr(iter, precedence::TUPLE);
                    self.p(":");
                });
                self.body(body);
                if !orelse.is_empty() {
                    statement!({
                        self.p("else:");
                    });
                    self.body(orelse);
                }
            }
            StmtKind::AsyncFor {
                target,
                iter,
                body,
                orelse,
                ..
            } => {
                statement!({
                    self.p("async for ");
                    self.unparse_expr(target, precedence::TUPLE);
                    self.p(" in ");
                    self.unparse_expr(iter, precedence::TUPLE);
                    self.p(":");
                });
                self.body(body);
                if !orelse.is_empty() {
                    statement!({
                        self.p("else:");
                    });
                    self.body(orelse);
                }
            }
            StmtKind::While { test, body, orelse } => {
                statement!({
                    self.p("while ");
                    self.unparse_expr(test, precedence::TUPLE);
                    self.p(":");
                });
                self.body(body);
                if !orelse.is_empty() {
                    statement!({
                        self.p("else:");
                    });
                    self.body(orelse);
                }
            }
            StmtKind::If { test, body, orelse } => {
                statement!({
                    self.p("if ");
                    self.unparse_expr(test, precedence::TUPLE);
                    self.p(":");
                });
                self.body(body);

                let mut orelse_: &Vec<Stmt<U>> = orelse;
                loop {
                    if orelse_.len() == 1 && matches!(orelse_[0].node, StmtKind::If { .. }) {
                        if let StmtKind::If { body, test, orelse } = &orelse_[0].node {
                            statement!({
                                self.p("elif ");
                                self.unparse_expr(test, precedence::TUPLE);
                                self.p(":");
                            });
                            self.body(body);
                            orelse_ = orelse;
                        }
                    } else {
                        if !orelse_.is_empty() {
                            statement!({
                                self.p("else:");
                            });
                            self.body(orelse_);
                        }
                        break;
                    }
                }
            }
            StmtKind::With { items, body, .. } => {
                statement!({
                    self.p("with ");
                    let mut first = true;
                    for item in items {
                        self.p_delim(&mut first, ", ");
                        self.unparse_withitem(item);
                    }
                    self.p(":");
                });
                self.body(body);
            }
            StmtKind::AsyncWith { items, body, .. } => {
                statement!({
                    self.p("async with ");
                    let mut first = true;
                    for item in items {
                        self.p_delim(&mut first, ", ");
                        self.unparse_withitem(item);
                    }
                    self.p(":");
                });
                self.body(body);
            }
            StmtKind::Match { .. } => {}
            StmtKind::Raise { exc, cause } => {
                statement!({
                    self.p("raise");
                    if let Some(exc) = exc {
                        self.p(" ");
                        self.unparse_expr(exc, precedence::TEST);
                    }
                    if let Some(cause) = cause {
                        self.p(" from ");
                        self.unparse_expr(cause, precedence::TEST);
                    }
                });
            }
            StmtKind::Try {
                body,
                handlers,
                orelse,
                finalbody,
            } => {
                statement!({
                    self.p("try:");
                });
                self.body(body);

                for handler in handlers {
                    statement!({
                        self.unparse_excepthandler(handler);
                    });
                }

                if !orelse.is_empty() {
                    statement!({
                        self.p("else:");
                    });
                    self.body(orelse);
                }
                if !finalbody.is_empty() {
                    statement!({
                        self.p("finally:");
                    });
                    self.body(finalbody);
                }
            }
            StmtKind::Assert { test, msg } => {
                statement!({
                    self.p("assert ");
                    self.unparse_expr(test, precedence::TEST);
                    if let Some(msg) = msg {
                        self.p(", ");
                        self.unparse_expr(msg, precedence::TEST);
                    }
                });
            }
            StmtKind::Import { names } => {
                statement!({
                    self.p("import ");
                    let mut first = true;
                    for alias in names {
                        self.p_delim(&mut first, ", ");
                        self.unparse_alias(alias);
                    }
                });
            }
            StmtKind::ImportFrom {
                module,
                names,
                level,
            } => {
                statement!({
                    self.p("from ");
                    if let Some(level) = level {
                        self.p(&".".repeat(*level));
                    }
                    if let Some(module) = module {
                        self.p(module);
                    }
                    self.p(" import ");
                    let mut first = true;
                    for alias in names {
                        self.p_delim(&mut first, ", ");
                        self.unparse_alias(alias);
                    }
                });
            }
            StmtKind::Global { names } => {
                statement!({
                    self.p("global ");
                    let mut first = true;
                    for name in names {
                        self.p_delim(&mut first, ", ");
                        self.p(name);
                    }
                });
            }
            StmtKind::Nonlocal { names } => {
                statement!({
                    self.p("nonlocal ");
                    let mut first = true;
                    for name in names {
                        self.p_delim(&mut first, ", ");
                        self.p(name);
                    }
                });
            }
            StmtKind::Expr { value } => {
                statement!({
                    self.unparse_expr(value, 0);
                });
            }
            StmtKind::Pass => {
                statement!({
                    self.p("pass");
                });
            }
            StmtKind::Break => {
                statement!({
                    self.p("break");
                });
            }
            StmtKind::Continue => {
                statement!({
                    self.p("continue");
                });
            }
        }
    }

    fn unparse_excepthandler<U>(&mut self, ast: &Excepthandler<U>) {
        match &ast.node {
            ExcepthandlerKind::ExceptHandler { type_, name, body } => {
                self.p("except");
                if let Some(type_) = type_ {
                    self.p(" ");
                    self.unparse_expr(type_, precedence::EXPR);
                }
                if let Some(name) = name {
                    self.p(" as ");
                    self.p(name);
                }
                self.p(":");
                self.body(body);
            }
        }
    }

    pub fn unparse_expr<U>(&mut self, ast: &Expr<U>, level: u8) {
        macro_rules! opprec {
            ($opty:ident, $x:expr, $enu:path, $($var:ident($op:literal, $prec:ident)),*$(,)?) => {
                match $x {
                    $(<$enu>::$var => (opprec!(@space $opty, $op), precedence::$prec),)*
                }
            };
            (@space bin, $op:literal) => {
                concat!(" ", $op, " ")
            };
            (@space un, $op:literal) => {
                $op
            };
        }
        macro_rules! group_if {
            ($lvl:expr, $body:block) => {{
                let group = level > $lvl;
                self.p_if(group, "(");
                let ret = $body;
                self.p_if(group, ")");
                ret
            }};
        }
        match &ast.node {
            ExprKind::BoolOp { op, values } => {
                let (op, prec) = opprec!(bin, op, Boolop, And("and", AND), Or("or", OR));
                group_if!(prec, {
                    let mut first = true;
                    for val in values {
                        self.p_delim(&mut first, op);
                        self.unparse_expr(val, prec + 1);
                    }
                });
            }
            ExprKind::NamedExpr { target, value } => {
                group_if!(precedence::TUPLE, {
                    self.unparse_expr(target, precedence::ATOM);
                    self.p(" := ");
                    self.unparse_expr(value, precedence::ATOM);
                });
            }
            ExprKind::BinOp { left, op, right } => {
                let rassoc = matches!(op, Operator::Pow);
                let (op, prec) = opprec!(
                    bin,
                    op,
                    Operator,
                    Add("+", ARITH),
                    Sub("-", ARITH),
                    Mult("*", TERM),
                    MatMult("@", TERM),
                    Div("/", TERM),
                    Mod("%", TERM),
                    Pow("**", POWER),
                    LShift("<<", SHIFT),
                    RShift(">>", SHIFT),
                    BitOr("|", BOR),
                    BitXor("^", BXOR),
                    BitAnd("&", BAND),
                    FloorDiv("//", TERM),
                );
                group_if!(prec, {
                    self.unparse_expr(left, prec + u8::from(rassoc));
                    self.p(op);
                    self.unparse_expr(right, prec + u8::from(!rassoc));
                });
            }
            ExprKind::UnaryOp { op, operand } => {
                let (op, prec) = opprec!(
                    un,
                    op,
                    rustpython_parser::ast::Unaryop,
                    Invert("~", FACTOR),
                    Not("not ", NOT),
                    UAdd("+", FACTOR),
                    USub("-", FACTOR)
                );
                group_if!(prec, {
                    self.p(op);
                    self.unparse_expr(operand, prec);
                });
            }
            ExprKind::Lambda { args, body } => {
                group_if!(precedence::TEST, {
                    let npos = args.args.len() + args.posonlyargs.len();
                    self.p(if npos > 0 { "lambda " } else { "lambda" });
                    self.unparse_args(args);
                    self.p(": ");
                    self.unparse_expr(body, precedence::TEST);
                });
            }
            ExprKind::IfExp { test, body, orelse } => {
                group_if!(precedence::TEST, {
                    self.unparse_expr(body, precedence::TEST + 1);
                    self.p(" if ");
                    self.unparse_expr(test, precedence::TEST + 1);
                    self.p(" else ");
                    self.unparse_expr(orelse, precedence::TEST);
                });
            }
            ExprKind::Dict { keys, values } => {
                self.p("{");
                let mut first = true;
                for (k, v) in keys.iter().zip(values) {
                    self.p_delim(&mut first, ", ");
                    if let Some(k) = k {
                        self.unparse_expr(k, precedence::TEST);
                        self.p(": ");
                        self.unparse_expr(v, precedence::TEST);
                    } else {
                        self.p("**");
                        self.unparse_expr(v, precedence::EXPR);
                    }
                }
                self.p("}");
            }
            ExprKind::Set { elts } => {
                if elts.is_empty() {
                    self.p("set()");
                } else {
                    self.p("{");
                    let mut first = true;
                    for v in elts {
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(v, precedence::TEST);
                    }
                    self.p("}");
                }
            }
            ExprKind::ListComp { elt, generators } => {
                self.p("[");
                self.unparse_expr(elt, precedence::TEST);
                self.unparse_comp(generators);
                self.p("]");
            }
            ExprKind::SetComp { elt, generators } => {
                self.p("{");
                self.unparse_expr(elt, precedence::TEST);
                self.unparse_comp(generators);
                self.p("}");
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.p("{");
                self.unparse_expr(key, precedence::TEST);
                self.p(": ");
                self.unparse_expr(value, precedence::TEST);
                self.unparse_comp(generators);
                self.p("}");
            }
            ExprKind::GeneratorExp { elt, generators } => {
                self.p("(");
                self.unparse_expr(elt, precedence::TEST);
                self.unparse_comp(generators);
                self.p(")");
            }
            ExprKind::Await { value } => {
                group_if!(precedence::AWAIT, {
                    self.p("await ");
                    self.unparse_expr(value, precedence::ATOM);
                });
            }
            ExprKind::Yield { value } => {
                group_if!(precedence::AWAIT, {
                    self.p("yield");
                    if let Some(value) = value {
                        self.p(" ");
                        self.unparse_expr(value, precedence::ATOM);
                    }
                });
            }
            ExprKind::YieldFrom { value } => {
                group_if!(precedence::AWAIT, {
                    self.p("yield from ");
                    self.unparse_expr(value, precedence::ATOM);
                });
            }
            ExprKind::Compare {
                left,
                ops,
                comparators,
            } => {
                group_if!(precedence::CMP, {
                    let new_lvl = precedence::CMP + 1;
                    self.unparse_expr(left, new_lvl);
                    for (op, cmp) in ops.iter().zip(comparators) {
                        let op = match op {
                            Cmpop::Eq => " == ",
                            Cmpop::NotEq => " != ",
                            Cmpop::Lt => " < ",
                            Cmpop::LtE => " <= ",
                            Cmpop::Gt => " > ",
                            Cmpop::GtE => " >= ",
                            Cmpop::Is => " is ",
                            Cmpop::IsNot => " is not ",
                            Cmpop::In => " in ",
                            Cmpop::NotIn => " not in ",
                        };
                        self.p(op);
                        self.unparse_expr(cmp, new_lvl);
                    }
                });
            }
            ExprKind::Call {
                func,
                args,
                keywords,
            } => {
                self.unparse_expr(func, precedence::ATOM);
                self.p("(");
                if let (
                    [Expr {
                        node: ExprKind::GeneratorExp { elt, generators },
                        ..
                    }],
                    [],
                ) = (&**args, &**keywords)
                {
                    // make sure a single genexp doesn't get double parens
                    self.unparse_expr(elt, precedence::TEST);
                    self.unparse_comp(generators);
                } else {
                    let mut first = true;
                    for arg in args {
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(arg, precedence::TEST);
                    }
                    for kw in keywords {
                        self.p_delim(&mut first, ", ");
                        if let Some(arg) = &kw.node.arg {
                            self.p(arg);
                            self.p("=");
                            self.unparse_expr(&kw.node.value, precedence::TEST);
                        } else {
                            self.p("**");
                            self.unparse_expr(&kw.node.value, precedence::EXPR);
                        }
                    }
                }
                self.p(")");
            }
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => self.unparse_formatted(value, *conversion, format_spec.as_deref()),
            ExprKind::JoinedStr { values } => self.unparse_joinedstr(values, false),
            ExprKind::Constant { value, kind } => {
                if let Some(kind) = kind {
                    self.p(kind);
                }
                self.unparse_constant(value);
            }
            ExprKind::Attribute { value, attr, .. } => {
                if let ExprKind::Constant {
                    value: Constant::Int(_),
                    ..
                } = &value.node
                {
                    self.p("(");
                    self.unparse_expr(value, precedence::ATOM);
                    self.p(").");
                } else {
                    self.unparse_expr(value, precedence::ATOM);
                    self.p(".");
                };
                self.p(attr);
            }
            ExprKind::Subscript { value, slice, .. } => {
                self.unparse_expr(value, precedence::ATOM);
                let mut lvl = precedence::TUPLE;
                if let ExprKind::Tuple { elts, .. } = &slice.node {
                    if elts
                        .iter()
                        .any(|expr| matches!(expr.node, ExprKind::Starred { .. }))
                    {
                        lvl += 1;
                    }
                }
                self.p("[");
                self.unparse_expr(slice, lvl);
                self.p("]");
            }
            ExprKind::Starred { value, .. } => {
                self.p("*");
                self.unparse_expr(value, precedence::EXPR);
            }
            ExprKind::Name { id, .. } => self.p(id),
            ExprKind::List { elts, .. } => {
                self.p("[");
                let mut first = true;
                for elt in elts {
                    self.p_delim(&mut first, ", ");
                    self.unparse_expr(elt, precedence::TEST);
                }
                self.p("]");
            }
            ExprKind::Tuple { elts, .. } => {
                if elts.is_empty() {
                    self.p("()");
                } else {
                    group_if!(precedence::TUPLE, {
                        let mut first = true;
                        for elt in elts {
                            self.p_delim(&mut first, ", ");
                            self.unparse_expr(elt, precedence::TEST);
                        }
                        self.p_if(elts.len() == 1, ",");
                    });
                }
            }
            ExprKind::Slice { lower, upper, step } => {
                if let Some(lower) = lower {
                    self.unparse_expr(lower, precedence::TEST);
                }
                self.p(":");
                if let Some(upper) = upper {
                    self.unparse_expr(upper, precedence::TEST);
                }
                if let Some(step) = step {
                    self.p(":");
                    self.unparse_expr(step, precedence::TEST);
                }
            }
        }
    }

    pub fn unparse_constant(&mut self, constant: &Constant) {
        assert_eq!(f64::MAX_10_EXP, 308);
        let inf_str = "1e309";
        match constant {
            Constant::Bytes(b) => {
                self.p(&bytes::repr(b, self.quote.into()));
            }
            Constant::Str(s) => {
                self.p(&format!("{}", str::repr(s, self.quote.into())));
            }
            Constant::None => self.p("None"),
            Constant::Bool(b) => self.p(if *b { "True" } else { "False" }),
            Constant::Int(i) => self.p(&format!("{}", i)),
            Constant::Tuple(tup) => {
                if let [elt] = &**tup {
                    self.p("(");
                    self.unparse_constant(elt);
                    self.p(",");
                    self.p(")");
                } else {
                    self.p("(");
                    for (i, elt) in tup.iter().enumerate() {
                        if i != 0 {
                            self.p(", ");
                        }
                        self.unparse_constant(elt);
                    }
                    self.p(")");
                }
            }
            Constant::Float(fp) => {
                if fp.is_infinite() {
                    self.p(inf_str);
                } else {
                    self.p(&rustpython_common::float_ops::to_string(*fp));
                }
            }
            Constant::Complex { real, imag } => {
                let value = if *real == 0.0 {
                    format!("{imag}j")
                } else {
                    format!("({real}{imag:+}j)")
                };
                if real.is_infinite() || imag.is_infinite() {
                    self.p(&value.replace("inf", inf_str));
                } else {
                    self.p(&value);
                }
            }
            Constant::Ellipsis => self.p("..."),
        }
    }

    fn unparse_args<U>(&mut self, args: &Arguments<U>) {
        let mut first = true;
        let defaults_start = args.posonlyargs.len() + args.args.len() - args.defaults.len();
        for (i, arg) in args.posonlyargs.iter().chain(&args.args).enumerate() {
            self.p_delim(&mut first, ", ");
            self.unparse_arg(arg);
            if let Some(i) = i.checked_sub(defaults_start) {
                self.p("=");
                self.unparse_expr(&args.defaults[i], precedence::TEST);
            }
            self.p_if(i + 1 == args.posonlyargs.len(), ", /");
        }
        if args.vararg.is_some() || !args.kwonlyargs.is_empty() {
            self.p_delim(&mut first, ", ");
            self.p("*");
        }
        if let Some(vararg) = &args.vararg {
            self.unparse_arg(vararg);
        }
        let defaults_start = args.kwonlyargs.len() - args.kw_defaults.len();
        for (i, kwarg) in args.kwonlyargs.iter().enumerate() {
            self.p_delim(&mut first, ", ");
            self.unparse_arg(kwarg);
            if let Some(default) = i
                .checked_sub(defaults_start)
                .and_then(|i| args.kw_defaults.get(i))
            {
                self.p("=");
                self.unparse_expr(default, precedence::TEST);
            }
        }
        if let Some(kwarg) = &args.kwarg {
            self.p_delim(&mut first, ", ");
            self.p("**");
            self.unparse_arg(kwarg);
        }
    }

    fn unparse_arg<U>(&mut self, arg: &Arg<U>) {
        self.p(&arg.node.arg);
        if let Some(ann) = &arg.node.annotation {
            self.p(": ");
            self.unparse_expr(ann, precedence::TEST);
        }
    }

    fn unparse_comp<U>(&mut self, generators: &[Comprehension<U>]) {
        for comp in generators {
            self.p(if comp.is_async > 0 {
                " async for "
            } else {
                " for "
            });
            self.unparse_expr(&comp.target, precedence::TUPLE);
            self.p(" in ");
            self.unparse_expr(&comp.iter, precedence::TEST + 1);
            for cond in &comp.ifs {
                self.p(" if ");
                self.unparse_expr(cond, precedence::TEST + 1);
            }
        }
    }

    fn unparse_fstring_body<U>(&mut self, values: &[Expr<U>], is_spec: bool) {
        for value in values {
            self.unparse_fstring_elem(value, is_spec);
        }
    }

    fn unparse_formatted<U>(&mut self, val: &Expr<U>, conversion: usize, spec: Option<&Expr<U>>) {
        let mut generator = Generator::new(self.indent, self.quote, self.line_ending);
        generator.unparse_expr(val, precedence::TEST + 1);
        let brace = if generator.buffer.starts_with('{') {
            // put a space to avoid escaping the bracket
            "{ "
        } else {
            "{"
        };
        self.p(brace);
        self.buffer += &generator.buffer;

        if conversion != ConversionFlag::None as usize {
            self.p("!");
            #[allow(clippy::cast_possible_truncation)]
            self.p(&format!("{}", conversion as u8 as char));
        }

        if let Some(spec) = spec {
            self.p(":");
            self.unparse_fstring_elem(spec, true);
        }

        self.p("}");
    }

    fn unparse_fstring_elem<U>(&mut self, expr: &Expr<U>, is_spec: bool) {
        match &expr.node {
            ExprKind::Constant { value, .. } => {
                if let Constant::Str(s) = value {
                    self.unparse_fstring_str(s);
                } else {
                    unreachable!()
                }
            }
            ExprKind::JoinedStr { values } => self.unparse_joinedstr(values, is_spec),
            ExprKind::FormattedValue {
                value,
                conversion,
                format_spec,
            } => self.unparse_formatted(value, *conversion, format_spec.as_deref()),
            _ => unreachable!(),
        }
    }

    fn unparse_fstring_str(&mut self, s: &str) {
        let s = s.replace('{', "{{").replace('}', "}}");
        self.p(&s);
    }

    fn unparse_joinedstr<U>(&mut self, values: &[Expr<U>], is_spec: bool) {
        if is_spec {
            self.unparse_fstring_body(values, is_spec);
        } else {
            self.p("f");
            let mut generator = Generator::new(
                self.indent,
                match self.quote {
                    Quote::Single => &Quote::Double,
                    Quote::Double => &Quote::Single,
                },
                self.line_ending,
            );
            generator.unparse_fstring_body(values, is_spec);
            let body = &generator.buffer;
            self.p(&format!("{}", str::repr(body, self.quote.into())));
        }
    }

    fn unparse_alias<U>(&mut self, alias: &Alias<U>) {
        self.p(&alias.node.name);
        if let Some(asname) = &alias.node.asname {
            self.p(" as ");
            self.p(asname);
        }
    }

    fn unparse_withitem<U>(&mut self, withitem: &Withitem<U>) {
        self.unparse_expr(&withitem.context_expr, precedence::EXPR);
        if let Some(optional_vars) = &withitem.optional_vars {
            self.p(" as ");
            self.unparse_expr(optional_vars, precedence::EXPR);
        }
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser::parser;

    use crate::source_code::stylist::{Indentation, LineEnding, Quote};
    use crate::source_code::Generator;

    fn round_trip(contents: &str) -> String {
        let indentation = Indentation::default();
        let quote = Quote::default();
        let line_ending = LineEnding::default();
        let program = parser::parse_program(contents, "<filename>").unwrap();
        let stmt = program.first().unwrap();
        let mut generator = Generator::new(&indentation, &quote, &line_ending);
        generator.unparse_stmt(stmt);
        generator.generate()
    }

    fn round_trip_with(
        indentation: &Indentation,
        quote: &Quote,
        line_ending: &LineEnding,
        contents: &str,
    ) -> String {
        let program = parser::parse_program(contents, "<filename>").unwrap();
        let stmt = program.first().unwrap();
        let mut generator = Generator::new(indentation, quote, line_ending);
        generator.unparse_stmt(stmt);
        generator.generate()
    }

    macro_rules! assert_round_trip {
        ($contents:expr) => {
            assert_eq!(
                round_trip($contents),
                $contents.replace('\n', LineEnding::default().as_str())
            );
        };
    }

    #[test]
    fn unparse() {
        assert_round_trip!("x.foo");
        assert_round_trip!("(5).foo");
        assert_round_trip!("a @ b");
        assert_round_trip!("a @= b");
        assert_round_trip!("[1, 2, 3]");
        assert_round_trip!("foo(1)");
        assert_round_trip!("foo(1, 2)");
        assert_round_trip!("foo(x for x in y)");
        assert_round_trip!("x = yield 1");
        assert_round_trip!("lambda: (1, 2, 3)");
        assert_round_trip!("return 3 and 4");
        assert_round_trip!("return 3 or 4");
        assert_round_trip!("yield from some()");
        assert_round_trip!(r#"assert (1, 2, 3), "msg""#);
        assert_round_trip!("import ast");
        assert_round_trip!("import operator as op");
        assert_round_trip!("from math import floor");
        assert_round_trip!("from .. import foobar");
        assert_round_trip!("from ..aaa import foo, bar as bar2");
        assert_round_trip!(r#"return f"functools.{qualname}({', '.join(args)})""#);
        assert_round_trip!(r#"my_function(*[1], *[2], **{"three": 3}, **{"four": "four"})"#);
        assert_round_trip!(r#"our_dict = {"a": 1, **{"b": 2, "c": 3}}"#);
        assert_round_trip!("f(**x)");
        assert_round_trip!("{**x}");
        assert_round_trip!("f(**([] or 5))");
        assert_round_trip!("{**([] or 5)}");
        assert_round_trip!(
            r#"def f() -> (int, int):
    pass"#
        );
        assert_round_trip!(
            r#"def test(a, b, /, c, *, d, **kwargs):
    pass"#
        );
        assert_round_trip!(
            r#"def test(a=3, b=4, /, c=7):
    pass"#
        );
        assert_round_trip!(
            r#"def test(a, b=4, /, c=8, d=9):
    pass"#
        );
        assert_round_trip!(
            r#"def call(*popenargs, timeout=None, **kwargs):
    pass"#
        );
        assert_round_trip!(
            r#"@functools.lru_cache(maxsize=None)
def f(x: int, y: int) -> int:
    return x + y"#
        );
        assert_eq!(round_trip(r#"x = (1, 2, 3)"#), r#"x = 1, 2, 3"#);
        assert_eq!(round_trip(r#"-(1) + ~(2) + +(3)"#), r#"-1 + ~2 + +3"#);
    }

    #[test]
    fn quote() {
        assert_eq!(round_trip(r#""hello""#), r#""hello""#);
        assert_eq!(round_trip(r#"'hello'"#), r#""hello""#);
        assert_eq!(round_trip(r#"u'hello'"#), r#"u"hello""#);
        assert_eq!(round_trip(r#"r'hello'"#), r#""hello""#);
        assert_eq!(round_trip(r#"b'hello'"#), r#"b"hello""#);
        assert_eq!(round_trip(r#"("abc" "def" "ghi")"#), r#""abcdefghi""#);
        assert_eq!(round_trip(r#""he\"llo""#), r#"'he"llo'"#);
        assert_eq!(round_trip(r#"f"abc{'def'}{1}""#), r#"f"abc{'def'}{1}""#);
        assert_eq!(round_trip(r#"f'abc{"def"}{1}'"#), r#"f"abc{'def'}{1}""#);
    }

    #[test]
    fn indent() {
        assert_eq!(
            round_trip(
                r#"
if True:
  pass
"#
                .trim(),
            ),
            r#"
if True:
    pass
"#
            .trim()
            .replace('\n', LineEnding::default().as_str())
        );
    }

    #[test]
    fn set_quote() {
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::Double,
                &LineEnding::default(),
                r#""hello""#
            ),
            r#""hello""#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::Single,
                &LineEnding::default(),
                r#""hello""#
            ),
            r#"'hello'"#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::Double,
                &LineEnding::default(),
                r#"'hello'"#
            ),
            r#""hello""#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::Single,
                &LineEnding::default(),
                r#"'hello'"#
            ),
            r#"'hello'"#
        );
    }

    #[test]
    fn set_indent() {
        assert_eq!(
            round_trip_with(
                &Indentation::new("    ".to_string()),
                &Quote::default(),
                &LineEnding::default(),
                r#"
if True:
  pass
"#
                .trim(),
            ),
            r#"
if True:
    pass
"#
            .trim()
            .replace('\n', LineEnding::default().as_str())
        );
        assert_eq!(
            round_trip_with(
                &Indentation::new("  ".to_string()),
                &Quote::default(),
                &LineEnding::default(),
                r#"
if True:
  pass
"#
                .trim(),
            ),
            r#"
if True:
  pass
"#
            .trim()
            .replace('\n', LineEnding::default().as_str())
        );
        assert_eq!(
            round_trip_with(
                &Indentation::new("\t".to_string()),
                &Quote::default(),
                &LineEnding::default(),
                r#"
if True:
  pass
"#
                .trim(),
            ),
            r#"
if True:
	pass
"#
            .trim()
            .replace('\n', LineEnding::default().as_str())
        );
    }

    #[test]
    fn set_line_ending() {
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::default(),
                &LineEnding::Lf,
                "if True:\n    print(42)",
            ),
            "if True:\n    print(42)",
        );

        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::default(),
                &LineEnding::CrLf,
                "if True:\n    print(42)",
            ),
            "if True:\r\n    print(42)",
        );

        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                &Quote::default(),
                &LineEnding::Cr,
                "if True:\n    print(42)",
            ),
            "if True:\r    print(42)",
        );
    }
}
