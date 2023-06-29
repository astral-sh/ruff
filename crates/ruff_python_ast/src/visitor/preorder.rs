use rustpython_ast::{ArgWithDefault, Mod, TypeIgnore};
use rustpython_parser::ast::{
    self, Alias, Arg, Arguments, BoolOp, CmpOp, Comprehension, Constant, Decorator, ExceptHandler,
    Expr, Keyword, MatchCase, Operator, Pattern, Stmt, UnaryOp, WithItem,
};

/// Visitor that traverses all nodes recursively in pre-order.
pub trait PreorderVisitor<'a> {
    fn visit_mod(&mut self, module: &'a Mod) {
        walk_module(self, module);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_annotation(&mut self, expr: &'a Expr) {
        walk_annotation(self, expr);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }

    fn visit_decorator(&mut self, decorator: &'a Decorator) {
        walk_decorator(self, decorator);
    }

    fn visit_constant(&mut self, constant: &'a Constant) {
        walk_constant(self, constant);
    }

    fn visit_bool_op(&mut self, bool_op: &'a BoolOp) {
        walk_bool_op(self, bool_op);
    }

    fn visit_operator(&mut self, operator: &'a Operator) {
        walk_operator(self, operator);
    }

    fn visit_unary_op(&mut self, unary_op: &'a UnaryOp) {
        walk_unary_op(self, unary_op);
    }

    fn visit_cmp_op(&mut self, cmp_op: &'a CmpOp) {
        walk_cmp_op(self, cmp_op);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        walk_comprehension(self, comprehension);
    }

    fn visit_except_handler(&mut self, except_handler: &'a ExceptHandler) {
        walk_except_handler(self, except_handler);
    }

    fn visit_format_spec(&mut self, format_spec: &'a Expr) {
        walk_format_spec(self, format_spec);
    }

    fn visit_arguments(&mut self, arguments: &'a Arguments) {
        walk_arguments(self, arguments);
    }

    fn visit_arg(&mut self, arg: &'a Arg) {
        walk_arg(self, arg);
    }

    fn visit_arg_with_default(&mut self, arg_with_default: &'a ArgWithDefault) {
        walk_arg_with_default(self, arg_with_default);
    }

    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        walk_alias(self, alias);
    }

    fn visit_with_item(&mut self, with_item: &'a WithItem) {
        walk_with_item(self, with_item);
    }

    fn visit_match_case(&mut self, match_case: &'a MatchCase) {
        walk_match_case(self, match_case);
    }

    fn visit_pattern(&mut self, pattern: &'a Pattern) {
        walk_pattern(self, pattern);
    }

    fn visit_body(&mut self, body: &'a [Stmt]) {
        walk_body(self, body);
    }

    fn visit_type_ignore(&mut self, type_ignore: &'a TypeIgnore) {
        walk_type_ignore(self, type_ignore);
    }
}

pub fn walk_module<'a, V>(visitor: &mut V, module: &'a Mod)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match module {
        Mod::Module(ast::ModModule {
            body,
            range: _,
            type_ignores,
        }) => {
            visitor.visit_body(body);
            for ignore in type_ignores {
                visitor.visit_type_ignore(ignore);
            }
        }
        Mod::Interactive(ast::ModInteractive { body, range: _ }) => visitor.visit_body(body),
        Mod::Expression(ast::ModExpression { body, range: _ }) => visitor.visit_expr(body),
        Mod::FunctionType(ast::ModFunctionType {
            range: _,
            argtypes,
            returns,
        }) => {
            for arg_type in argtypes {
                visitor.visit_expr(arg_type);
            }

            visitor.visit_expr(returns);
        }
    }
}

pub fn walk_body<'a, V>(visitor: &mut V, body: &'a [Stmt])
where
    V: PreorderVisitor<'a> + ?Sized,
{
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'a, V>(visitor: &mut V, stmt: &'a Stmt)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match stmt {
        Stmt::Expr(ast::StmtExpr {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        Stmt::FunctionDef(ast::StmtFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        })
        | Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        }) => {
            for decorator in decorator_list {
                visitor.visit_decorator(decorator);
            }

            visitor.visit_arguments(args);

            for expr in returns {
                visitor.visit_annotation(expr);
            }

            visitor.visit_body(body);
        }

        Stmt::ClassDef(ast::StmtClassDef {
            bases,
            keywords,
            body,
            decorator_list,
            ..
        }) => {
            for decorator in decorator_list {
                visitor.visit_decorator(decorator);
            }

            for expr in bases {
                visitor.visit_expr(expr);
            }

            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }

            visitor.visit_body(body);
        }

        Stmt::Return(ast::StmtReturn {
            value,
            range: _range,
        }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }

        Stmt::Delete(ast::StmtDelete {
            targets,
            range: _range,
        }) => {
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }

        Stmt::Assign(ast::StmtAssign {
            targets,
            value,
            range: _,
            type_comment: _,
        }) => {
            for expr in targets {
                visitor.visit_expr(expr);
            }

            visitor.visit_expr(value);
        }

        Stmt::AugAssign(ast::StmtAugAssign {
            target,
            op,
            value,
            range: _range,
        }) => {
            visitor.visit_expr(target);
            visitor.visit_operator(op);
            visitor.visit_expr(value);
        }

        Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            annotation,
            value,
            range: _,
            simple: _,
        }) => {
            visitor.visit_expr(target);
            visitor.visit_annotation(annotation);
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }

        Stmt::For(ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        })
        | Stmt::AsyncFor(ast::StmtAsyncFor {
            target,
            iter,
            body,
            orelse,
            ..
        }) => {
            visitor.visit_expr(target);
            visitor.visit_expr(iter);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }

        Stmt::While(ast::StmtWhile {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }

        Stmt::If(ast::StmtIf {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }

        Stmt::With(ast::StmtWith {
            items,
            body,
            type_comment: _,
            range: _,
        })
        | Stmt::AsyncWith(ast::StmtAsyncWith {
            items,
            body,
            type_comment: _,
            range: _,
        }) => {
            for with_item in items {
                visitor.visit_with_item(with_item);
            }
            visitor.visit_body(body);
        }

        Stmt::Match(ast::StmtMatch {
            subject,
            cases,
            range: _range,
        }) => {
            visitor.visit_expr(subject);
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }

        Stmt::Raise(ast::StmtRaise {
            exc,
            cause,
            range: _range,
        }) => {
            if let Some(expr) = exc {
                visitor.visit_expr(expr);
            };
            if let Some(expr) = cause {
                visitor.visit_expr(expr);
            };
        }

        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            range: _range,
        })
        | Stmt::TryStar(ast::StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
            range: _range,
        }) => {
            visitor.visit_body(body);
            for except_handler in handlers {
                visitor.visit_except_handler(except_handler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }

        Stmt::Assert(ast::StmtAssert {
            test,
            msg,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            if let Some(expr) = msg {
                visitor.visit_expr(expr);
            }
        }

        Stmt::Import(ast::StmtImport {
            names,
            range: _range,
        }) => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }

        Stmt::ImportFrom(ast::StmtImportFrom {
            range: _,
            module: _,
            names,
            level: _,
        }) => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }

        Stmt::Pass(_)
        | Stmt::Break(_)
        | Stmt::Continue(_)
        | Stmt::Global(_)
        | Stmt::Nonlocal(_) => {}
    }
}

pub fn walk_annotation<'a, V: PreorderVisitor<'a> + ?Sized>(visitor: &mut V, expr: &'a Expr) {
    visitor.visit_expr(expr);
}

pub fn walk_decorator<'a, V>(visitor: &mut V, decorator: &'a Decorator)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_expr(&decorator.expression);
}

pub fn walk_expr<'a, V>(visitor: &mut V, expr: &'a Expr)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match expr {
        Expr::BoolOp(ast::ExprBoolOp {
            op,
            values,
            range: _range,
        }) => match values.as_slice() {
            [left, rest @ ..] => {
                visitor.visit_expr(left);
                visitor.visit_bool_op(op);
                for expr in rest {
                    visitor.visit_expr(expr);
                }
            }
            [] => {
                visitor.visit_bool_op(op);
            }
        },

        Expr::NamedExpr(ast::ExprNamedExpr {
            target,
            value,
            range: _range,
        }) => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }

        Expr::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _range,
        }) => {
            visitor.visit_expr(left);
            visitor.visit_operator(op);
            visitor.visit_expr(right);
        }

        Expr::UnaryOp(ast::ExprUnaryOp {
            op,
            operand,
            range: _range,
        }) => {
            visitor.visit_unary_op(op);
            visitor.visit_expr(operand);
        }

        Expr::Lambda(ast::ExprLambda {
            args,
            body,
            range: _range,
        }) => {
            visitor.visit_arguments(args);
            visitor.visit_expr(body);
        }

        Expr::IfExp(ast::ExprIfExp {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_expr(body);
            visitor.visit_expr(orelse);
        }

        Expr::Dict(ast::ExprDict {
            keys,
            values,
            range: _range,
        }) => {
            for (key, value) in keys.iter().zip(values) {
                if let Some(key) = key {
                    visitor.visit_expr(key);
                }
                visitor.visit_expr(value);
            }
        }

        Expr::Set(ast::ExprSet {
            elts,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }

        Expr::ListComp(ast::ExprListComp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::SetComp(ast::ExprSetComp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(key);
            visitor.visit_expr(value);

            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::GeneratorExp(ast::ExprGeneratorExp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::Await(ast::ExprAwait {
            value,
            range: _range,
        })
        | Expr::YieldFrom(ast::ExprYieldFrom {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        Expr::Yield(ast::ExprYield {
            value,
            range: _range,
        }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }

        Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _range,
        }) => {
            visitor.visit_expr(left);

            for (op, comparator) in ops.iter().zip(comparators) {
                visitor.visit_cmp_op(op);
                visitor.visit_expr(comparator);
            }
        }

        Expr::Call(ast::ExprCall {
            func,
            args,
            keywords,
            range: _range,
        }) => {
            visitor.visit_expr(func);
            for expr in args {
                visitor.visit_expr(expr);
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }
        }

        Expr::FormattedValue(ast::ExprFormattedValue {
            value, format_spec, ..
        }) => {
            visitor.visit_expr(value);

            if let Some(expr) = format_spec {
                visitor.visit_format_spec(expr);
            }
        }

        Expr::JoinedStr(ast::ExprJoinedStr {
            values,
            range: _range,
        }) => {
            for expr in values {
                visitor.visit_expr(expr);
            }
        }

        Expr::Constant(ast::ExprConstant {
            value,
            range: _,
            kind: _,
        }) => visitor.visit_constant(value),

        Expr::Attribute(ast::ExprAttribute {
            value,
            attr: _,
            ctx: _,
            range: _,
        }) => {
            visitor.visit_expr(value);
        }

        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _range,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx: _,
            range: _range,
        }) => {
            visitor.visit_expr(value);
        }

        Expr::Name(ast::ExprName {
            id: _,
            ctx: _,
            range: _,
        }) => {}

        Expr::List(ast::ExprList {
            elts,
            ctx: _,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }
        Expr::Tuple(ast::ExprTuple {
            elts,
            ctx: _,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }

        Expr::Slice(ast::ExprSlice {
            lower,
            upper,
            step,
            range: _range,
        }) => {
            if let Some(expr) = lower {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = upper {
                visitor.visit_expr(expr);
            }
            if let Some(expr) = step {
                visitor.visit_expr(expr);
            }
        }
    }
}

pub fn walk_constant<'a, V>(visitor: &mut V, constant: &'a Constant)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    if let Constant::Tuple(constants) = constant {
        for constant in constants {
            visitor.visit_constant(constant);
        }
    }
}

pub fn walk_comprehension<'a, V>(visitor: &mut V, comprehension: &'a Comprehension)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_expr(&comprehension.target);
    visitor.visit_expr(&comprehension.iter);

    for expr in &comprehension.ifs {
        visitor.visit_expr(expr);
    }
}

pub fn walk_except_handler<'a, V>(visitor: &mut V, except_handler: &'a ExceptHandler)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match except_handler {
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
            range: _,
            type_,
            name: _,
            body,
        }) => {
            if let Some(expr) = type_ {
                visitor.visit_expr(expr);
            }
            visitor.visit_body(body);
        }
    }
}

pub fn walk_format_spec<'a, V: PreorderVisitor<'a> + ?Sized>(
    visitor: &mut V,
    format_spec: &'a Expr,
) {
    visitor.visit_expr(format_spec);
}

pub fn walk_arguments<'a, V>(visitor: &mut V, arguments: &'a Arguments)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    for arg in arguments.posonlyargs.iter().chain(&arguments.args) {
        visitor.visit_arg_with_default(arg);
    }

    if let Some(arg) = &arguments.vararg {
        visitor.visit_arg(arg);
    }

    for arg in &arguments.kwonlyargs {
        visitor.visit_arg_with_default(arg);
    }

    if let Some(arg) = &arguments.kwarg {
        visitor.visit_arg(arg);
    }
}

pub fn walk_arg<'a, V>(visitor: &mut V, arg: &'a Arg)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    if let Some(expr) = &arg.annotation {
        visitor.visit_annotation(expr);
    }
}

pub fn walk_arg_with_default<'a, V>(visitor: &mut V, arg_with_default: &'a ArgWithDefault)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_arg(&arg_with_default.def);
    if let Some(expr) = &arg_with_default.default {
        visitor.visit_expr(expr);
    }
}

#[inline]
pub fn walk_keyword<'a, V>(visitor: &mut V, keyword: &'a Keyword)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_expr(&keyword.value);
}

pub fn walk_with_item<'a, V>(visitor: &mut V, with_item: &'a WithItem)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_expr(&with_item.context_expr);

    if let Some(expr) = &with_item.optional_vars {
        visitor.visit_expr(expr);
    }
}

pub fn walk_match_case<'a, V>(visitor: &mut V, match_case: &'a MatchCase)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_pattern(&match_case.pattern);
    if let Some(expr) = &match_case.guard {
        visitor.visit_expr(expr);
    }
    visitor.visit_body(&match_case.body);
}

pub fn walk_pattern<'a, V>(visitor: &mut V, pattern: &'a Pattern)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match pattern {
        Pattern::MatchValue(ast::PatternMatchValue {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        Pattern::MatchSingleton(ast::PatternMatchSingleton {
            value,
            range: _range,
        }) => {
            visitor.visit_constant(value);
        }

        Pattern::MatchSequence(ast::PatternMatchSequence {
            patterns,
            range: _range,
        }) => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }

        Pattern::MatchMapping(ast::PatternMatchMapping {
            keys,
            patterns,
            range: _,
            rest: _,
        }) => {
            for (key, pattern) in keys.iter().zip(patterns) {
                visitor.visit_expr(key);
                visitor.visit_pattern(pattern);
            }
        }

        Pattern::MatchClass(ast::PatternMatchClass {
            cls,
            patterns,
            kwd_attrs: _,
            kwd_patterns,
            range: _,
        }) => {
            visitor.visit_expr(cls);
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }

            for pattern in kwd_patterns {
                visitor.visit_pattern(pattern);
            }
        }

        Pattern::MatchStar(_) => {}

        Pattern::MatchAs(ast::PatternMatchAs {
            pattern,
            range: _,
            name: _,
        }) => {
            if let Some(pattern) = pattern {
                visitor.visit_pattern(pattern);
            }
        }

        Pattern::MatchOr(ast::PatternMatchOr {
            patterns,
            range: _range,
        }) => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
    }
}

#[inline]
pub fn walk_type_ignore<'a, V>(_visitor: &mut V, _type_ignore: &'a TypeIgnore)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

pub fn walk_bool_op<'a, V>(_visitor: &mut V, _bool_op: &'a BoolOp)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_operator<'a, V>(_visitor: &mut V, _operator: &'a Operator)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_unary_op<'a, V>(_visitor: &mut V, _unary_op: &'a UnaryOp)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_cmp_op<'a, V>(_visitor: &mut V, _cmp_op: &'a CmpOp)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_alias<'a, V>(_visitor: &mut V, _alias: &'a Alias)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

#[cfg(test)]
mod tests {
    use std::fmt::{Debug, Write};

    use insta::assert_snapshot;
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};

    use crate::node::AnyNodeRef;
    use crate::visitor::preorder::{
        walk_alias, walk_arg, walk_arguments, walk_comprehension, walk_except_handler, walk_expr,
        walk_keyword, walk_match_case, walk_module, walk_pattern, walk_stmt, walk_type_ignore,
        walk_with_item, Alias, Arg, Arguments, BoolOp, CmpOp, Comprehension, Constant,
        ExceptHandler, Expr, Keyword, MatchCase, Mod, Operator, Pattern, PreorderVisitor, Stmt,
        TypeIgnore, UnaryOp, WithItem,
    };

    #[test]
    fn function_arguments() {
        let source = r#"def a(b, c,/, d, e = 20, *args, named=5, other=20, **kwargs): pass"#;

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn function_positional_only_with_default() {
        let source = r#"def a(b, c = 34,/, e = 20, *args): pass"#;

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn compare() {
        let source = r#"4 < x < 5"#;

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn list_comprehension() {
        let source = "[x for x in numbers]";

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn dict_comprehension() {
        let source = "{x: x**2 for x in numbers}";

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn set_comprehension() {
        let source = "{x for x in numbers}";

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn match_class_pattern() {
        let source = r#"
match x:
    case Point2D(0, 0):
        ...
    case Point3D(x=0, y=0, z=0):
        ...
"#;

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    #[test]
    fn decorators() {
        let source = r#"
@decorator
def a():
    pass

@test
class A:
    pass
"#;

        let trace = trace_preorder_visitation(source);

        assert_snapshot!(trace);
    }

    fn trace_preorder_visitation(source: &str) -> String {
        let tokens = lex(source, Mode::Module);
        let parsed = parse_tokens(tokens, Mode::Module, "test.py").unwrap();

        let mut visitor = RecordVisitor::default();
        visitor.visit_mod(&parsed);

        visitor.output
    }

    /// Emits a `tree` with a node for every visited AST node (labelled by the AST node's kind)
    /// and leafs for attributes.
    #[derive(Default)]
    struct RecordVisitor {
        depth: usize,
        output: String,
    }

    impl RecordVisitor {
        fn enter_node<'a, T>(&mut self, node: T)
        where
            T: Into<AnyNodeRef<'a>>,
        {
            self.emit(&node.into().kind());
            self.depth += 1;
        }

        fn exit_node(&mut self) {
            self.depth -= 1;
        }

        fn emit(&mut self, text: &dyn Debug) {
            for _ in 0..self.depth {
                self.output.push_str("  ");
            }

            writeln!(self.output, "- {text:?}").unwrap();
        }
    }

    impl PreorderVisitor<'_> for RecordVisitor {
        fn visit_mod(&mut self, module: &Mod) {
            self.enter_node(module);
            walk_module(self, module);
            self.exit_node();
        }

        fn visit_stmt(&mut self, stmt: &Stmt) {
            self.enter_node(stmt);
            walk_stmt(self, stmt);
            self.exit_node();
        }

        fn visit_annotation(&mut self, expr: &Expr) {
            self.enter_node(expr);
            walk_expr(self, expr);
            self.exit_node();
        }

        fn visit_expr(&mut self, expr: &Expr) {
            self.enter_node(expr);
            walk_expr(self, expr);
            self.exit_node();
        }

        fn visit_constant(&mut self, constant: &Constant) {
            self.emit(&constant);
        }

        fn visit_bool_op(&mut self, bool_op: &BoolOp) {
            self.emit(&bool_op);
        }

        fn visit_operator(&mut self, operator: &Operator) {
            self.emit(&operator);
        }

        fn visit_unary_op(&mut self, unary_op: &UnaryOp) {
            self.emit(&unary_op);
        }

        fn visit_cmp_op(&mut self, cmp_op: &CmpOp) {
            self.emit(&cmp_op);
        }

        fn visit_comprehension(&mut self, comprehension: &Comprehension) {
            self.enter_node(comprehension);
            walk_comprehension(self, comprehension);
            self.exit_node();
        }

        fn visit_except_handler(&mut self, except_handler: &ExceptHandler) {
            self.enter_node(except_handler);
            walk_except_handler(self, except_handler);
            self.exit_node();
        }

        fn visit_format_spec(&mut self, format_spec: &Expr) {
            self.enter_node(format_spec);
            walk_expr(self, format_spec);
            self.exit_node();
        }

        fn visit_arguments(&mut self, arguments: &Arguments) {
            self.enter_node(arguments);
            walk_arguments(self, arguments);
            self.exit_node();
        }

        fn visit_arg(&mut self, arg: &Arg) {
            self.enter_node(arg);
            walk_arg(self, arg);
            self.exit_node();
        }

        fn visit_keyword(&mut self, keyword: &Keyword) {
            self.enter_node(keyword);
            walk_keyword(self, keyword);
            self.exit_node();
        }

        fn visit_alias(&mut self, alias: &Alias) {
            self.enter_node(alias);
            walk_alias(self, alias);
            self.exit_node();
        }

        fn visit_with_item(&mut self, with_item: &WithItem) {
            self.enter_node(with_item);
            walk_with_item(self, with_item);
            self.exit_node();
        }

        fn visit_match_case(&mut self, match_case: &MatchCase) {
            self.enter_node(match_case);
            walk_match_case(self, match_case);
            self.exit_node();
        }

        fn visit_pattern(&mut self, pattern: &Pattern) {
            self.enter_node(pattern);
            walk_pattern(self, pattern);
            self.exit_node();
        }

        fn visit_type_ignore(&mut self, type_ignore: &TypeIgnore) {
            self.enter_node(type_ignore);
            walk_type_ignore(self, type_ignore);
            self.exit_node();
        }
    }
}
