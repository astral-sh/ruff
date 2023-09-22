//! Generate Python source code from an abstract syntax tree (AST).

use std::ops::Deref;

use ruff_python_ast::{
    self as ast, Alias, ArgOrKeyword, BoolOp, CmpOp, Comprehension, Constant, ConversionFlag,
    DebugText, ExceptHandler, Expr, Identifier, MatchCase, Operator, Parameter, Parameters,
    Pattern, Stmt, Suite, TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
    WithItem,
};
use ruff_python_ast::{ParameterWithDefault, TypeParams};
use ruff_python_literal::escape::{AsciiEscape, Escape, UnicodeEscape};
use ruff_source_file::LineEnding;

use super::stylist::{Indentation, Quote, Stylist};

mod precedence {
    pub(crate) const NAMED_EXPR: u8 = 1;
    pub(crate) const ASSIGN: u8 = 3;
    pub(crate) const ANN_ASSIGN: u8 = 5;
    pub(crate) const AUG_ASSIGN: u8 = 5;
    pub(crate) const EXPR: u8 = 5;
    pub(crate) const YIELD: u8 = 7;
    pub(crate) const YIELD_FROM: u8 = 7;
    pub(crate) const IF: u8 = 9;
    pub(crate) const FOR: u8 = 9;
    pub(crate) const WHILE: u8 = 9;
    pub(crate) const RETURN: u8 = 11;
    pub(crate) const SLICE: u8 = 13;
    pub(crate) const SUBSCRIPT: u8 = 13;
    pub(crate) const COMPREHENSION_TARGET: u8 = 19;
    pub(crate) const TUPLE: u8 = 19;
    pub(crate) const FORMATTED_VALUE: u8 = 19;
    pub(crate) const COMMA: u8 = 21;
    pub(crate) const ASSERT: u8 = 23;
    pub(crate) const COMPREHENSION_ELEMENT: u8 = 27;
    pub(crate) const LAMBDA: u8 = 27;
    pub(crate) const IF_EXP: u8 = 27;
    pub(crate) const COMPREHENSION: u8 = 29;
    pub(crate) const OR: u8 = 31;
    pub(crate) const AND: u8 = 33;
    pub(crate) const NOT: u8 = 35;
    pub(crate) const CMP: u8 = 37;
    pub(crate) const BIT_OR: u8 = 39;
    pub(crate) const BIT_XOR: u8 = 41;
    pub(crate) const BIT_AND: u8 = 43;
    pub(crate) const LSHIFT: u8 = 45;
    pub(crate) const RSHIFT: u8 = 45;
    pub(crate) const ADD: u8 = 47;
    pub(crate) const SUB: u8 = 47;
    pub(crate) const MULT: u8 = 49;
    pub(crate) const DIV: u8 = 49;
    pub(crate) const MOD: u8 = 49;
    pub(crate) const FLOORDIV: u8 = 49;
    pub(crate) const MAT_MULT: u8 = 49;
    pub(crate) const INVERT: u8 = 53;
    pub(crate) const UADD: u8 = 53;
    pub(crate) const USUB: u8 = 53;
    pub(crate) const POW: u8 = 55;
    pub(crate) const AWAIT: u8 = 57;
    pub(crate) const MAX: u8 = 63;
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

    /// Generate source code from a [`Stmt`].
    pub fn stmt(mut self, stmt: &Stmt) -> String {
        self.unparse_stmt(stmt);
        self.generate()
    }

    /// Generate source code from an [`Expr`].
    pub fn expr(mut self, expr: &Expr) -> String {
        self.unparse_expr(expr, 0);
        self.generate()
    }

    /// Generate source code from a [`Constant`].
    pub fn constant(mut self, constant: &Constant) -> String {
        self.unparse_constant(constant);
        self.generate()
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

    fn body(&mut self, stmts: &[Stmt]) {
        self.indent_depth = self.indent_depth.saturating_add(1);
        for stmt in stmts {
            self.unparse_stmt(stmt);
        }
        self.indent_depth = self.indent_depth.saturating_sub(1);
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

    fn p_id(&mut self, s: &Identifier) {
        self.p(s.as_str());
    }

    fn p_bytes_repr(&mut self, s: &[u8]) {
        let escape = AsciiEscape::with_preferred_quote(s, self.quote.into());
        if let Some(len) = escape.layout().len {
            self.buffer.reserve(len);
        }
        escape.bytes_repr().write(&mut self.buffer).unwrap(); // write to string doesn't fail
    }

    fn p_str_repr(&mut self, s: &str) {
        let escape = UnicodeEscape::with_preferred_quote(s, self.quote.into());
        if let Some(len) = escape.layout().len {
            self.buffer.reserve(len);
        }
        escape.str_repr().write(&mut self.buffer).unwrap(); // write to string doesn't fail
    }

    fn p_if(&mut self, cond: bool, s: &str) {
        if cond {
            self.p(s);
        }
    }

    fn p_delim(&mut self, first: &mut bool, s: &str) {
        self.p_if(!std::mem::take(first), s);
    }

    pub(crate) fn generate(self) -> String {
        self.buffer
    }

    pub fn unparse_suite(&mut self, suite: &Suite) {
        for stmt in suite {
            self.unparse_stmt(stmt);
        }
    }

    pub(crate) fn unparse_stmt(&mut self, ast: &Stmt) {
        macro_rules! statement {
            ($body:block) => {{
                self.newline();
                self.p(&self.indent.deref().repeat(self.indent_depth));
                $body
                self.initial = false;
            }};
        }

        match ast {
            Stmt::FunctionDef(ast::StmtFunctionDef {
                is_async,
                name,
                parameters,
                body,
                returns,
                decorator_list,
                type_params,
                ..
            }) => {
                self.newlines(if self.indent_depth == 0 { 2 } else { 1 });
                for decorator in decorator_list {
                    statement!({
                        self.p("@");
                        self.unparse_expr(&decorator.expression, precedence::MAX);
                    });
                }
                statement!({
                    if *is_async {
                        self.p("async ");
                    }
                    self.p("def ");
                    self.p_id(name);
                    if let Some(type_params) = type_params {
                        self.unparse_type_params(type_params);
                    }
                    self.p("(");
                    self.unparse_parameters(parameters);
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
            Stmt::ClassDef(ast::StmtClassDef {
                name,
                arguments,
                body,
                decorator_list,
                type_params,
                range: _,
            }) => {
                self.newlines(if self.indent_depth == 0 { 2 } else { 1 });
                for decorator in decorator_list {
                    statement!({
                        self.p("@");
                        self.unparse_expr(&decorator.expression, precedence::MAX);
                    });
                }
                statement!({
                    self.p("class ");
                    self.p_id(name);
                    if let Some(type_params) = type_params {
                        self.unparse_type_params(type_params);
                    }
                    if let Some(arguments) = arguments {
                        self.p("(");
                        let mut first = true;
                        for arg_or_keyword in arguments.arguments_source_order() {
                            match arg_or_keyword {
                                ArgOrKeyword::Arg(arg) => {
                                    self.p_delim(&mut first, ", ");
                                    self.unparse_expr(arg, precedence::MAX);
                                }
                                ArgOrKeyword::Keyword(keyword) => {
                                    self.p_delim(&mut first, ", ");
                                    if let Some(arg) = &keyword.arg {
                                        self.p_id(arg);
                                        self.p("=");
                                    } else {
                                        self.p("**");
                                    }
                                    self.unparse_expr(&keyword.value, precedence::MAX);
                                }
                            }
                        }
                        self.p(")");
                    }
                    self.p(":");
                });
                self.body(body);
                if self.indent_depth == 0 {
                    self.newlines(2);
                }
            }
            Stmt::Return(ast::StmtReturn { value, range: _ }) => {
                statement!({
                    if let Some(expr) = value {
                        self.p("return ");
                        self.unparse_expr(expr, precedence::RETURN);
                    } else {
                        self.p("return");
                    }
                });
            }
            Stmt::Delete(ast::StmtDelete { targets, range: _ }) => {
                statement!({
                    self.p("del ");
                    let mut first = true;
                    for expr in targets {
                        self.p_delim(&mut first, ", ");
                        self.unparse_expr(expr, precedence::COMMA);
                    }
                });
            }
            Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
                statement!({
                    for target in targets {
                        self.unparse_expr(target, precedence::ASSIGN);
                        self.p(" = ");
                    }
                    self.unparse_expr(value, precedence::ASSIGN);
                });
            }
            Stmt::AugAssign(ast::StmtAugAssign {
                target,
                op,
                value,
                range: _,
            }) => {
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
            Stmt::AnnAssign(ast::StmtAnnAssign {
                target,
                annotation,
                value,
                simple,
                range: _,
            }) => {
                statement!({
                    let need_parens = matches!(target.as_ref(), Expr::Name(_)) && !simple;
                    self.p_if(need_parens, "(");
                    self.unparse_expr(target, precedence::ANN_ASSIGN);
                    self.p_if(need_parens, ")");
                    self.p(": ");
                    self.unparse_expr(annotation, precedence::COMMA);
                    if let Some(value) = value {
                        self.p(" = ");
                        self.unparse_expr(value, precedence::COMMA);
                    }
                });
            }
            Stmt::For(ast::StmtFor {
                is_async,
                target,
                iter,
                body,
                orelse,
                ..
            }) => {
                statement!({
                    if *is_async {
                        self.p("async ");
                    }
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
            Stmt::While(ast::StmtWhile {
                test,
                body,
                orelse,
                range: _,
            }) => {
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
            Stmt::If(ast::StmtIf {
                test,
                body,
                elif_else_clauses,
                range: _,
            }) => {
                statement!({
                    self.p("if ");
                    self.unparse_expr(test, precedence::IF);
                    self.p(":");
                });
                self.body(body);

                for clause in elif_else_clauses {
                    if let Some(test) = &clause.test {
                        statement!({
                            self.p("elif ");
                            self.unparse_expr(test, precedence::IF);
                            self.p(":");
                        });
                    } else {
                        statement!({
                            self.p("else:");
                        });
                    }
                    self.body(&clause.body);
                }
            }
            Stmt::With(ast::StmtWith {
                is_async,
                items,
                body,
                ..
            }) => {
                statement!({
                    if *is_async {
                        self.p("async ");
                    }
                    self.p("with ");
                    let mut first = true;
                    for item in items {
                        self.p_delim(&mut first, ", ");
                        self.unparse_with_item(item);
                    }
                    self.p(":");
                });
                self.body(body);
            }
            Stmt::Match(ast::StmtMatch {
                subject,
                cases,
                range: _,
            }) => {
                statement!({
                    self.p("match ");
                    self.unparse_expr(subject, precedence::MAX);
                    self.p(":");
                });
                for case in cases {
                    self.indent_depth = self.indent_depth.saturating_add(1);
                    statement!({
                        self.unparse_match_case(case);
                    });
                    self.indent_depth = self.indent_depth.saturating_sub(1);
                }
            }
            Stmt::TypeAlias(ast::StmtTypeAlias {
                name,
                range: _,
                type_params,
                value,
            }) => {
                self.p("type ");
                self.unparse_expr(name, precedence::MAX);
                if let Some(type_params) = type_params {
                    self.unparse_type_params(type_params);
                }
                self.p(" = ");
                self.unparse_expr(value, precedence::ASSIGN);
            }
            Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _,
            }) => {
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
            Stmt::Try(ast::StmtTry {
                body,
                handlers,
                orelse,
                finalbody,
                is_star,
                range: _,
            }) => {
                statement!({
                    self.p("try:");
                });
                self.body(body);

                for handler in handlers {
                    statement!({
                        self.unparse_except_handler(handler, *is_star);
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
            Stmt::Assert(ast::StmtAssert {
                test,
                msg,
                range: _,
            }) => {
                statement!({
                    self.p("assert ");
                    self.unparse_expr(test, precedence::ASSERT);
                    if let Some(msg) = msg {
                        self.p(", ");
                        self.unparse_expr(msg, precedence::ASSERT);
                    }
                });
            }
            Stmt::Import(ast::StmtImport { names, range: _ }) => {
                statement!({
                    self.p("import ");
                    let mut first = true;
                    for alias in names {
                        self.p_delim(&mut first, ", ");
                        self.unparse_alias(alias);
                    }
                });
            }
            Stmt::ImportFrom(ast::StmtImportFrom {
                module,
                names,
                level,
                range: _,
            }) => {
                statement!({
                    self.p("from ");
                    if let Some(level) = level {
                        for _ in 0..*level {
                            self.p(".");
                        }
                    }
                    if let Some(module) = module {
                        self.p_id(module);
                    }
                    self.p(" import ");
                    let mut first = true;
                    for alias in names {
                        self.p_delim(&mut first, ", ");
                        self.unparse_alias(alias);
                    }
                });
            }
            Stmt::Global(ast::StmtGlobal { names, range: _ }) => {
                statement!({
                    self.p("global ");
                    let mut first = true;
                    for name in names {
                        self.p_delim(&mut first, ", ");
                        self.p_id(name);
                    }
                });
            }
            Stmt::Nonlocal(ast::StmtNonlocal { names, range: _ }) => {
                statement!({
                    self.p("nonlocal ");
                    let mut first = true;
                    for name in names {
                        self.p_delim(&mut first, ", ");
                        self.p_id(name);
                    }
                });
            }
            Stmt::Expr(ast::StmtExpr { value, range: _ }) => {
                statement!({
                    self.unparse_expr(value, precedence::EXPR);
                });
            }
            Stmt::Pass(_) => {
                statement!({
                    self.p("pass");
                });
            }
            Stmt::Break(_) => {
                statement!({
                    self.p("break");
                });
            }
            Stmt::Continue(_) => {
                statement!({
                    self.p("continue");
                });
            }
            Stmt::IpyEscapeCommand(ast::StmtIpyEscapeCommand { kind, value, .. }) => {
                statement!({
                    self.p(&format!("{kind}{value}"));
                });
            }
        }
    }

    fn unparse_except_handler(&mut self, ast: &ExceptHandler, star: bool) {
        match ast {
            ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                type_,
                name,
                body,
                range: _,
            }) => {
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
                    self.p_id(name);
                }
                self.p(":");
                self.body(body);
            }
        }
    }

    fn unparse_pattern(&mut self, ast: &Pattern) {
        match ast {
            Pattern::MatchValue(ast::PatternMatchValue { value, range: _ }) => {
                self.unparse_expr(value, precedence::MAX);
            }
            Pattern::MatchSingleton(ast::PatternMatchSingleton { value, range: _ }) => {
                self.unparse_constant(value);
            }
            Pattern::MatchSequence(ast::PatternMatchSequence { patterns, range: _ }) => {
                self.p("[");
                let mut first = true;
                for pattern in patterns {
                    self.p_delim(&mut first, ", ");
                    self.unparse_pattern(pattern);
                }
                self.p("]");
            }
            Pattern::MatchMapping(ast::PatternMatchMapping {
                keys,
                patterns,
                rest,
                range: _,
            }) => {
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
                    self.p_id(rest);
                }
                self.p("}");
            }
            Pattern::MatchClass(_) => {}
            Pattern::MatchStar(ast::PatternMatchStar { name, range: _ }) => {
                self.p("*");
                if let Some(name) = name {
                    self.p_id(name);
                } else {
                    self.p("_");
                }
            }
            Pattern::MatchAs(ast::PatternMatchAs {
                pattern,
                name,
                range: _,
            }) => {
                if let Some(pattern) = pattern {
                    self.unparse_pattern(pattern);
                    self.p(" as ");
                }
                if let Some(name) = name {
                    self.p_id(name);
                } else {
                    self.p("_");
                }
            }
            Pattern::MatchOr(ast::PatternMatchOr { patterns, range: _ }) => {
                let mut first = true;
                for pattern in patterns {
                    self.p_delim(&mut first, " | ");
                    self.unparse_pattern(pattern);
                }
            }
        }
    }

    fn unparse_match_case(&mut self, ast: &MatchCase) {
        self.p("case ");
        self.unparse_pattern(&ast.pattern);
        if let Some(guard) = &ast.guard {
            self.p(" if ");
            self.unparse_expr(guard, precedence::MAX);
        }
        self.p(":");
        self.body(&ast.body);
    }

    fn unparse_type_params(&mut self, type_params: &TypeParams) {
        self.p("[");
        let mut first = true;
        for type_param in type_params.iter() {
            self.p_delim(&mut first, ", ");
            self.unparse_type_param(type_param);
        }
        self.p("]");
    }

    pub(crate) fn unparse_type_param(&mut self, ast: &TypeParam) {
        match ast {
            TypeParam::TypeVar(TypeParamTypeVar { name, bound, .. }) => {
                self.p_id(name);
                if let Some(expr) = bound {
                    self.p(": ");
                    self.unparse_expr(expr, precedence::MAX);
                }
            }
            TypeParam::TypeVarTuple(TypeParamTypeVarTuple { name, .. }) => {
                self.p("*");
                self.p_id(name);
            }
            TypeParam::ParamSpec(TypeParamParamSpec { name, .. }) => {
                self.p("**");
                self.p_id(name);
            }
        }
    }

    pub(crate) fn unparse_expr(&mut self, ast: &Expr, level: u8) {
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
        match ast {
            Expr::BoolOp(ast::ExprBoolOp {
                op,
                values,
                range: _,
            }) => {
                let (op, prec) = opprec!(bin, op, BoolOp, And("and", AND), Or("or", OR));
                group_if!(prec, {
                    let mut first = true;
                    for val in values {
                        self.p_delim(&mut first, op);
                        self.unparse_expr(val, prec + 1);
                    }
                });
            }
            Expr::NamedExpr(ast::ExprNamedExpr {
                target,
                value,
                range: _,
            }) => {
                group_if!(precedence::NAMED_EXPR, {
                    self.unparse_expr(target, precedence::NAMED_EXPR);
                    self.p(" := ");
                    self.unparse_expr(value, precedence::NAMED_EXPR + 1);
                });
            }
            Expr::BinOp(ast::ExprBinOp {
                left,
                op,
                right,
                range: _,
            }) => {
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
            Expr::UnaryOp(ast::ExprUnaryOp {
                op,
                operand,
                range: _,
            }) => {
                let (op, prec) = opprec!(
                    un,
                    op,
                    ruff_python_ast::UnaryOp,
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
            Expr::Lambda(ast::ExprLambda {
                parameters,
                body,
                range: _,
            }) => {
                group_if!(precedence::LAMBDA, {
                    self.p("lambda");
                    if let Some(parameters) = parameters {
                        self.p(" ");
                        self.unparse_parameters(parameters);
                    }
                    self.p(": ");
                    self.unparse_expr(body, precedence::LAMBDA);
                });
            }
            Expr::IfExp(ast::ExprIfExp {
                test,
                body,
                orelse,
                range: _,
            }) => {
                group_if!(precedence::IF_EXP, {
                    self.unparse_expr(body, precedence::IF_EXP + 1);
                    self.p(" if ");
                    self.unparse_expr(test, precedence::IF_EXP + 1);
                    self.p(" else ");
                    self.unparse_expr(orelse, precedence::IF_EXP);
                });
            }
            Expr::Dict(ast::ExprDict {
                keys,
                values,
                range: _,
            }) => {
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
            Expr::Set(ast::ExprSet { elts, range: _ }) => {
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
            Expr::ListComp(ast::ExprListComp {
                elt,
                generators,
                range: _,
            }) => {
                self.p("[");
                self.unparse_expr(elt, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p("]");
            }
            Expr::SetComp(ast::ExprSetComp {
                elt,
                generators,
                range: _,
            }) => {
                self.p("{");
                self.unparse_expr(elt, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p("}");
            }
            Expr::DictComp(ast::ExprDictComp {
                key,
                value,
                generators,
                range: _,
            }) => {
                self.p("{");
                self.unparse_expr(key, precedence::COMPREHENSION_ELEMENT);
                self.p(": ");
                self.unparse_expr(value, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p("}");
            }
            Expr::GeneratorExp(ast::ExprGeneratorExp {
                elt,
                generators,
                range: _,
            }) => {
                self.p("(");
                self.unparse_expr(elt, precedence::COMPREHENSION_ELEMENT);
                self.unparse_comp(generators);
                self.p(")");
            }
            Expr::Await(ast::ExprAwait { value, range: _ }) => {
                group_if!(precedence::AWAIT, {
                    self.p("await ");
                    self.unparse_expr(value, precedence::MAX);
                });
            }
            Expr::Yield(ast::ExprYield { value, range: _ }) => {
                group_if!(precedence::YIELD, {
                    self.p("yield");
                    if let Some(value) = value {
                        self.p(" ");
                        self.unparse_expr(value, precedence::YIELD + 1);
                    }
                });
            }
            Expr::YieldFrom(ast::ExprYieldFrom { value, range: _ }) => {
                group_if!(precedence::YIELD_FROM, {
                    self.p("yield from ");
                    self.unparse_expr(value, precedence::MAX);
                });
            }
            Expr::Compare(ast::ExprCompare {
                left,
                ops,
                comparators,
                range: _,
            }) => {
                group_if!(precedence::CMP, {
                    let new_lvl = precedence::CMP + 1;
                    self.unparse_expr(left, new_lvl);
                    for (op, cmp) in ops.iter().zip(comparators) {
                        let op = match op {
                            CmpOp::Eq => " == ",
                            CmpOp::NotEq => " != ",
                            CmpOp::Lt => " < ",
                            CmpOp::LtE => " <= ",
                            CmpOp::Gt => " > ",
                            CmpOp::GtE => " >= ",
                            CmpOp::Is => " is ",
                            CmpOp::IsNot => " is not ",
                            CmpOp::In => " in ",
                            CmpOp::NotIn => " not in ",
                        };
                        self.p(op);
                        self.unparse_expr(cmp, new_lvl);
                    }
                });
            }
            Expr::Call(ast::ExprCall {
                func,
                arguments,
                range: _,
            }) => {
                self.unparse_expr(func, precedence::MAX);
                self.p("(");
                if let (
                    [Expr::GeneratorExp(ast::ExprGeneratorExp {
                        elt,
                        generators,
                        range: _,
                    })],
                    [],
                ) = (arguments.args.as_slice(), arguments.keywords.as_slice())
                {
                    // Ensure that a single generator doesn't get double-parenthesized.
                    self.unparse_expr(elt, precedence::COMMA);
                    self.unparse_comp(generators);
                } else {
                    let mut first = true;

                    for arg_or_keyword in arguments.arguments_source_order() {
                        match arg_or_keyword {
                            ArgOrKeyword::Arg(arg) => {
                                self.p_delim(&mut first, ", ");
                                self.unparse_expr(arg, precedence::COMMA);
                            }
                            ArgOrKeyword::Keyword(keyword) => {
                                self.p_delim(&mut first, ", ");
                                if let Some(arg) = &keyword.arg {
                                    self.p_id(arg);
                                    self.p("=");
                                    self.unparse_expr(&keyword.value, precedence::COMMA);
                                } else {
                                    self.p("**");
                                    self.unparse_expr(&keyword.value, precedence::MAX);
                                }
                            }
                        }
                    }
                }
                self.p(")");
            }
            Expr::FormattedValue(ast::ExprFormattedValue {
                value,
                debug_text,
                conversion,
                format_spec,
                range: _,
            }) => self.unparse_formatted(
                value,
                debug_text.as_ref(),
                *conversion,
                format_spec.as_deref(),
            ),
            Expr::FString(ast::ExprFString { values, .. }) => {
                self.unparse_f_string(values, false);
            }
            Expr::Constant(ast::ExprConstant { value, range: _ }) => {
                self.unparse_constant(value);
            }
            Expr::Attribute(ast::ExprAttribute { value, attr, .. }) => {
                if let Expr::Constant(ast::ExprConstant {
                    value: Constant::Int(_),
                    ..
                }) = value.as_ref()
                {
                    self.p("(");
                    self.unparse_expr(value, precedence::MAX);
                    self.p(").");
                } else {
                    self.unparse_expr(value, precedence::MAX);
                    self.p(".");
                };
                self.p_id(attr);
            }
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                self.unparse_expr(value, precedence::MAX);
                self.p("[");
                self.unparse_expr(slice, precedence::SUBSCRIPT);
                self.p("]");
            }
            Expr::Starred(ast::ExprStarred { value, .. }) => {
                self.p("*");
                self.unparse_expr(value, precedence::MAX);
            }
            Expr::Name(ast::ExprName { id, .. }) => self.p(id.as_str()),
            Expr::List(ast::ExprList { elts, .. }) => {
                self.p("[");
                let mut first = true;
                for elt in elts {
                    self.p_delim(&mut first, ", ");
                    self.unparse_expr(elt, precedence::COMMA);
                }
                self.p("]");
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
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
            Expr::Slice(ast::ExprSlice {
                lower,
                upper,
                step,
                range: _,
            }) => {
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
            Expr::IpyEscapeCommand(ast::ExprIpyEscapeCommand { kind, value, .. }) => {
                self.p(&format!("{kind}{value}"));
            }
        }
    }

    pub(crate) fn unparse_constant(&mut self, constant: &Constant) {
        assert_eq!(f64::MAX_10_EXP, 308);
        let inf_str = "1e309";
        match constant {
            Constant::Bytes(b) => {
                self.p_bytes_repr(b);
            }
            Constant::Str(ast::StringConstant { value, unicode, .. }) => {
                if *unicode {
                    self.p("u");
                }
                self.p_str_repr(value);
            }
            Constant::None => self.p("None"),
            Constant::Bool(b) => self.p(if *b { "True" } else { "False" }),
            Constant::Int(i) => self.p(&format!("{i}")),
            Constant::Float(fp) => {
                if fp.is_infinite() {
                    self.p(inf_str);
                } else {
                    self.p(&ruff_python_literal::float::to_string(*fp));
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

    fn unparse_parameters(&mut self, parameters: &Parameters) {
        let mut first = true;
        for (i, parameter_with_default) in parameters
            .posonlyargs
            .iter()
            .chain(&parameters.args)
            .enumerate()
        {
            self.p_delim(&mut first, ", ");
            self.unparse_parameter_with_default(parameter_with_default);
            self.p_if(i + 1 == parameters.posonlyargs.len(), ", /");
        }
        if parameters.vararg.is_some() || !parameters.kwonlyargs.is_empty() {
            self.p_delim(&mut first, ", ");
            self.p("*");
        }
        if let Some(vararg) = &parameters.vararg {
            self.unparse_parameter(vararg);
        }
        for kwarg in &parameters.kwonlyargs {
            self.p_delim(&mut first, ", ");
            self.unparse_parameter_with_default(kwarg);
        }
        if let Some(kwarg) = &parameters.kwarg {
            self.p_delim(&mut first, ", ");
            self.p("**");
            self.unparse_parameter(kwarg);
        }
    }

    fn unparse_parameter(&mut self, parameter: &Parameter) {
        self.p_id(&parameter.name);
        if let Some(ann) = &parameter.annotation {
            self.p(": ");
            self.unparse_expr(ann, precedence::COMMA);
        }
    }

    fn unparse_parameter_with_default(&mut self, parameter_with_default: &ParameterWithDefault) {
        self.unparse_parameter(&parameter_with_default.parameter);
        if let Some(default) = &parameter_with_default.default {
            self.p("=");
            self.unparse_expr(default, precedence::COMMA);
        }
    }

    fn unparse_comp(&mut self, generators: &[Comprehension]) {
        for comp in generators {
            self.p(if comp.is_async {
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

    fn unparse_f_string_body(&mut self, values: &[Expr], is_spec: bool) {
        for value in values {
            self.unparse_f_string_elem(value, is_spec);
        }
    }

    fn unparse_formatted(
        &mut self,
        val: &Expr,
        debug_text: Option<&DebugText>,
        conversion: ConversionFlag,
        spec: Option<&Expr>,
    ) {
        let mut generator = Generator::new(self.indent, self.quote, self.line_ending);
        generator.unparse_expr(val, precedence::FORMATTED_VALUE);
        let brace = if generator.buffer.starts_with('{') {
            // put a space to avoid escaping the bracket
            "{ "
        } else {
            "{"
        };
        self.p(brace);

        if let Some(debug_text) = debug_text {
            self.buffer += debug_text.leading.as_str();
        }

        self.buffer += &generator.buffer;

        if let Some(debug_text) = debug_text {
            self.buffer += debug_text.trailing.as_str();
        }

        if !conversion.is_none() {
            self.p("!");
            #[allow(clippy::cast_possible_truncation)]
            self.p(&format!("{}", conversion as u8 as char));
        }

        if let Some(spec) = spec {
            self.p(":");
            self.unparse_f_string_elem(spec, true);
        }

        self.p("}");
    }

    fn unparse_f_string_elem(&mut self, expr: &Expr, is_spec: bool) {
        match expr {
            Expr::Constant(ast::ExprConstant { value, .. }) => {
                if let Constant::Str(ast::StringConstant { value, .. }) = value {
                    self.unparse_f_string_literal(value);
                } else {
                    unreachable!()
                }
            }
            Expr::FString(ast::ExprFString { values, .. }) => {
                self.unparse_f_string(values, is_spec);
            }
            Expr::FormattedValue(ast::ExprFormattedValue {
                value,
                debug_text,
                conversion,
                format_spec,
                range: _,
            }) => self.unparse_formatted(
                value,
                debug_text.as_ref(),
                *conversion,
                format_spec.as_deref(),
            ),
            _ => unreachable!(),
        }
    }

    fn unparse_f_string_literal(&mut self, s: &str) {
        let s = s.replace('{', "{{").replace('}', "}}");
        self.p(&s);
    }

    fn unparse_f_string(&mut self, values: &[Expr], is_spec: bool) {
        if is_spec {
            self.unparse_f_string_body(values, is_spec);
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
            generator.unparse_f_string_body(values, is_spec);
            let body = &generator.buffer;
            self.p_str_repr(body);
        }
    }

    fn unparse_alias(&mut self, alias: &Alias) {
        self.p_id(&alias.name);
        if let Some(asname) = &alias.asname {
            self.p(" as ");
            self.p_id(asname);
        }
    }

    fn unparse_with_item(&mut self, with_item: &WithItem) {
        self.unparse_expr(&with_item.context_expr, precedence::MAX);
        if let Some(optional_vars) = &with_item.optional_vars {
            self.p(" as ");
            self.unparse_expr(optional_vars, precedence::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use ruff_python_ast::{Mod, ModModule};
    use ruff_python_parser::{self, parse_suite, Mode};
    use ruff_source_file::LineEnding;

    use crate::stylist::{Indentation, Quote};

    use super::Generator;

    fn round_trip(contents: &str) -> String {
        let indentation = Indentation::default();
        let quote = Quote::default();
        let line_ending = LineEnding::default();
        let stmt = parse_suite(contents, "<filename>").unwrap();
        let mut generator = Generator::new(&indentation, quote, line_ending);
        generator.unparse_suite(&stmt);
        generator.generate()
    }

    fn round_trip_with(
        indentation: &Indentation,
        quote: Quote,
        line_ending: LineEnding,
        contents: &str,
    ) -> String {
        let stmt = parse_suite(contents, "<filename>").unwrap();
        let mut generator = Generator::new(indentation, quote, line_ending);
        generator.unparse_suite(&stmt);
        generator.generate()
    }

    fn jupyter_round_trip(contents: &str) -> String {
        let indentation = Indentation::default();
        let quote = Quote::default();
        let line_ending = LineEnding::default();
        let ast = ruff_python_parser::parse(contents, Mode::Ipython, "<filename>").unwrap();
        let Mod::Module(ModModule { body, .. }) = ast else {
            panic!("Source code didn't return ModModule")
        };
        let [stmt] = body.as_slice() else {
            panic!("Expected only one statement in source code")
        };
        let mut generator = Generator::new(&indentation, quote, line_ending);
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
    fn unparse_magic_commands() {
        assert_eq!(
            jupyter_round_trip("%matplotlib inline"),
            "%matplotlib inline"
        );
        assert_eq!(
            jupyter_round_trip("%matplotlib \\\n  inline"),
            "%matplotlib   inline"
        );
        assert_eq!(jupyter_round_trip("dir = !pwd"), "dir = !pwd");
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
        assert_round_trip!("foo([x for x in y])");
        assert_round_trip!("foo([(x := 2) for x in y])");
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
            r#"class Foo[T]:
    pass"#
        );
        assert_round_trip!(
            r#"class Foo[T](Bar):
    pass"#
        );
        assert_round_trip!(
            r#"class Foo[*Ts]:
    pass"#
        );
        assert_round_trip!(
            r#"class Foo[**P]:
    pass"#
        );
        assert_round_trip!(
            r#"class Foo[T, U, *Ts, **P]:
    pass"#
        );
        assert_round_trip!(
            r#"def f() -> (int, str):
    pass"#
        );
        assert_round_trip!("[await x async for x in y]");
        assert_round_trip!("[await i for i in b if await c]");
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
            r#"def test[T]():
    pass"#
        );
        assert_round_trip!(
            r#"def test[*Ts]():
    pass"#
        );
        assert_round_trip!(
            r#"def test[**P]():
    pass"#
        );
        assert_round_trip!(
            r#"def test[T, U, *Ts, **P]():
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
        assert_round_trip!(
            r#"def f():

    def f():
        pass"#
        );
        assert_round_trip!(
            r#"@foo
def f():

    @foo
    def f():
        pass"#
        );

        assert_round_trip!(
            r#"@foo
class Foo:

    @foo
    def f():
        pass"#
        );

        assert_round_trip!(r#"[lambda n: n for n in range(10)]"#);
        assert_round_trip!(r#"[n[0:2] for n in range(10)]"#);
        assert_round_trip!(r#"[n[0] for n in range(10)]"#);
        assert_round_trip!(r#"[(n, n * 2) for n in range(10)]"#);
        assert_round_trip!(r#"[1 if n % 2 == 0 else 0 for n in range(10)]"#);
        assert_round_trip!(r#"[n % 2 == 0 or 0 for n in range(10)]"#);
        assert_round_trip!(r#"[(n := 2) for n in range(10)]"#);
        assert_round_trip!(r#"((n := 2) for n in range(10))"#);
        assert_round_trip!(r#"[n * 2 for n in range(10)]"#);
        assert_round_trip!(r#"{n * 2 for n in range(10)}"#);
        assert_round_trip!(r#"{i: n * 2 for i, n in enumerate(range(10))}"#);
        assert_round_trip!(
            "class SchemaItem(NamedTuple):
    fields: ((\"property_key\", str),)"
        );
        assert_round_trip!(
            "def func():
    return (i := 1)"
        );
        assert_round_trip!("yield (i := 1)");
        assert_round_trip!("x = (i := 1)");
        assert_round_trip!("x += (i := 1)");

        // Type aliases
        assert_round_trip!(r#"type Foo = int | str"#);
        assert_round_trip!(r#"type Foo[T] = list[T]"#);
        assert_round_trip!(r#"type Foo[*Ts] = ..."#);
        assert_round_trip!(r#"type Foo[**P] = ..."#);
        assert_round_trip!(r#"type Foo[T, U, *Ts, **P] = ..."#);
        // https://github.com/astral-sh/ruff/issues/6498
        assert_round_trip!(r#"f(a=1, *args, **kwargs)"#);
        assert_round_trip!(r#"f(*args, a=1, **kwargs)"#);
        assert_round_trip!(r#"f(*args, a=1, *args2, **kwargs)"#);
        assert_round_trip!("class A(*args, a=2, *args2, **kwargs):\n    pass");
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
    fn self_documenting_f_string() {
        assert_round_trip!(r#"f"{ chr(65)  =   }""#);
        assert_round_trip!(r#"f"{ chr(65)  =   !s}""#);
        assert_round_trip!(r#"f"{ chr(65)  =   !r}""#);
        assert_round_trip!(r#"f"{ chr(65)  =   :#x}""#);
        assert_round_trip!(r#"f"{a=!r:0.05f}""#);
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
