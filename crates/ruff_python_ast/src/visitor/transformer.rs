use crate::{
    self as ast, Alias, Arguments, BoolOp, BytesLiteral, CmpOp, Comprehension, Decorator,
    ElifElseClause, ExceptHandler, Expr, ExprContext, FString, InterpolatedStringElement, Keyword,
    MatchCase, Operator, Parameter, Parameters, Pattern, PatternArguments, PatternKeyword, Stmt,
    StringLiteral, TString, TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
    TypeParams, UnaryOp, WithItem,
};
use ruff_allocator::{Allocator, Box as ArenaBox, Slice as ArenaSlice};

/// A trait for transforming ASTs. Visits all nodes in the AST recursively in evaluation-order.
pub trait Transformer<'ast> {
    fn allocator(&self) -> &'ast Allocator;

    fn visit_stmt(&self, stmt: &mut Stmt<'ast>) {
        walk_stmt(self, stmt);
    }
    fn visit_annotation(&self, expr: &mut Expr<'ast>) {
        walk_annotation(self, expr);
    }
    fn visit_decorator(&self, decorator: &mut Decorator<'ast>) {
        walk_decorator(self, decorator);
    }
    fn visit_expr(&self, expr: &mut Expr<'ast>) {
        walk_expr(self, expr);
    }
    fn visit_expr_context(&self, expr_context: &mut ExprContext) {
        walk_expr_context(self, expr_context);
    }
    fn visit_bool_op(&self, bool_op: &mut BoolOp) {
        walk_bool_op(self, bool_op);
    }
    fn visit_operator(&self, operator: &mut Operator) {
        walk_operator(self, operator);
    }
    fn visit_unary_op(&self, unary_op: &mut UnaryOp) {
        walk_unary_op(self, unary_op);
    }
    fn visit_cmp_op(&self, cmp_op: &mut CmpOp) {
        walk_cmp_op(self, cmp_op);
    }
    fn visit_comprehension(&self, comprehension: &mut Comprehension<'ast>) {
        walk_comprehension(self, comprehension);
    }
    fn visit_except_handler(&self, except_handler: &mut ExceptHandler<'ast>) {
        walk_except_handler(self, except_handler);
    }
    fn visit_arguments(&self, arguments: &mut Arguments<'ast>) {
        walk_arguments(self, arguments);
    }
    fn visit_parameters(&self, parameters: &mut Parameters<'ast>) {
        walk_parameters(self, parameters);
    }
    fn visit_parameter(&self, parameter: &mut Parameter<'ast>) {
        walk_parameter(self, parameter);
    }
    fn visit_keyword(&self, keyword: &mut Keyword<'ast>) {
        walk_keyword(self, keyword);
    }
    fn visit_alias(&self, alias: &mut Alias) {
        walk_alias(self, alias);
    }
    fn visit_with_item(&self, with_item: &mut WithItem<'ast>) {
        walk_with_item(self, with_item);
    }
    fn visit_type_params(&self, type_params: &mut TypeParams<'ast>) {
        walk_type_params(self, type_params);
    }
    fn visit_type_param(&self, type_param: &mut TypeParam<'ast>) {
        walk_type_param(self, type_param);
    }
    fn visit_match_case(&self, match_case: &mut MatchCase<'ast>) {
        walk_match_case(self, match_case);
    }
    fn visit_pattern(&self, pattern: &mut Pattern<'ast>) {
        walk_pattern(self, pattern);
    }
    fn visit_pattern_arguments(&self, pattern_arguments: &mut PatternArguments<'ast>) {
        walk_pattern_arguments(self, pattern_arguments);
    }
    fn visit_pattern_keyword(&self, pattern_keyword: &mut PatternKeyword<'ast>) {
        walk_pattern_keyword(self, pattern_keyword);
    }
    fn visit_body(&self, body: &mut ArenaSlice<'ast, Stmt<'ast>>) {
        walk_body(self, body);
    }
    fn visit_elif_else_clause(&self, elif_else_clause: &mut ElifElseClause<'ast>) {
        walk_elif_else_clause(self, elif_else_clause);
    }
    fn visit_f_string(&self, f_string: &mut FString<'ast>) {
        walk_f_string(self, f_string);
    }
    fn visit_interpolated_string_element(
        &self,
        interpolated_string_element: &mut InterpolatedStringElement<'ast>,
    ) {
        walk_interpolated_string_element(self, interpolated_string_element);
    }
    fn visit_t_string(&self, t_string: &mut TString<'ast>) {
        walk_t_string(self, t_string);
    }
    fn visit_string_literal(&self, string_literal: &mut StringLiteral<'ast>) {
        walk_string_literal(self, string_literal);
    }
    fn visit_bytes_literal(&self, bytes_literal: &mut BytesLiteral<'ast>) {
        walk_bytes_literal(self, bytes_literal);
    }
}

fn transform_box<'ast, T, V, F>(visitor: &V, value: &mut ArenaBox<'ast, T>, transform: F)
where
    T: Clone,
    V: Transformer<'ast> + ?Sized,
    F: FnOnce(&V, &mut T),
{
    // Arena-backed child nodes are immutable and may be shared by cloned syntax. Rewrites
    // therefore path-copy each traversed boxed child into the same arena before mutating it.
    let mut owned = (**value).clone();
    transform(visitor, &mut owned);
    *value = ArenaBox::new_in(owned, visitor.allocator());
}

fn transform_vec<'ast, T, V, F>(visitor: &V, value: &mut ArenaSlice<'ast, T>, mut transform: F)
where
    T: Clone,
    V: Transformer<'ast> + ?Sized,
    F: FnMut(&V, &mut T),
{
    value.transform_in(visitor.allocator(), |values| {
        for value in values {
            transform(visitor, value);
        }
    });
}

pub fn walk_body<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    body: &mut ArenaSlice<'ast, Stmt<'ast>>,
) {
    transform_vec(visitor, body, V::visit_stmt);
}

pub fn walk_elif_else_clause<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    elif_else_clause: &mut ElifElseClause<'ast>,
) {
    if let Some(test) = &mut elif_else_clause.test {
        visitor.visit_expr(test);
    }
    visitor.visit_body(&mut elif_else_clause.body);
}

pub fn walk_stmt<'ast, V: Transformer<'ast> + ?Sized>(visitor: &V, stmt: &mut Stmt<'ast>) {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef {
            parameters,
            body,
            decorator_list,
            returns,
            type_params,
            ..
        }) => {
            transform_vec(visitor, decorator_list, V::visit_decorator);
            if let Some(type_params) = type_params {
                transform_box(visitor, type_params, V::visit_type_params);
            }
            transform_box(visitor, parameters, V::visit_parameters);
            if let Some(expr) = returns {
                transform_box(visitor, expr, V::visit_annotation);
            }
            visitor.visit_body(body);
        }
        Stmt::ClassDef(ast::StmtClassDef {
            arguments,
            body,
            decorator_list,
            type_params,
            ..
        }) => {
            transform_vec(visitor, decorator_list, V::visit_decorator);
            if let Some(type_params) = type_params {
                transform_box(visitor, type_params, V::visit_type_params);
            }
            if let Some(arguments) = arguments {
                transform_box(visitor, arguments, V::visit_arguments);
            }
            visitor.visit_body(body);
        }
        Stmt::Return(ast::StmtReturn {
            value,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = value {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        Stmt::Delete(ast::StmtDelete {
            targets,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, targets, V::visit_expr);
        }
        Stmt::TypeAlias(ast::StmtTypeAlias {
            range: _,
            node_index: _,
            name,
            type_params,
            value,
        }) => {
            transform_box(visitor, value, V::visit_expr);
            if let Some(type_params) = type_params {
                transform_box(visitor, type_params, V::visit_type_params);
            }
            transform_box(visitor, name, V::visit_expr);
        }
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            transform_box(visitor, value, V::visit_expr);
            transform_vec(visitor, targets, V::visit_expr);
        }
        Stmt::AugAssign(ast::StmtAugAssign {
            target,
            op,
            value,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, value, V::visit_expr);
            visitor.visit_operator(op);
            transform_box(visitor, target, V::visit_expr);
        }
        Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            annotation,
            value,
            ..
        }) => {
            if let Some(expr) = value {
                transform_box(visitor, expr, V::visit_expr);
            }
            transform_box(visitor, annotation, V::visit_annotation);
            transform_box(visitor, target, V::visit_expr);
        }
        Stmt::For(ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        }) => {
            transform_box(visitor, iter, V::visit_expr);
            transform_box(visitor, target, V::visit_expr);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::While(ast::StmtWhile {
            test,
            body,
            orelse,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, test, V::visit_expr);
            visitor.visit_body(body);
            visitor.visit_body(orelse);
        }
        Stmt::If(ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, test, V::visit_expr);
            visitor.visit_body(body);
            transform_vec(visitor, elif_else_clauses, |visitor, clause| {
                walk_elif_else_clause(visitor, clause);
            });
        }
        Stmt::With(ast::StmtWith { items, body, .. }) => {
            transform_vec(visitor, items, V::visit_with_item);
            visitor.visit_body(body);
        }
        Stmt::Match(ast::StmtMatch {
            subject,
            cases,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, subject, V::visit_expr);
            transform_vec(visitor, cases, V::visit_match_case);
        }
        Stmt::Raise(ast::StmtRaise {
            exc,
            cause,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = exc {
                transform_box(visitor, expr, V::visit_expr);
            }
            if let Some(expr) = cause {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
            node_index: _,
        }) => {
            visitor.visit_body(body);
            transform_vec(visitor, handlers, V::visit_except_handler);
            visitor.visit_body(orelse);
            visitor.visit_body(finalbody);
        }
        Stmt::Assert(ast::StmtAssert {
            test,
            msg,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, test, V::visit_expr);
            if let Some(expr) = msg {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        Stmt::Import(ast::StmtImport {
            names,
            is_lazy: _,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, names, V::visit_alias);
        }
        Stmt::ImportFrom(ast::StmtImportFrom { names, .. }) => {
            transform_vec(visitor, names, V::visit_alias);
        }
        Stmt::Global(_) => {}
        Stmt::Nonlocal(_) => {}
        Stmt::Expr(ast::StmtExpr {
            value,
            range: _,
            node_index: _,
        }) => transform_box(visitor, value, V::visit_expr),
        Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) | Stmt::IpyEscapeCommand(_) => {}
    }
}

pub fn walk_annotation<'ast, V: Transformer<'ast> + ?Sized>(visitor: &V, expr: &mut Expr<'ast>) {
    visitor.visit_expr(expr);
}

pub fn walk_decorator<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    decorator: &mut Decorator<'ast>,
) {
    visitor.visit_expr(&mut decorator.expression);
}

pub fn walk_expr<'ast, V: Transformer<'ast> + ?Sized>(visitor: &V, expr: &mut Expr<'ast>) {
    match expr {
        Expr::BoolOp(ast::ExprBoolOp {
            op,
            values,
            range: _,
            node_index: _,
        }) => {
            visitor.visit_bool_op(op);
            transform_vec(visitor, values, V::visit_expr);
        }
        Expr::Named(ast::ExprNamed {
            target,
            value,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, value, V::visit_expr);
            transform_box(visitor, target, V::visit_expr);
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            op,
            right,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, left, V::visit_expr);
            visitor.visit_operator(op);
            transform_box(visitor, right, V::visit_expr);
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op,
            operand,
            range: _,
            node_index: _,
        }) => {
            visitor.visit_unary_op(op);
            transform_box(visitor, operand, V::visit_expr);
        }
        Expr::Lambda(ast::ExprLambda {
            parameters,
            body,
            range: _,
            node_index: _,
        }) => {
            if let Some(parameters) = parameters {
                transform_box(visitor, parameters, V::visit_parameters);
            }
            transform_box(visitor, body, V::visit_expr);
        }
        Expr::If(ast::ExprIf {
            test,
            body,
            orelse,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, test, V::visit_expr);
            transform_box(visitor, body, V::visit_expr);
            transform_box(visitor, orelse, V::visit_expr);
        }
        Expr::Dict(ast::ExprDict {
            items,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, items, |visitor, ast::DictItem { key, value }| {
                if let Some(key) = key {
                    visitor.visit_expr(key);
                }
                visitor.visit_expr(value);
            });
        }
        Expr::Set(ast::ExprSet {
            elts,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, elts, V::visit_expr);
        }
        Expr::ListComp(ast::ExprListComp {
            elt,
            generators,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, generators, V::visit_comprehension);
            transform_box(visitor, elt, V::visit_expr);
        }
        Expr::SetComp(ast::ExprSetComp {
            elt,
            generators,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, generators, V::visit_comprehension);
            transform_box(visitor, elt, V::visit_expr);
        }
        Expr::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, generators, V::visit_comprehension);
            if let Some(key) = key {
                transform_box(visitor, key, V::visit_expr);
            }
            transform_box(visitor, value, V::visit_expr);
        }
        Expr::Generator(ast::ExprGenerator {
            elt,
            generators,
            range: _,
            node_index: _,
            parenthesized: _,
        }) => {
            transform_vec(visitor, generators, V::visit_comprehension);
            transform_box(visitor, elt, V::visit_expr);
        }
        Expr::Await(ast::ExprAwait {
            value,
            range: _,
            node_index: _,
        }) => transform_box(visitor, value, V::visit_expr),
        Expr::Yield(ast::ExprYield {
            value,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = value {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        Expr::YieldFrom(ast::ExprYieldFrom {
            value,
            range: _,
            node_index: _,
        }) => transform_box(visitor, value, V::visit_expr),
        Expr::Compare(ast::ExprCompare {
            left,
            ops,
            comparators,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, left, V::visit_expr);
            transform_vec(visitor, ops, V::visit_cmp_op);
            transform_vec(visitor, comparators, V::visit_expr);
        }
        Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, func, V::visit_expr);
            visitor.visit_arguments(arguments);
        }
        Expr::FString(ast::ExprFString { value, .. }) => {
            value.transform_in(visitor.allocator(), |f_string_part| match f_string_part {
                ast::FStringPart::Literal(string_literal) => {
                    visitor.visit_string_literal(string_literal);
                }
                ast::FStringPart::FString(f_string) => {
                    visitor.visit_f_string(f_string);
                }
            });
        }
        Expr::TString(ast::ExprTString { value, .. }) => {
            value.transform_in(visitor.allocator(), |t_string| {
                visitor.visit_t_string(t_string);
            });
        }
        Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
            value.transform_in(visitor.allocator(), |string_literal| {
                visitor.visit_string_literal(string_literal);
            });
        }
        Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
            value.transform_in(visitor.allocator(), |bytes_literal| {
                visitor.visit_bytes_literal(bytes_literal);
            });
        }
        Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_) => {}
        Expr::Attribute(ast::ExprAttribute { value, ctx, .. }) => {
            transform_box(visitor, value, V::visit_expr);
            visitor.visit_expr_context(ctx);
        }
        Expr::Subscript(ast::ExprSubscript {
            value,
            slice,
            ctx,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, value, V::visit_expr);
            transform_box(visitor, slice, V::visit_expr);
            visitor.visit_expr_context(ctx);
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx,
            range: _,
            node_index: _,
        }) => {
            transform_box(visitor, value, V::visit_expr);
            visitor.visit_expr_context(ctx);
        }
        Expr::Name(ast::ExprName { ctx, .. }) => {
            visitor.visit_expr_context(ctx);
        }
        Expr::List(ast::ExprList {
            elts,
            ctx,
            range: _,
            node_index: _,
        }) => {
            transform_vec(visitor, elts, V::visit_expr);
            visitor.visit_expr_context(ctx);
        }
        Expr::Tuple(ast::ExprTuple {
            elts,
            ctx,
            range: _,
            node_index: _,
            parenthesized: _,
        }) => {
            transform_vec(visitor, elts, V::visit_expr);
            visitor.visit_expr_context(ctx);
        }
        Expr::Slice(ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = lower {
                transform_box(visitor, expr, V::visit_expr);
            }
            if let Some(expr) = upper {
                transform_box(visitor, expr, V::visit_expr);
            }
            if let Some(expr) = step {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        Expr::IpyEscapeCommand(_) => {}
    }
}

pub fn walk_comprehension<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    comprehension: &mut Comprehension<'ast>,
) {
    visitor.visit_expr(&mut comprehension.iter);
    visitor.visit_expr(&mut comprehension.target);
    transform_vec(visitor, &mut comprehension.ifs, V::visit_expr);
}

pub fn walk_except_handler<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    except_handler: &mut ExceptHandler<'ast>,
) {
    match except_handler {
        ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, body, .. }) => {
            if let Some(expr) = type_ {
                transform_box(visitor, expr, V::visit_expr);
            }
            visitor.visit_body(body);
        }
    }
}

pub fn walk_arguments<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    arguments: &mut Arguments<'ast>,
) {
    // Note that there might be keywords before the last arg, e.g. in
    // f(*args, a=2, *args2, **kwargs)`, but we follow Python in evaluating first `args` and then
    // `keywords`. See also [Arguments::arguments_source_order`].
    transform_vec(visitor, &mut arguments.args, V::visit_expr);
    transform_vec(visitor, &mut arguments.keywords, V::visit_keyword);
}

pub fn walk_parameters<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    parameters: &mut Parameters<'ast>,
) {
    // Defaults are evaluated before annotations.
    parameters
        .posonlyargs
        .transform_in(visitor.allocator(), |args| {
            for arg in args {
                if let Some(default) = &mut arg.default {
                    transform_box(visitor, default, V::visit_expr);
                }
            }
        });
    parameters.args.transform_in(visitor.allocator(), |args| {
        for arg in args {
            if let Some(default) = &mut arg.default {
                transform_box(visitor, default, V::visit_expr);
            }
        }
    });
    parameters
        .kwonlyargs
        .transform_in(visitor.allocator(), |args| {
            for arg in args {
                if let Some(default) = &mut arg.default {
                    transform_box(visitor, default, V::visit_expr);
                }
            }
        });

    parameters
        .posonlyargs
        .transform_in(visitor.allocator(), |args| {
            for arg in args {
                visitor.visit_parameter(&mut arg.parameter);
            }
        });
    parameters.args.transform_in(visitor.allocator(), |args| {
        for arg in args {
            visitor.visit_parameter(&mut arg.parameter);
        }
    });
    if let Some(arg) = &mut parameters.vararg {
        transform_box(visitor, arg, V::visit_parameter);
    }
    parameters
        .kwonlyargs
        .transform_in(visitor.allocator(), |args| {
            for arg in args {
                visitor.visit_parameter(&mut arg.parameter);
            }
        });
    if let Some(arg) = &mut parameters.kwarg {
        transform_box(visitor, arg, V::visit_parameter);
    }
}

pub fn walk_parameter<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    parameter: &mut Parameter<'ast>,
) {
    if let Some(expr) = &mut parameter.annotation {
        transform_box(visitor, expr, V::visit_annotation);
    }
}

pub fn walk_keyword<'ast, V: Transformer<'ast> + ?Sized>(visitor: &V, keyword: &mut Keyword<'ast>) {
    visitor.visit_expr(&mut keyword.value);
}

pub fn walk_with_item<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    with_item: &mut WithItem<'ast>,
) {
    visitor.visit_expr(&mut with_item.context_expr);
    if let Some(expr) = &mut with_item.optional_vars {
        transform_box(visitor, expr, V::visit_expr);
    }
}

pub fn walk_type_params<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    type_params: &mut TypeParams<'ast>,
) {
    transform_vec(visitor, &mut type_params.type_params, V::visit_type_param);
}

pub fn walk_type_param<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    type_param: &mut TypeParam<'ast>,
) {
    match type_param {
        TypeParam::TypeVar(TypeParamTypeVar {
            bound,
            default,
            name: _,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = bound {
                transform_box(visitor, expr, V::visit_expr);
            }
            if let Some(expr) = default {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
            default,
            name: _,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = default {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
        TypeParam::ParamSpec(TypeParamParamSpec {
            default,
            name: _,
            range: _,
            node_index: _,
        }) => {
            if let Some(expr) = default {
                transform_box(visitor, expr, V::visit_expr);
            }
        }
    }
}

pub fn walk_match_case<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    match_case: &mut MatchCase<'ast>,
) {
    visitor.visit_pattern(&mut match_case.pattern);
    if let Some(expr) = &mut match_case.guard {
        transform_box(visitor, expr, V::visit_expr);
    }
    visitor.visit_body(&mut match_case.body);
}

pub fn walk_pattern<'ast, V: Transformer<'ast> + ?Sized>(visitor: &V, pattern: &mut Pattern<'ast>) {
    match pattern {
        Pattern::MatchValue(ast::PatternMatchValue { value, .. }) => {
            transform_box(visitor, value, V::visit_expr);
        }
        Pattern::MatchSingleton(_) => {}
        Pattern::MatchSequence(ast::PatternMatchSequence { patterns, .. }) => {
            transform_vec(visitor, patterns, V::visit_pattern);
        }
        Pattern::MatchMapping(ast::PatternMatchMapping { keys, patterns, .. }) => {
            transform_vec(visitor, keys, V::visit_expr);
            transform_vec(visitor, patterns, V::visit_pattern);
        }
        Pattern::MatchClass(ast::PatternMatchClass { cls, arguments, .. }) => {
            transform_box(visitor, cls, V::visit_expr);
            visitor.visit_pattern_arguments(arguments);
        }
        Pattern::MatchStar(_) => {}
        Pattern::MatchAs(ast::PatternMatchAs { pattern, .. }) => {
            if let Some(pattern) = pattern {
                transform_box(visitor, pattern, V::visit_pattern);
            }
        }
        Pattern::MatchOr(ast::PatternMatchOr { patterns, .. }) => {
            transform_vec(visitor, patterns, V::visit_pattern);
        }
    }
}

pub fn walk_pattern_arguments<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    pattern_arguments: &mut PatternArguments<'ast>,
) {
    transform_vec(visitor, &mut pattern_arguments.patterns, V::visit_pattern);
    transform_vec(
        visitor,
        &mut pattern_arguments.keywords,
        V::visit_pattern_keyword,
    );
}

pub fn walk_pattern_keyword<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    pattern_keyword: &mut PatternKeyword<'ast>,
) {
    visitor.visit_pattern(&mut pattern_keyword.pattern);
}

pub fn walk_f_string<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    f_string: &mut FString<'ast>,
) {
    f_string
        .elements
        .transform_in(visitor.allocator(), |element| {
            visitor.visit_interpolated_string_element(element);
        });
}

pub fn walk_interpolated_string_element<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    interpolated_string_element: &mut InterpolatedStringElement<'ast>,
) {
    if let ast::InterpolatedStringElement::Interpolation(ast::InterpolatedElement {
        expression,
        format_spec,
        ..
    }) = interpolated_string_element
    {
        transform_box(visitor, expression, V::visit_expr);
        if let Some(format_spec) = format_spec {
            transform_box(visitor, format_spec, |visitor, format_spec| {
                format_spec
                    .elements
                    .transform_in(visitor.allocator(), |element| {
                        visitor.visit_interpolated_string_element(element);
                    });
            });
        }
    }
}

pub fn walk_t_string<'ast, V: Transformer<'ast> + ?Sized>(
    visitor: &V,
    t_string: &mut TString<'ast>,
) {
    t_string
        .elements
        .transform_in(visitor.allocator(), |element| {
            visitor.visit_interpolated_string_element(element);
        });
}

pub fn walk_expr_context<'ast, V: Transformer<'ast> + ?Sized>(
    _visitor: &V,
    _expr_context: &mut ExprContext,
) {
}

pub fn walk_bool_op<'ast, V: Transformer<'ast> + ?Sized>(_visitor: &V, _bool_op: &mut BoolOp) {}

pub fn walk_operator<'ast, V: Transformer<'ast> + ?Sized>(_visitor: &V, _operator: &mut Operator) {}

pub fn walk_unary_op<'ast, V: Transformer<'ast> + ?Sized>(_visitor: &V, _unary_op: &mut UnaryOp) {}

pub fn walk_cmp_op<'ast, V: Transformer<'ast> + ?Sized>(_visitor: &V, _cmp_op: &mut CmpOp) {}

pub fn walk_alias<'ast, V: Transformer<'ast> + ?Sized>(_visitor: &V, _alias: &mut Alias) {}

pub fn walk_string_literal<'ast, V: Transformer<'ast> + ?Sized>(
    _visitor: &V,
    _string_literal: &mut StringLiteral<'ast>,
) {
}

pub fn walk_bytes_literal<'ast, V: Transformer<'ast> + ?Sized>(
    _visitor: &V,
    _bytes_literal: &mut BytesLiteral<'ast>,
) {
}
