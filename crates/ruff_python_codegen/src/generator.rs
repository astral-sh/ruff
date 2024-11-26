//! Generate Python source code from an abstract syntax tree (AST).

use crate::precedence;
use std::ops::Deref;

use ruff_python_ast::str::Quote;
use ruff_python_ast::TypeParams;
use ruff_python_ast::{
    self as ast, Alias, ArgOrKeyword, ExceptHandler, Expr, MatchCase, Operator, Pattern, Singleton,
    Stmt, Suite, TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple, WithItem,
};
use ruff_source_file::LineEnding;

use super::expression_generator::ExpressionGenerator;
use super::stylist::{Indentation, Stylist};

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

impl<'a> ExpressionGenerator<'a> for Generator<'a> {
    fn subgenerator(&self) -> Self {
        Generator::new(self.indent, self.quote, self.line_ending)
    }

    fn subgenerator_with_opposite_quotes(&self) -> Self {
        Generator::new(self.indent, self.quote.opposite(), self.line_ending)
    }

    fn buffer(&mut self) -> &mut String {
        &mut self.buffer
    }

    fn quote(&self) -> Quote {
        self.quote
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
                statement!({
                    self.p("type ");
                    self.unparse_expr(name, precedence::MAX);
                    if let Some(type_params) = type_params {
                        self.unparse_type_params(type_params);
                    }
                    self.p(" = ");
                    self.unparse_expr(value, precedence::ASSIGN);
                });
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
                    if *level > 0 {
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
                self.unparse_singleton(value);
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
            TypeParam::TypeVar(TypeParamTypeVar {
                name,
                bound,
                default,
                ..
            }) => {
                self.p_id(name);
                if let Some(expr) = bound {
                    self.p(": ");
                    self.unparse_expr(expr, precedence::MAX);
                }
                if let Some(expr) = default {
                    self.p(" = ");
                    self.unparse_expr(expr, precedence::MAX);
                }
            }
            TypeParam::TypeVarTuple(TypeParamTypeVarTuple { name, default, .. }) => {
                self.p("*");
                self.p_id(name);
                if let Some(expr) = default {
                    self.p(" = ");
                    self.unparse_expr(expr, precedence::MAX);
                }
            }
            TypeParam::ParamSpec(TypeParamParamSpec { name, default, .. }) => {
                self.p("**");
                self.p_id(name);
                if let Some(expr) = default {
                    self.p(" = ");
                    self.unparse_expr(expr, precedence::MAX);
                }
            }
        }
    }

    pub(crate) fn unparse_singleton(&mut self, singleton: &Singleton) {
        match singleton {
            Singleton::None => self.p("None"),
            Singleton::True => self.p("True"),
            Singleton::False => self.p("False"),
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
    use ruff_python_ast::{str::Quote, Mod, ModModule};
    use ruff_python_parser::{self, parse_module, Mode};
    use ruff_source_file::LineEnding;

    use crate::stylist::Indentation;

    use super::Generator;

    fn round_trip(contents: &str) -> String {
        let indentation = Indentation::default();
        let quote = Quote::default();
        let line_ending = LineEnding::default();
        let module = parse_module(contents).unwrap();
        let mut generator = Generator::new(&indentation, quote, line_ending);
        generator.unparse_suite(module.suite());
        generator.generate()
    }

    fn round_trip_with(
        indentation: &Indentation,
        quote: Quote,
        line_ending: LineEnding,
        contents: &str,
    ) -> String {
        let module = parse_module(contents).unwrap();
        let mut generator = Generator::new(indentation, quote, line_ending);
        generator.unparse_suite(module.suite());
        generator.generate()
    }

    fn jupyter_round_trip(contents: &str) -> String {
        let indentation = Indentation::default();
        let quote = Quote::default();
        let line_ending = LineEnding::default();
        let parsed = ruff_python_parser::parse(contents, Mode::Ipython).unwrap();
        let Mod::Module(ModModule { body, .. }) = parsed.into_syntax() else {
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
        assert_round_trip!(r"j = [1, 2, 3]");
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
            r"class Foo(Bar, object):
    pass"
        );
        assert_round_trip!(
            r"class Foo[T]:
    pass"
        );
        assert_round_trip!(
            r"class Foo[T](Bar):
    pass"
        );
        assert_round_trip!(
            r"class Foo[*Ts]:
    pass"
        );
        assert_round_trip!(
            r"class Foo[**P]:
    pass"
        );
        assert_round_trip!(
            r"class Foo[T, U, *Ts, **P]:
    pass"
        );
        assert_round_trip!(
            r"def f() -> (int, str):
    pass"
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
            r"def f() -> (int, int):
    pass"
        );
        assert_round_trip!(
            r"def test(a, b, /, c, *, d, **kwargs):
    pass"
        );
        assert_round_trip!(
            r"def test(a=3, b=4, /, c=7):
    pass"
        );
        assert_round_trip!(
            r"def test(a, b=4, /, c=8, d=9):
    pass"
        );
        assert_round_trip!(
            r"def test[T]():
    pass"
        );
        assert_round_trip!(
            r"def test[*Ts]():
    pass"
        );
        assert_round_trip!(
            r"def test[**P]():
    pass"
        );
        assert_round_trip!(
            r"def test[T, U, *Ts, **P]():
    pass"
        );
        assert_round_trip!(
            r"def call(*popenargs, timeout=None, **kwargs):
    pass"
        );
        assert_round_trip!(
            r"@functools.lru_cache(maxsize=None)
def f(x: int, y: int) -> int:
    return x + y"
        );
        assert_round_trip!(
            r"try:
    pass
except Exception as e:
    pass"
        );
        assert_round_trip!(
            r"try:
    pass
except* Exception as e:
    pass"
        );
        assert_round_trip!(
            r"match x:
    case [1, 2, 3]:
        return 2
    case 4 as y:
        return y"
        );
        assert_round_trip!(
            r"type X = int
type Y = str"
        );
        assert_eq!(round_trip(r"x = (1, 2, 3)"), r"x = 1, 2, 3");
        assert_eq!(round_trip(r"-(1) + ~(2) + +(3)"), r"-1 + ~2 + +3");
        assert_round_trip!(
            r"def f():

    def f():
        pass"
        );
        assert_round_trip!(
            r"@foo
def f():

    @foo
    def f():
        pass"
        );

        assert_round_trip!(
            r"@foo
class Foo:

    @foo
    def f():
        pass"
        );

        assert_round_trip!(r"[lambda n: n for n in range(10)]");
        assert_round_trip!(r"[n[0:2] for n in range(10)]");
        assert_round_trip!(r"[n[0] for n in range(10)]");
        assert_round_trip!(r"[(n, n * 2) for n in range(10)]");
        assert_round_trip!(r"[1 if n % 2 == 0 else 0 for n in range(10)]");
        assert_round_trip!(r"[n % 2 == 0 or 0 for n in range(10)]");
        assert_round_trip!(r"[(n := 2) for n in range(10)]");
        assert_round_trip!(r"((n := 2) for n in range(10))");
        assert_round_trip!(r"[n * 2 for n in range(10)]");
        assert_round_trip!(r"{n * 2 for n in range(10)}");
        assert_round_trip!(r"{i: n * 2 for i, n in enumerate(range(10))}");
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
        assert_round_trip!(r"type Foo = int | str");
        assert_round_trip!(r"type Foo[T] = list[T]");
        assert_round_trip!(r"type Foo[*Ts] = ...");
        assert_round_trip!(r"type Foo[**P] = ...");
        assert_round_trip!(r"type Foo[T = int] = list[T]");
        assert_round_trip!(r"type Foo[*Ts = int] = ...");
        assert_round_trip!(r"type Foo[*Ts = *int] = ...");
        assert_round_trip!(r"type Foo[**P = int] = ...");
        assert_round_trip!(r"type Foo[T, U, *Ts, **P] = ...");
        // https://github.com/astral-sh/ruff/issues/6498
        assert_round_trip!(r"f(a=1, *args, **kwargs)");
        assert_round_trip!(r"f(*args, a=1, **kwargs)");
        assert_round_trip!(r"f(*args, a=1, *args2, **kwargs)");
        assert_round_trip!("class A(*args, a=2, *args2, **kwargs):\n    pass");
    }

    #[test]
    fn quote() {
        assert_eq!(round_trip(r#""hello""#), r#""hello""#);
        assert_eq!(round_trip(r"'hello'"), r#""hello""#);
        assert_eq!(round_trip(r"u'hello'"), r#"u"hello""#);
        assert_eq!(round_trip(r"r'hello'"), r#""hello""#);
        assert_eq!(round_trip(r"b'hello'"), r#"b"hello""#);
        assert_eq!(round_trip(r#"("abc" "def" "ghi")"#), r#""abc" "def" "ghi""#);
        assert_eq!(round_trip(r#""he\"llo""#), r#"'he"llo'"#);
        assert_eq!(round_trip(r#"f"abc{'def'}{1}""#), r#"f"abc{'def'}{1}""#);
        assert_eq!(round_trip(r#"f'abc{"def"}{1}'"#), r#"f"abc{'def'}{1}""#);
    }

    #[test]
    fn self_documenting_fstring() {
        assert_round_trip!(r#"f"{ chr(65)  =   }""#);
        assert_round_trip!(r#"f"{ chr(65)  =   !s}""#);
        assert_round_trip!(r#"f"{ chr(65)  =   !r}""#);
        assert_round_trip!(r#"f"{ chr(65)  =   :#x}""#);
        assert_round_trip!(r#"f"{  ( chr(65)  ) = }""#);
        assert_round_trip!(r#"f"{a=!r:0.05f}""#);
    }

    #[test]
    fn implicit_string_concatenation() {
        assert_round_trip!(r#""first" "second" "third""#);
        assert_round_trip!(r#"b"first" b"second" b"third""#);
        assert_round_trip!(r#""first" "second" f"third {var}""#);
    }

    #[test]
    fn indent() {
        assert_eq!(
            round_trip(
                r"
if True:
  pass
"
                .trim(),
            ),
            r"
if True:
    pass
"
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
            r"'hello'"
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::Double,
                LineEnding::default(),
                r"'hello'"
            ),
            r#""hello""#
        );
        assert_eq!(
            round_trip_with(
                &Indentation::default(),
                Quote::Single,
                LineEnding::default(),
                r"'hello'"
            ),
            r"'hello'"
        );
    }

    #[test]
    fn set_indent() {
        assert_eq!(
            round_trip_with(
                &Indentation::new("    ".to_string()),
                Quote::default(),
                LineEnding::default(),
                r"
if True:
  pass
"
                .trim(),
            ),
            r"
if True:
    pass
"
            .trim()
            .replace('\n', LineEnding::default().as_str())
        );
        assert_eq!(
            round_trip_with(
                &Indentation::new("  ".to_string()),
                Quote::default(),
                LineEnding::default(),
                r"
if True:
  pass
"
                .trim(),
            ),
            r"
if True:
  pass
"
            .trim()
            .replace('\n', LineEnding::default().as_str())
        );
        assert_eq!(
            round_trip_with(
                &Indentation::new("\t".to_string()),
                Quote::default(),
                LineEnding::default(),
                r"
if True:
  pass
"
                .trim(),
            ),
            r"
if True:
	pass
"
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
