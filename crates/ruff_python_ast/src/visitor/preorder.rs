use crate::prelude::*;

/// Visitor that traverses all nodes recursively in pre-order.
pub trait PreorderVisitor<'a> {
    fn visit_mod(&mut self, module: &'a Mod) {
        walk_module(self, module);
    }

    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        walk_stmt(self, stmt);
    }

    fn visit_annotation(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        walk_expr(self, expr);
    }

    fn visit_constant(&mut self, constant: &'a Constant) {
        walk_constant(self, constant);
    }

    fn visit_boolop(&mut self, boolop: &'a Boolop) {
        walk_boolop(self, boolop);
    }

    fn visit_operator(&mut self, operator: &'a Operator) {
        walk_operator(self, operator);
    }

    fn visit_unaryop(&mut self, unaryop: &'a Unaryop) {
        walk_unaryop(self, unaryop);
    }

    fn visit_cmpop(&mut self, cmpop: &'a Cmpop) {
        walk_cmpop(self, cmpop);
    }

    fn visit_comprehension(&mut self, comprehension: &'a Comprehension) {
        walk_comprehension(self, comprehension);
    }

    fn visit_excepthandler(&mut self, excepthandler: &'a Excepthandler) {
        walk_excepthandler(self, excepthandler);
    }

    fn visit_format_spec(&mut self, format_spec: &'a Expr) {
        walk_expr(self, format_spec);
    }

    fn visit_arguments(&mut self, arguments: &'a Arguments) {
        walk_arguments(self, arguments);
    }

    fn visit_arg(&mut self, arg: &'a Arg) {
        walk_arg(self, arg);
    }

    fn visit_keyword(&mut self, keyword: &'a Keyword) {
        walk_keyword(self, keyword);
    }

    fn visit_alias(&mut self, alias: &'a Alias) {
        walk_alias(self, alias);
    }

    fn visit_withitem(&mut self, withitem: &'a Withitem) {
        walk_withitem(self, withitem);
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
        Mod::Module(ModModule {
            body,
            range: _,
            type_ignores,
        }) => {
            visitor.visit_body(body);
            for ignore in type_ignores {
                visitor.visit_type_ignore(ignore);
            }
        }
        Mod::Interactive(ModInteractive { body, range: _ }) => visitor.visit_body(body),
        Mod::Expression(ModExpression { body, range: _ }) => visitor.visit_expr(body),
        Mod::FunctionType(ModFunctionType {
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
        Stmt::Expr(StmtExpr {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        Stmt::FunctionDef(StmtFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        })
        | Stmt::AsyncFunctionDef(StmtAsyncFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        }) => {
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }

            visitor.visit_arguments(args);

            for expr in returns {
                visitor.visit_annotation(expr);
            }

            visitor.visit_body(body);
        }

        Stmt::ClassDef(StmtClassDef {
            bases,
            keywords,
            body,
            decorator_list,
            ..
        }) => {
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }

            for expr in bases {
                visitor.visit_expr(expr);
            }

            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }

            visitor.visit_body(body);
        }

        Stmt::Return(StmtReturn {
            value,
            range: _range,
        }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }

        Stmt::Delete(StmtDelete {
            targets,
            range: _range,
        }) => {
            for expr in targets {
                visitor.visit_expr(expr);
            }
        }

        Stmt::Assign(StmtAssign {
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

        Stmt::AugAssign(StmtAugAssign {
            target,
            op,
            value,
            range: _range,
        }) => {
            visitor.visit_expr(target);
            visitor.visit_operator(op);
            visitor.visit_expr(value);
        }

        Stmt::AnnAssign(StmtAnnAssign {
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

        Stmt::For(StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        })
        | Stmt::AsyncFor(StmtAsyncFor {
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

        Stmt::While(StmtWhile {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }

        Stmt::If(StmtIf {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }

        Stmt::With(StmtWith {
            items,
            body,
            type_comment: _,
            range: _,
        })
        | Stmt::AsyncWith(StmtAsyncWith {
            items,
            body,
            type_comment: _,
            range: _,
        }) => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            visitor.visit_body(body);
        }

        Stmt::Match(StmtMatch {
            subject,
            cases,
            range: _range,
        }) => {
            visitor.visit_expr(subject);
            for match_case in cases {
                visitor.visit_match_case(match_case);
            }
        }

        Stmt::Raise(StmtRaise {
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

        Stmt::Try(StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            range: _range,
        })
        | Stmt::TryStar(StmtTryStar {
            body,
            handlers,
            orelse,
            finalbody,
            range: _range,
        }) => {
            visitor.visit_body(body);
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }

        Stmt::Assert(StmtAssert {
            test,
            msg,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            if let Some(expr) = msg {
                visitor.visit_expr(expr);
            }
        }

        Stmt::Import(StmtImport {
            names,
            range: _range,
        }) => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }

        Stmt::ImportFrom(StmtImportFrom {
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

pub fn walk_expr<'a, V>(visitor: &mut V, expr: &'a Expr)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match expr {
        Expr::BoolOp(ExprBoolOp {
            op,
            values,
            range: _range,
        }) => match values.as_slice() {
            [left, rest @ ..] => {
                visitor.visit_expr(left);
                visitor.visit_boolop(op);
                for expr in rest {
                    visitor.visit_expr(expr);
                }
            }
            [] => {
                visitor.visit_boolop(op);
            }
        },

        Expr::NamedExpr(ExprNamedExpr {
            target,
            value,
            range: _range,
        }) => {
            visitor.visit_expr(target);
            visitor.visit_expr(value);
        }

        Expr::BinOp(ExprBinOp {
            left,
            op,
            right,
            range: _range,
        }) => {
            visitor.visit_expr(left);
            visitor.visit_operator(op);
            visitor.visit_expr(right);
        }

        Expr::UnaryOp(ExprUnaryOp {
            op,
            operand,
            range: _range,
        }) => {
            visitor.visit_unaryop(op);
            visitor.visit_expr(operand);
        }

        Expr::Lambda(ExprLambda {
            args,
            body,
            range: _range,
        }) => {
            visitor.visit_arguments(args);
            visitor.visit_expr(body);
        }

        Expr::IfExp(ExprIfExp {
            test,
            body,
            orelse,
            range: _range,
        }) => {
            visitor.visit_expr(test);
            visitor.visit_expr(body);
            visitor.visit_expr(orelse);
        }

        Expr::Dict(ExprDict {
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

        Expr::Set(ExprSet {
            elts,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }

        Expr::ListComp(ExprListComp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::SetComp(ExprSetComp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::DictComp(ExprDictComp {
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

        Expr::GeneratorExp(ExprGeneratorExp {
            elt,
            generators,
            range: _range,
        }) => {
            visitor.visit_expr(elt);
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
        }

        Expr::Await(ExprAwait {
            value,
            range: _range,
        })
        | Expr::YieldFrom(ExprYieldFrom {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        Expr::Yield(ExprYield {
            value,
            range: _range,
        }) => {
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
        }

        Expr::Compare(ExprCompare {
            left,
            ops,
            comparators,
            range: _range,
        }) => {
            visitor.visit_expr(left);

            for (op, comparator) in ops.iter().zip(comparators) {
                visitor.visit_cmpop(op);
                visitor.visit_expr(comparator);
            }
        }

        Expr::Call(ExprCall {
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

        Expr::FormattedValue(ExprFormattedValue {
            value, format_spec, ..
        }) => {
            visitor.visit_expr(value);

            if let Some(expr) = format_spec {
                visitor.visit_format_spec(expr);
            }
        }

        Expr::JoinedStr(ExprJoinedStr {
            values,
            range: _range,
        }) => {
            for expr in values {
                visitor.visit_expr(expr);
            }
        }

        Expr::Constant(ExprConstant {
            value,
            range: _,
            kind: _,
        }) => visitor.visit_constant(value),

        Expr::Attribute(ExprAttribute {
            value,
            attr: _,
            ctx: _,
            range: _,
        }) => {
            visitor.visit_expr(value);
        }

        Expr::Subscript(ExprSubscript {
            value,
            slice,
            ctx: _,
            range: _range,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
        }
        Expr::Starred(ExprStarred {
            value,
            ctx: _,
            range: _range,
        }) => {
            visitor.visit_expr(value);
        }

        Expr::Name(ExprName {
            id: _,
            ctx: _,
            range: _,
        }) => {}

        Expr::List(ExprList {
            elts,
            ctx: _,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }
        Expr::Tuple(ExprTuple {
            elts,
            ctx: _,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
        }

        Expr::Slice(ExprSlice {
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

pub fn walk_excepthandler<'a, V>(visitor: &mut V, excepthandler: &'a Excepthandler)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    match excepthandler {
        Excepthandler::ExceptHandler(ExcepthandlerExceptHandler {
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

pub fn walk_arguments<'a, V>(visitor: &mut V, arguments: &'a Arguments)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    let non_default_args_len =
        arguments.posonlyargs.len() + arguments.args.len() - arguments.defaults.len();

    let mut args_iter = arguments.posonlyargs.iter().chain(&arguments.args);

    for _ in 0..non_default_args_len {
        visitor.visit_arg(args_iter.next().unwrap());
    }

    for (arg, default) in args_iter.zip(&arguments.defaults) {
        visitor.visit_arg(arg);
        visitor.visit_expr(default);
    }

    if let Some(arg) = &arguments.vararg {
        visitor.visit_arg(arg);
    }

    let non_default_kwargs_len = arguments.kwonlyargs.len() - arguments.kw_defaults.len();
    let mut kwargsonly_iter = arguments.kwonlyargs.iter();

    for _ in 0..non_default_kwargs_len {
        visitor.visit_arg(kwargsonly_iter.next().unwrap());
    }

    for (arg, default) in kwargsonly_iter.zip(&arguments.kw_defaults) {
        visitor.visit_arg(arg);
        visitor.visit_expr(default);
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

#[inline]
pub fn walk_keyword<'a, V>(visitor: &mut V, keyword: &'a Keyword)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_expr(&keyword.value);
}

pub fn walk_withitem<'a, V>(visitor: &mut V, withitem: &'a Withitem)
where
    V: PreorderVisitor<'a> + ?Sized,
{
    visitor.visit_expr(&withitem.context_expr);

    if let Some(expr) = &withitem.optional_vars {
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
        Pattern::MatchValue(PatternMatchValue {
            value,
            range: _range,
        }) => visitor.visit_expr(value),

        Pattern::MatchSingleton(PatternMatchSingleton {
            value,
            range: _range,
        }) => {
            visitor.visit_constant(value);
        }

        Pattern::MatchSequence(PatternMatchSequence {
            patterns,
            range: _range,
        }) => {
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }

        Pattern::MatchMapping(PatternMatchMapping {
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

        Pattern::MatchClass(PatternMatchClass {
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

        Pattern::MatchAs(PatternMatchAs {
            pattern,
            range: _,
            name: _,
        }) => {
            if let Some(pattern) = pattern {
                visitor.visit_pattern(pattern);
            }
        }

        Pattern::MatchOr(PatternMatchOr {
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

pub fn walk_boolop<'a, V>(_visitor: &mut V, _boolop: &'a Boolop)
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
pub fn walk_unaryop<'a, V>(_visitor: &mut V, _unaryop: &'a Unaryop)
where
    V: PreorderVisitor<'a> + ?Sized,
{
}

#[inline]
pub fn walk_cmpop<'a, V>(_visitor: &mut V, _cmpop: &'a Cmpop)
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
    use crate::node::AnyNodeRef;
    use crate::visitor::preorder::{
        walk_alias, walk_arg, walk_arguments, walk_comprehension, walk_excepthandler, walk_expr,
        walk_keyword, walk_match_case, walk_module, walk_pattern, walk_stmt, walk_type_ignore,
        walk_withitem, Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant,
        Excepthandler, Expr, Keyword, MatchCase, Mod, Operator, Pattern, PreorderVisitor, Stmt,
        String, TypeIgnore, Unaryop, Withitem,
    };
    use insta::assert_snapshot;
    use rustpython_parser::lexer::lex;
    use rustpython_parser::{parse_tokens, Mode};
    use std::fmt::{Debug, Write};

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
        fn visit_boolop(&mut self, boolop: &Boolop) {
            self.emit(&boolop);
        }
        fn visit_operator(&mut self, operator: &Operator) {
            self.emit(&operator);
        }
        fn visit_unaryop(&mut self, unaryop: &Unaryop) {
            self.emit(&unaryop);
        }
        fn visit_cmpop(&mut self, cmpop: &Cmpop) {
            self.emit(&cmpop);
        }

        fn visit_comprehension(&mut self, comprehension: &Comprehension) {
            self.enter_node(comprehension);
            walk_comprehension(self, comprehension);
            self.exit_node();
        }
        fn visit_excepthandler(&mut self, excepthandler: &Excepthandler) {
            self.enter_node(excepthandler);
            walk_excepthandler(self, excepthandler);
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
        fn visit_withitem(&mut self, withitem: &Withitem) {
            self.enter_node(withitem);
            walk_withitem(self, withitem);
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
