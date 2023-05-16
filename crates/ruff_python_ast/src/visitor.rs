//! AST visitor trait and walk functions.

use rustpython_parser::ast::{
    self, Alias, Arg, Arguments, Boolop, Cmpop, Comprehension, Constant, Excepthandler, Expr,
    ExprContext, Keyword, MatchCase, Operator, Pattern, Stmt, Unaryop, Withitem,
};

/// A trait for AST visitors. Visits all nodes in the AST recursively.
///
/// Prefer [`crate::statement_visitor::StatementVisitor`] for visitors that only need to visit
/// statements.
pub trait Visitor<'a> {
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
    fn visit_expr_context(&mut self, expr_context: &'a ExprContext) {
        walk_expr_context(self, expr_context);
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
}

pub fn walk_body<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, body: &'a [Stmt]) {
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
}

pub fn walk_stmt<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, stmt: &'a Stmt) {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        }) => {
            visitor.visit_arguments(args);
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }
            for expr in returns {
                visitor.visit_annotation(expr);
            }
            visitor.visit_body(body);
        }
        Stmt::AsyncFunctionDef(ast::StmtAsyncFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        }) => {
            visitor.visit_arguments(args);
            for expr in decorator_list {
                visitor.visit_expr(expr);
            }
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
            for expr in bases {
                visitor.visit_expr(expr);
            }
            for keyword in keywords {
                visitor.visit_keyword(keyword);
            }
            for expr in decorator_list {
                visitor.visit_expr(expr);
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
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            visitor.visit_expr(value);
            for expr in targets {
                visitor.visit_expr(expr);
            }
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
            ..
        }) => {
            visitor.visit_annotation(annotation);
            if let Some(expr) = value {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr(target);
        }
        Stmt::For(ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        }) => {
            visitor.visit_expr(iter);
            visitor.visit_expr(target);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::AsyncFor(ast::StmtAsyncFor {
            target,
            iter,
            body,
            orelse,
            ..
        }) => {
            visitor.visit_expr(iter);
            visitor.visit_expr(target);
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
        Stmt::With(ast::StmtWith { items, body, .. }) => {
            for withitem in items {
                visitor.visit_withitem(withitem);
            }
            visitor.visit_body(body);
        }
        Stmt::AsyncWith(ast::StmtAsyncWith { items, body, .. }) => {
            for withitem in items {
                visitor.visit_withitem(withitem);
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
        }) => {
            visitor.visit_body(body);
            for excepthandler in handlers {
                visitor.visit_excepthandler(excepthandler);
            }
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        Stmt::TryStar(ast::StmtTryStar {
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
        Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
            for alias in names {
                visitor.visit_alias(alias);
            }
        }
        Stmt::Global(_) => {}
        Stmt::Nonlocal(_) => {}
        Stmt::Expr(ast::StmtExpr {
            value,
            range: _range,
        }) => visitor.visit_expr(value),
        Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) => {}
    }
}

pub fn walk_expr<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, expr: &'a Expr) {
    match expr {
        Expr::BoolOp(ast::ExprBoolOp {
            op,
            values,
            range: _range,
        }) => {
            visitor.visit_boolop(op);
            for expr in values {
                visitor.visit_expr(expr);
            }
        }
        Expr::NamedExpr(ast::ExprNamedExpr {
            target,
            value,
            range: _range,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(target);
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
            visitor.visit_unaryop(op);
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
            for expr in keys.iter().flatten() {
                visitor.visit_expr(expr);
            }
            for expr in values {
                visitor.visit_expr(expr);
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
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        Expr::SetComp(ast::ExprSetComp {
            elt,
            generators,
            range: _range,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        Expr::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            range: _range,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(key);
            visitor.visit_expr(value);
        }
        Expr::GeneratorExp(ast::ExprGeneratorExp {
            elt,
            generators,
            range: _range,
        }) => {
            for comprehension in generators {
                visitor.visit_comprehension(comprehension);
            }
            visitor.visit_expr(elt);
        }
        Expr::Await(ast::ExprAwait {
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
        Expr::YieldFrom(ast::ExprYieldFrom {
            value,
            range: _range,
        }) => visitor.visit_expr(value),
        Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _range,
        }) => {
            visitor.visit_expr(left);
            for cmpop in ops {
                visitor.visit_cmpop(cmpop);
            }
            for expr in comparators {
                visitor.visit_expr(expr);
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
        Expr::Constant(ast::ExprConstant { value, .. }) => visitor.visit_constant(value),
        Expr::Attribute(ast::ExprAttribute { value, ctx, .. }) => {
            visitor.visit_expr(value);
            visitor.visit_expr_context(ctx);
        }
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx,
            range: _range,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr(slice);
            visitor.visit_expr_context(ctx);
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx,
            range: _range,
        }) => {
            visitor.visit_expr(value);
            visitor.visit_expr_context(ctx);
        }
        Expr::Name(ast::ExprName { ctx, .. }) => {
            visitor.visit_expr_context(ctx);
        }
        Expr::List(ast::ExprList {
            elts,
            ctx,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr_context(ctx);
        }
        Expr::Tuple(ast::ExprTuple {
            elts,
            ctx,
            range: _range,
        }) => {
            for expr in elts {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr_context(ctx);
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

pub fn walk_constant<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, constant: &'a Constant) {
    if let Constant::Tuple(constants) = constant {
        for constant in constants {
            visitor.visit_constant(constant);
        }
    }
}

pub fn walk_comprehension<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    comprehension: &'a Comprehension,
) {
    visitor.visit_expr(&comprehension.iter);
    visitor.visit_expr(&comprehension.target);
    for expr in &comprehension.ifs {
        visitor.visit_expr(expr);
    }
}

pub fn walk_excepthandler<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    excepthandler: &'a Excepthandler,
) {
    match excepthandler {
        Excepthandler::ExceptHandler(ast::ExcepthandlerExceptHandler { type_, body, .. }) => {
            if let Some(expr) = type_ {
                visitor.visit_expr(expr);
            }
            visitor.visit_body(body);
        }
    }
}

pub fn walk_arguments<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, arguments: &'a Arguments) {
    for arg in &arguments.posonlyargs {
        visitor.visit_arg(arg);
    }
    for arg in &arguments.args {
        visitor.visit_arg(arg);
    }
    if let Some(arg) = &arguments.vararg {
        visitor.visit_arg(arg);
    }
    for arg in &arguments.kwonlyargs {
        visitor.visit_arg(arg);
    }
    for expr in &arguments.kw_defaults {
        visitor.visit_expr(expr);
    }
    if let Some(arg) = &arguments.kwarg {
        visitor.visit_arg(arg);
    }
    for expr in &arguments.defaults {
        visitor.visit_expr(expr);
    }
}

pub fn walk_arg<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, arg: &'a Arg) {
    if let Some(expr) = &arg.annotation {
        visitor.visit_annotation(expr);
    }
}

pub fn walk_keyword<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, keyword: &'a Keyword) {
    visitor.visit_expr(&keyword.value);
}

pub fn walk_withitem<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, withitem: &'a Withitem) {
    visitor.visit_expr(&withitem.context_expr);
    if let Some(expr) = &withitem.optional_vars {
        visitor.visit_expr(expr);
    }
}

pub fn walk_match_case<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, match_case: &'a MatchCase) {
    visitor.visit_pattern(&match_case.pattern);
    if let Some(expr) = &match_case.guard {
        visitor.visit_expr(expr);
    }
    visitor.visit_body(&match_case.body);
}

pub fn walk_pattern<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, pattern: &'a Pattern) {
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
        Pattern::MatchMapping(ast::PatternMatchMapping { keys, patterns, .. }) => {
            for expr in keys {
                visitor.visit_expr(expr);
            }
            for pattern in patterns {
                visitor.visit_pattern(pattern);
            }
        }
        Pattern::MatchClass(ast::PatternMatchClass {
            cls,
            patterns,
            kwd_patterns,
            ..
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
        Pattern::MatchAs(ast::PatternMatchAs { pattern, .. }) => {
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

#[allow(unused_variables)]
pub fn walk_expr_context<'a, V: Visitor<'a> + ?Sized>(
    visitor: &mut V,
    expr_context: &'a ExprContext,
) {
}

#[allow(unused_variables)]
pub fn walk_boolop<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, boolop: &'a Boolop) {}

#[allow(unused_variables)]
pub fn walk_operator<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, operator: &'a Operator) {}

#[allow(unused_variables)]
pub fn walk_unaryop<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, unaryop: &'a Unaryop) {}

#[allow(unused_variables)]
pub fn walk_cmpop<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, cmpop: &'a Cmpop) {}

#[allow(unused_variables)]
pub fn walk_alias<'a, V: Visitor<'a> + ?Sized>(visitor: &mut V, alias: &'a Alias) {}
