//! Generate Python source code from an abstract syntax tree (AST).

use std::ops::Deref;

use rustpython_parser::ast::{
    Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, ConversionFlag, Excepthandler,
    ExcepthandlerKind, Expr, ExprKind, MatchCase, Operator, Pattern, PatternKind, Stmt, StmtKind,
    Suite, Withitem,
};

use ruff_rustpython::vendor::{bytes, str};

use crate::source_code::stylist::{Indentation, LineEnding, Quote, Stylist};

mod precedence {
    pub const ASSIGN: u8 = 3;
    pub const ANN_ASSIGN: u8 = 5;
    pub const AUG_ASSIGN: u8 = 5;
    pub const EXPR: u8 = 5;
    pub const YIELD: u8 = 7;
    pub const YIELD_FROM: u8 = 7;
    pub const IF: u8 = 9;
    pub const FOR: u8 = 9;
    pub const ASYNC_FOR: u8 = 9;
    pub const WHILE: u8 = 9;
    pub const RETURN: u8 = 11;
    pub const SLICE: u8 = 13;
    pub const SUBSCRIPT: u8 = 13;
    pub const COMPREHENSION_TARGET: u8 = 19;
    pub const TUPLE: u8 = 19;
    pub const FORMATTED_VALUE: u8 = 19;
    pub const COMMA: u8 = 21;
    pub const NAMED_EXPR: u8 = 23;
    pub const ASSERT: u8 = 23;
    pub const LAMBDA: u8 = 27;
    pub const IF_EXP: u8 = 27;
    pub const COMPREHENSION: u8 = 29;
    pub const OR: u8 = 31;
    pub const AND: u8 = 33;
    pub const NOT: u8 = 35;
    pub const CMP: u8 = 37;
    pub const BIT_OR: u8 = 39;
    pub const BIT_XOR: u8 = 41;
    pub const BIT_AND: u8 = 43;
    pub const LSHIFT: u8 = 45;
    pub const RSHIFT: u8 = 45;
    pub const ADD: u8 = 47;
    pub const SUB: u8 = 47;
    pub const MULT: u8 = 49;
    pub const DIV: u8 = 49;
    pub const MOD: u8 = 49;
    pub const FLOORDIV: u8 = 49;
    pub const MAT_MULT: u8 = 49;
    pub const INVERT: u8 = 53;
    pub const UADD: u8 = 53;
    pub const USUB: u8 = 53;
    pub const POW: u8 = 55;
    pub const AWAIT: u8 = 57;
    pub const MAX: u8 = 63;
}

pub struct Generator<'a> {
    /// The indentation style to use.
    indent: &'a Indentation,
    /// The quote style to use for string literals.
    quote: Quote,
    /// The line ending to use.
    line_ending: LineEnding,
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
    pub const fn new(indent: &'a Indentation, quote: Quote, line_ending: LineEnding) -> Self {
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
                self.buffer += &self.line_ending;
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
                            self.unparse_expr(decorator, precedence::MAX);
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
                        self.unparse_expr(returns, precedence::MAX);
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
                            self.unparse_expr(decorator, precedence::MAX);
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
                        self.unparse_expr(returns, precedence::MAX);
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
                            self.unparse_expr(decorator, precedence::MAX);
                        });
                    }
                    self.newline();
                    self.p("class ");
                    self.p(name);
                    let mut first = true;
                    for base in bases {
                        self.p_if(first, "(");
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(base, precedence::MAX);
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
                        self.unparse_expr(&keyword.node.value, precedence::MAX);
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
                        self.unparse_expr(expr, precedence::RETURN);
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
                        self.unparse_expr(expr, precedence::COMMA);
                    }
                });
            }
            StmtKind::Assign { targets, value, .. } => {
                statement!({
                    for target in targets {
                        self.unparse_expr(target, precedence::ASSIGN);
                        self.p(" = ");
                    }
                    self.unparse_expr(value, precedence::ASSIGN);
                });
            }
            StmtKind::AugAssign { target, op, value } => {
                statement!({
                    self.unparse_expr(target, precedence::AUG_ASSIGN);
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
                    self.unparse_expr(value, precedence::AUG_ASSIGN);
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
                    self.unparse_expr(target, precedence::ANN_ASSIGN);
                    self.p_if(need_parens, ")");
                    self.p(": ");
                    self.unparse_expr(annotation, precedence::ANN_ASSIGN);
                    if let Some(value) = value {
                        self.p(" = ");
                        self.unparse_expr(value, precedence::COMMA);
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
                    self.unparse_expr(target, precedence::FOR);
                    self.p(" in ");
                    self.unparse_expr(iter, precedence::MAX);
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
                    self.unparse_expr(target, precedence::ASYNC_FOR);
                    self.p(" in ");
                    self.unparse_expr(iter, precedence::MAX);
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
                    self.unparse_expr(test, precedence::WHILE);
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
                    self.unparse_expr(test, precedence::IF);
                    self.p(":");
                });
                self.body(body);

                let mut orelse_: &Vec<Stmt<U>> = orelse;
                loop {
                    if orelse_.len() == 1 && matches!(orelse_[0].node, StmtKind::If { .. }) {
                        if let StmtKind::If { body, test, orelse } = &orelse_[0].node {
                            statement!({
                                self.p("elif ");
                                self.unparse_expr(test, precedence::IF);
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
            StmtKind::Match { subject, cases } => {
                statement!({
                    self.p("match ");
                    self.unparse_expr(subject, precedence::MAX);
                    self.p(":");
                });
                for case in cases {
                    self.indent_depth += 1;
                    statement!({
                        self.unparse_match_case(case);
                    });
                    self.indent_depth -= 1;
                }
            }
            StmtKind::Raise { exc, cause } => {
                statement!({
                    self.p("raise");
                    if let Some(exc) = exc {
                        self.p(" ");
                        self.unparse_expr(exc, precedence::MAX);
                    }
                    if let Some(cause) = cause {
                        self.p(" from ");
                        self.unparse_expr(cause, precedence::MAX);
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
                        self.unparse_excepthandler(handler, false);
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
            StmtKind::TryStar {
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
                        self.unparse_excepthandler(handler, true);
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
                    self.unparse_expr(test, precedence::ASSERT);
                    if let Some(msg) = msg {
                        self.p(", ");
                        self.unparse_expr(msg, precedence::ASSERT);
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
                    self.unparse_expr(value, precedence::EXPR);
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

    fn unparse_excepthandler<U>(&mut self, ast: &Excepthandler<U>, star: bool) {
        match &ast.node {
            ExcepthandlerKind::ExceptHandler { type_, name, body } => {
                self.p("except");
                if star {
                    self.p("*");
                }
                if let Some(type_) = type_ {
                    self.p(" ");
                    self.unparse_expr(type_, precedence::MAX);
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

    fn unparse_pattern<U>(&mut self, ast: &Pattern<U>) {
        match &ast.node {
            PatternKind::MatchValue { value } => {
                self.unparse_expr(value, precedence::MAX);
            }
            PatternKind::MatchSingleton { value } => {
                self.unparse_constant(value);
            }
            PatternKind::MatchSequence { patterns } => {
                self.p("[");
                let mut first = true;
                for pattern in patterns {
                    self.p_delim(&mut first, ", ");
                    self.unparse_pattern(pattern);
                }
                self.p("]");
            }
            PatternKind::MatchMapping {
                keys,
                patterns,
                rest,
            } => {
                self.p("{");
                let mut first = true;
                for (key, pattern) in keys.iter().zip(patterns) {
                    self.p_delim(&mut first, ", ");
                    self.unparse_expr(key, precedence::MAX);
                    self.p(": ");
                    self.unparse_pattern(pattern);
                }
                if let Some(rest) = rest {
                    self.p_delim(&mut first, ", ");
                    self.p("**");
                    self.p(rest);
                }
                self.p("}");
            }
            PatternKind::MatchClass { .. } => {}
            PatternKind::MatchStar { name } => {
                self.p("*");
                if let Some(name) = name {
                    self.p(name);
                } else {
                    self.p("_");
                }
            }
            PatternKind::MatchAs { pattern, name } => {
                if let Some(pattern) = pattern {
                    self.unparse_pattern(pattern);
                    self.p(" as ");
                }
                if let Some(name) = name {
                    self.p(name);
                } else {
                    self.p("_");
                }
            }
            PatternKind::MatchOr { patterns } => {
                let mut first = true;
                for pattern in patterns {
                    self.p_delim(&mut first, " | ");
                    self.unparse_pattern(pattern);
                }
            }
        }
    }

    fn unparse_match_case<U>(&mut self, ast: &MatchCase<U>) {
        self.p("case ");
        self.unparse_pattern(&ast.pattern);
        if let Some(guard) = &ast.guard {
            self.p(" if ");
            self.unparse_expr(guard, precedence::MAX);
        }
        self.p(":");
        self.body(&ast.body);
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
                group_if!(precedence::NAMED_EXPR, {
                    self.unparse_expr(target, precedence::NAMED_EXPR);
                    self.p(" := ");
                    self.unparse_expr(value, precedence::NAMED_EXPR + 1);
                });
            }
            ExprKind::BinOp { left, op, right } => {
                let rassoc = matches!(op, Operator::Pow);
                let (op, prec) = opprec!(
                    bin,
                    op,
                    Operator,
                    Add("+", ADD),
                    Sub("-", SUB),
                    Mult("*", MULT),
                    MatMult("@", MAT_MULT),
                    Div("/", DIV),
                    Mod("%", MOD),
                    Pow("**", POW),
                    LShift("<<", LSHIFT),
                    RShift(">>", RSHIFT),
                    BitOr("|", BIT_OR),
                    BitXor("^", BIT_XOR),
                    BitAnd("&", BIT_AND),
                    FloorDiv("//", FLOORDIV),
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
                    Invert("~", INVERT),
                    Not("not ", NOT),
                    UAdd("+", UADD),
                    USub("-", USUB)
                );
                group_if!(prec, {
                    self.p(op);
                    self.unparse_expr(operand, prec);
                });
            }
            ExprKind::Lambda { args, body } => {
                group_if!(precedence::LAMBDA, {
                    let npos = args.args.len() + args.posonlyargs.len();
                    self.p(if npos > 0 { "lambda " } else { "lambda" });
                    self.unparse_args(args);
                    self.p(": ");
                    self.unparse_expr(body, precedence::LAMBDA);
                });
            }
            ExprKind::IfExp { test, body, orelse } => {
                group_if!(precedence::IF_EXP, {
                    self.unparse_expr(body, precedence::IF_EXP + 1);
                    self.p(" if ");
                    self.unparse_expr(test, precedence::IF_EXP + 1);
                    self.p(" else ");
                    self.unparse_expr(orelse, precedence::IF_EXP);
                });
            }
            ExprKind::Dict { keys, values } => {
                self.p("{");
                let mut first = true;
                for (k, v) in keys.iter().zip(values) {
                    self.p_delim(&mut first, ", ");
                    if let Some(k) = k {
                        self.unparse_expr(k, precedence::COMMA);
                        self.p(": ");
                        self.unparse_expr(v, precedence::COMMA);
                    } else {
                        self.p("**");
                        self.unparse_expr(v, precedence::MAX);
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
                        self.unparse_expr(v, precedence::COMMA);
                    }
                    self.p("}");
                }
            }
            ExprKind::ListComp { elt, generators } => {
                self.p("[");
                self.unparse_expr(elt, precedence::MAX);
                self.unparse_comp(generators);
                self.p("]");
            }
            ExprKind::SetComp { elt, generators } => {
                self.p("{");
                self.unparse_expr(elt, precedence::MAX);
                self.unparse_comp(generators);
                self.p("}");
            }
            ExprKind::DictComp {
                key,
                value,
                generators,
            } => {
                self.p("{");
                self.unparse_expr(key, precedence::MAX);
                self.p(": ");
                self.unparse_expr(value, precedence::MAX);
                self.unparse_comp(generators);
                self.p("}");
            }
            ExprKind::GeneratorExp { elt, generators } => {
                self.p("(");
                self.unparse_expr(elt, precedence::COMMA);
                self.unparse_comp(generators);
                self.p(")");
            }
            ExprKind::Await { value } => {
                group_if!(precedence::AWAIT, {
                    self.p("await ");
                    self.unparse_expr(value, precedence::MAX);
                });
            }
            ExprKind::Yield { value } => {
                group_if!(precedence::YIELD, {
                    self.p("yield");
                    if let Some(value) = value {
                        self.p(" ");
                        self.unparse_expr(value, precedence::YIELD + 1);
                    }
                });
            }
            ExprKind::YieldFrom { value } => {
                group_if!(precedence::YIELD_FROM, {
                    self.p("yield from ");
                    self.unparse_expr(value, precedence::MAX);
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
                self.unparse_expr(func, precedence::MAX);
                self.p("(");
                if let (
                    [Expr {
                        node: ExprKind::GeneratorExp { elt, generators },
                        ..
                    }],
                    [],
                ) = (&**args, &**keywords)
                {
                    // Ensure that a single generator doesn't get double-parenthesized.
                    self.unparse_expr(elt, precedence::COMMA);
                    self.unparse_comp(generators);
                } else {
                    let mut first = true;
                    for arg in args {
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(arg, precedence::COMMA);
                    }
                    for kw in keywords {
                        self.p_delim(&mut first, ", ");
                        if let Some(arg) = &kw.node.arg {
                            self.p(arg);
                            self.p("=");
                            self.unparse_expr(&kw.node.value, precedence::COMMA);
                        } else {
                            self.p("**");
                            self.unparse_expr(&kw.node.value, precedence::MAX);
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
                    self.unparse_expr(value, precedence::MAX);
                    self.p(").");
                } else {
                    self.unparse_expr(value, precedence::MAX);
                    self.p(".");
                };
                self.p(attr);
            }
            ExprKind::Subscript { value, slice, .. } => {
                self.unparse_expr(value, precedence::MAX);
                self.p("[");
                self.unparse_expr(slice, precedence::SUBSCRIPT);
                self.p("]");
            }
            ExprKind::Starred { value, .. } => {
                self.p("*");
                self.unparse_expr(value, precedence::MAX);
            }
            ExprKind::Name { id, .. } => self.p(id),
            ExprKind::List { elts, .. } => {
                self.p("[");
                let mut first = true;
                for elt in elts {
                    self.p_delim(&mut first, ", ");
                    self.unparse_expr(elt, precedence::COMMA);
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
                            self.unparse_expr(elt, precedence::COMMA);
                        }
                        self.p_if(elts.len() == 1, ",");
                    });
                }
            }
            ExprKind::Slice { lower, upper, step } => {
                if let Some(lower) = lower {
                    self.unparse_expr(lower, precedence::SLICE);
                }
                self.p(":");
                if let Some(upper) = upper {
                    self.unparse_expr(upper, precedence::SLICE);
                }
                if let Some(step) = step {
                    self.p(":");
                    self.unparse_expr(step, precedence::SLICE);
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
            Constant::Int(i) => self.p(&format!("{i}")),
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
                self.unparse_expr(&args.defaults[i], precedence::COMMA);
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
                self.unparse_expr(default, precedence::COMMA);
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
            self.unparse_expr(ann, precedence::COMMA);
        }
    }

    fn unparse_comp<U>(&mut self, generators: &[Comprehension<U>]) {
        for comp in generators {
            self.p(if comp.is_async > 0 {
                " async for "
            } else {
                " for "
            });
            self.unparse_expr(&comp.target, precedence::COMPREHENSION_TARGET);
            self.p(" in ");
            self.unparse_expr(&comp.iter, precedence::COMPREHENSION);
            for cond in &comp.ifs {
                self.p(" if ");
                self.unparse_expr(cond, precedence::COMPREHENSION);
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
        generator.unparse_expr(val, precedence::FORMATTED_VALUE);
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
                    Quote::Single => Quote::Double,
                    Quote::Double => Quote::Single,
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
        self.unparse_expr(&withitem.context_expr, precedence::MAX);
        if let Some(optional_vars) = &withitem.optional_vars {
            self.p(" as ");
            self.unparse_expr(optional_vars, precedence::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use rustpython_parser as parser;

    use crate::source_code::stylist::{Indentation, LineEnding, Quote};
    use crate::source_code::Generator;

    fn round_trip(contents: &str) -> String {
        let indentation = Indentation::default();
        let quote = Quote::default();
        let line_ending = LineEnding::default();
        let program = parser::parse_program(contents, "<filename>").unwrap();
        let stmt = program.first().unwrap();
        let mut generator = Generator::new(&indentation, quote, line_ending);
        generator.unparse_stmt(stmt);
        generator.generate()
    }

    fn round_trip_with(
        indentation: &Indentation,
        quote: Quote,
        line_ending: LineEnding,
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
        assert_round_trip!("{i for i in b async for i in a if await i for b in i}");
        assert_round_trip!("f(**x)");
        assert_round_trip!("{**x}");
        assert_round_trip!("f(**([] or 5))");
        assert_round_trip!(r#"my_function(*[1], *[2], **{"three": 3}, **{"four": "four"})"#);
        assert_round_trip!("{**([] or 5)}");
        assert_round_trip!("del l[0]");
        assert_round_trip!("del obj.x");
        assert_round_trip!("a @ b");
        assert_round_trip!("a @= b");
        assert_round_trip!("x.foo");
        assert_round_trip!("return await (await bar())");
        assert_round_trip!("(5).foo");
        assert_round_trip!(r#"our_dict = {"a": 1, **{"b": 2, "c": 3}}"#);
        assert_round_trip!(r#"j = [1, 2, 3]"#);
        assert_round_trip!(
            r#"def test(a1, a2, b1=j, b2="123", b3={}, b4=[]):
    pass"#
        );
        assert_round_trip!("a @ b");
        assert_round_trip!("a @= b");
        assert_round_trip!("[1, 2, 3]");
        assert_round_trip!("foo(1)");
        assert_round_trip!("foo(1, 2)");
        assert_round_trip!("foo(x for x in y)");
        assert_round_trip!("x = yield 1");
        assert_round_trip!("return (yield 1)");
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
        assert_round_trip!(r#"return f"functools.{qualname}({', '.join(args)})""#);
        assert_round_trip!(
            r#"class TreeFactory(*[FactoryMixin, TreeBase], **{"metaclass": Foo}):
    pass"#
        );
        assert_round_trip!(
            r#"class Foo(Bar, object):
    pass"#
        );
        assert_round_trip!(
            r#"def f() -> (int, str):
    pass"#
        );
        assert_round_trip!("[(await x) async for x in y]");
        assert_round_trip!("[(await i) for i in b if await c]");
        assert_round_trip!("(await x async for x in y)");
        assert_round_trip!(
            r#"async def read_data(db):
    async with connect(db) as db_cxn:
        data = await db_cxn.fetch("SELECT foo FROM bar;")
    async for datum in data:
        if quux(datum):
            return datum"#
        );
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
        assert_round_trip!(
            r#"try:
    pass
except Exception as e:
    pass"#
        );
        assert_round_trip!(
            r#"try:
    pass
except* Exception as e:
    pass"#
        );
        assert_round_trip!(
            r#"match x:
    case [1, 2, 3]:
        return 2
    case 4 as y:
        return y"#
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
                Quote::Double,
                LineEnding::default(),
                r#""hello""#
            ),
            r#""hello""#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::Single,
                LineEnding::default(),
                r#""hello""#
            ),
            r#"'hello'"#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::Double,
                LineEnding::default(),
                r#"'hello'"#
            ),
            r#""hello""#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::Single,
                LineEnding::default(),
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
                Quote::default(),
                LineEnding::default(),
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
                Quote::default(),
                LineEnding::default(),
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
                Quote::default(),
                LineEnding::default(),
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
                Quote::default(),
                LineEnding::Lf,
                "if True:\n    print(42)",
            ),
            "if True:\n    print(42)",
        );

        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::default(),
                LineEnding::CrLf,
                "if True:\n    print(42)",
            ),
            "if True:\r\n    print(42)",
        );

        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::default(),
                LineEnding::Cr,
                "if True:\n    print(42)",
            ),
            "if True:\r    print(42)",
        );
    }
}
