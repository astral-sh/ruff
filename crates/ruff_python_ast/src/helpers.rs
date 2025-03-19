use std::borrow::Cow;
use std::path::Path;

use rustc_hash::FxHashMap;

use ruff_python_trivia::{indentation_at_offset, CommentRanges, SimpleTokenKind, SimpleTokenizer};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::name::{Name, QualifiedName, QualifiedNameBuilder};
use crate::parenthesize::parenthesized_range;
use crate::statement_visitor::StatementVisitor;
use crate::visitor::Visitor;
use crate::{
    self as ast, Arguments, CmpOp, DictItem, ExceptHandler, Expr, FStringElement, MatchCase,
    Operator, Pattern, Stmt, TypeParam,
};
use crate::{AnyNodeRef, ExprContext};

/// Return `true` if the `Stmt` is a compound statement (as opposed to a simple statement).
pub const fn is_compound_statement(stmt: &Stmt) -> bool {
    matches!(
        stmt,
        Stmt::FunctionDef(_)
            | Stmt::ClassDef(_)
            | Stmt::While(_)
            | Stmt::For(_)
            | Stmt::Match(_)
            | Stmt::With(_)
            | Stmt::If(_)
            | Stmt::Try(_)
    )
}

fn is_iterable_initializer<F>(id: &str, is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    matches!(id, "list" | "tuple" | "set" | "dict" | "frozenset") && is_builtin(id)
}

/// Return `true` if the `Expr` contains an expression that appears to include a
/// side-effect (like a function call).
///
/// Accepts a closure that determines whether a given name (e.g., `"list"`) is a Python builtin.
pub fn contains_effect<F>(expr: &Expr, is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    any_over_expr(expr, &|expr| {
        // Accept empty initializers.
        if let Expr::Call(ast::ExprCall {
            func,
            arguments,
            range: _,
        }) = expr
        {
            // Ex) `list()`
            if arguments.is_empty() {
                if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                    if !is_iterable_initializer(id.as_str(), |id| is_builtin(id)) {
                        return true;
                    }
                    return false;
                }
            }
        }

        // Avoid false positive for overloaded operators.
        if let Expr::BinOp(ast::ExprBinOp { left, right, .. }) = expr {
            if !matches!(
                left.as_ref(),
                Expr::StringLiteral(_)
                    | Expr::BytesLiteral(_)
                    | Expr::NumberLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
                    | Expr::EllipsisLiteral(_)
                    | Expr::FString(_)
                    | Expr::List(_)
                    | Expr::Tuple(_)
                    | Expr::Set(_)
                    | Expr::Dict(_)
                    | Expr::ListComp(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            ) {
                return true;
            }
            if !matches!(
                right.as_ref(),
                Expr::StringLiteral(_)
                    | Expr::BytesLiteral(_)
                    | Expr::NumberLiteral(_)
                    | Expr::BooleanLiteral(_)
                    | Expr::NoneLiteral(_)
                    | Expr::EllipsisLiteral(_)
                    | Expr::FString(_)
                    | Expr::List(_)
                    | Expr::Tuple(_)
                    | Expr::Set(_)
                    | Expr::Dict(_)
                    | Expr::ListComp(_)
                    | Expr::SetComp(_)
                    | Expr::DictComp(_)
            ) {
                return true;
            }
            return false;
        }

        // Otherwise, avoid all complex expressions.
        matches!(
            expr,
            Expr::Await(_)
                | Expr::Call(_)
                | Expr::DictComp(_)
                | Expr::Generator(_)
                | Expr::ListComp(_)
                | Expr::SetComp(_)
                | Expr::Subscript(_)
                | Expr::Yield(_)
                | Expr::YieldFrom(_)
                | Expr::IpyEscapeCommand(_)
        )
    })
}

/// Call `func` over every `Expr` in `expr`, returning `true` if any expression
/// returns `true`..
pub fn any_over_expr(expr: &Expr, func: &dyn Fn(&Expr) -> bool) -> bool {
    if func(expr) {
        return true;
    }
    match expr {
        Expr::BoolOp(ast::ExprBoolOp { values, .. }) => {
            values.iter().any(|expr| any_over_expr(expr, func))
        }
        Expr::FString(ast::ExprFString { value, .. }) => value
            .elements()
            .any(|expr| any_over_f_string_element(expr, func)),
        Expr::Named(ast::ExprNamed {
            target,
            value,
            range: _,
        }) => any_over_expr(target, func) || any_over_expr(value, func),
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            any_over_expr(left, func) || any_over_expr(right, func)
        }
        Expr::UnaryOp(ast::ExprUnaryOp { operand, .. }) => any_over_expr(operand, func),
        Expr::Lambda(ast::ExprLambda { body, .. }) => any_over_expr(body, func),
        Expr::If(ast::ExprIf {
            test,
            body,
            orelse,
            range: _,
        }) => any_over_expr(test, func) || any_over_expr(body, func) || any_over_expr(orelse, func),
        Expr::Dict(ast::ExprDict { items, range: _ }) => {
            items.iter().any(|ast::DictItem { key, value }| {
                any_over_expr(value, func)
                    || key.as_ref().is_some_and(|key| any_over_expr(key, func))
            })
        }
        Expr::Set(ast::ExprSet { elts, range: _ })
        | Expr::List(ast::ExprList { elts, range: _, .. })
        | Expr::Tuple(ast::ExprTuple { elts, range: _, .. }) => {
            elts.iter().any(|expr| any_over_expr(expr, func))
        }
        Expr::ListComp(ast::ExprListComp {
            elt,
            generators,
            range: _,
        })
        | Expr::SetComp(ast::ExprSetComp {
            elt,
            generators,
            range: _,
        })
        | Expr::Generator(ast::ExprGenerator {
            elt,
            generators,
            range: _,
            parenthesized: _,
        }) => {
            any_over_expr(elt, func)
                || generators.iter().any(|generator| {
                    any_over_expr(&generator.target, func)
                        || any_over_expr(&generator.iter, func)
                        || generator.ifs.iter().any(|expr| any_over_expr(expr, func))
                })
        }
        Expr::DictComp(ast::ExprDictComp {
            key,
            value,
            generators,
            range: _,
        }) => {
            any_over_expr(key, func)
                || any_over_expr(value, func)
                || generators.iter().any(|generator| {
                    any_over_expr(&generator.target, func)
                        || any_over_expr(&generator.iter, func)
                        || generator.ifs.iter().any(|expr| any_over_expr(expr, func))
                })
        }
        Expr::Await(ast::ExprAwait { value, range: _ })
        | Expr::YieldFrom(ast::ExprYieldFrom { value, range: _ })
        | Expr::Attribute(ast::ExprAttribute {
            value, range: _, ..
        })
        | Expr::Starred(ast::ExprStarred {
            value, range: _, ..
        }) => any_over_expr(value, func),
        Expr::Yield(ast::ExprYield { value, range: _ }) => value
            .as_ref()
            .is_some_and(|value| any_over_expr(value, func)),
        Expr::Compare(ast::ExprCompare {
            left, comparators, ..
        }) => any_over_expr(left, func) || comparators.iter().any(|expr| any_over_expr(expr, func)),
        Expr::Call(ast::ExprCall {
            func: call_func,
            arguments,
            range: _,
        }) => {
            any_over_expr(call_func, func)
                // Note that this is the evaluation order but not necessarily the declaration order
                // (e.g. for `f(*args, a=2, *args2, **kwargs)` it's not)
                || arguments.args.iter().any(|expr| any_over_expr(expr, func))
                || arguments.keywords
                    .iter()
                    .any(|keyword| any_over_expr(&keyword.value, func))
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            any_over_expr(value, func) || any_over_expr(slice, func)
        }
        Expr::Slice(ast::ExprSlice {
            lower,
            upper,
            step,
            range: _,
        }) => {
            lower
                .as_ref()
                .is_some_and(|value| any_over_expr(value, func))
                || upper
                    .as_ref()
                    .is_some_and(|value| any_over_expr(value, func))
                || step
                    .as_ref()
                    .is_some_and(|value| any_over_expr(value, func))
        }
        Expr::Name(_)
        | Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_)
        | Expr::IpyEscapeCommand(_) => false,
    }
}

pub fn any_over_type_param(type_param: &TypeParam, func: &dyn Fn(&Expr) -> bool) -> bool {
    match type_param {
        TypeParam::TypeVar(ast::TypeParamTypeVar { bound, default, .. }) => {
            bound
                .as_ref()
                .is_some_and(|value| any_over_expr(value, func))
                || default
                    .as_ref()
                    .is_some_and(|value| any_over_expr(value, func))
        }
        TypeParam::TypeVarTuple(ast::TypeParamTypeVarTuple { default, .. }) => default
            .as_ref()
            .is_some_and(|value| any_over_expr(value, func)),
        TypeParam::ParamSpec(ast::TypeParamParamSpec { default, .. }) => default
            .as_ref()
            .is_some_and(|value| any_over_expr(value, func)),
    }
}

pub fn any_over_pattern(pattern: &Pattern, func: &dyn Fn(&Expr) -> bool) -> bool {
    match pattern {
        Pattern::MatchValue(ast::PatternMatchValue { value, range: _ }) => {
            any_over_expr(value, func)
        }
        Pattern::MatchSingleton(_) => false,
        Pattern::MatchSequence(ast::PatternMatchSequence { patterns, range: _ }) => patterns
            .iter()
            .any(|pattern| any_over_pattern(pattern, func)),
        Pattern::MatchMapping(ast::PatternMatchMapping { keys, patterns, .. }) => {
            keys.iter().any(|key| any_over_expr(key, func))
                || patterns
                    .iter()
                    .any(|pattern| any_over_pattern(pattern, func))
        }
        Pattern::MatchClass(ast::PatternMatchClass { cls, arguments, .. }) => {
            any_over_expr(cls, func)
                || arguments
                    .patterns
                    .iter()
                    .any(|pattern| any_over_pattern(pattern, func))
                || arguments
                    .keywords
                    .iter()
                    .any(|keyword| any_over_pattern(&keyword.pattern, func))
        }
        Pattern::MatchStar(_) => false,
        Pattern::MatchAs(ast::PatternMatchAs { pattern, .. }) => pattern
            .as_ref()
            .is_some_and(|pattern| any_over_pattern(pattern, func)),
        Pattern::MatchOr(ast::PatternMatchOr { patterns, range: _ }) => patterns
            .iter()
            .any(|pattern| any_over_pattern(pattern, func)),
    }
}

pub fn any_over_f_string_element(
    element: &ast::FStringElement,
    func: &dyn Fn(&Expr) -> bool,
) -> bool {
    match element {
        ast::FStringElement::Literal(_) => false,
        ast::FStringElement::Expression(ast::FStringExpressionElement {
            expression,
            format_spec,
            ..
        }) => {
            any_over_expr(expression, func)
                || format_spec.as_ref().is_some_and(|spec| {
                    spec.elements
                        .iter()
                        .any(|spec_element| any_over_f_string_element(spec_element, func))
                })
        }
    }
}

pub fn any_over_stmt(stmt: &Stmt, func: &dyn Fn(&Expr) -> bool) -> bool {
    match stmt {
        Stmt::FunctionDef(ast::StmtFunctionDef {
            parameters,
            type_params,
            body,
            decorator_list,
            returns,
            ..
        }) => {
            parameters.iter().any(|param| {
                param
                    .default()
                    .is_some_and(|default| any_over_expr(default, func))
                    || param
                        .annotation()
                        .is_some_and(|annotation| any_over_expr(annotation, func))
            }) || type_params.as_ref().is_some_and(|type_params| {
                type_params
                    .iter()
                    .any(|type_param| any_over_type_param(type_param, func))
            }) || body.iter().any(|stmt| any_over_stmt(stmt, func))
                || decorator_list
                    .iter()
                    .any(|decorator| any_over_expr(&decorator.expression, func))
                || returns
                    .as_ref()
                    .is_some_and(|value| any_over_expr(value, func))
        }
        Stmt::ClassDef(ast::StmtClassDef {
            arguments,
            type_params,
            body,
            decorator_list,
            ..
        }) => {
            // Note that e.g. `class A(*args, a=2, *args2, **kwargs): pass` is a valid class
            // definition
            arguments
                .as_deref()
                .is_some_and(|Arguments { args, keywords, .. }| {
                    args.iter().any(|expr| any_over_expr(expr, func))
                        || keywords
                            .iter()
                            .any(|keyword| any_over_expr(&keyword.value, func))
                })
                || type_params.as_ref().is_some_and(|type_params| {
                    type_params
                        .iter()
                        .any(|type_param| any_over_type_param(type_param, func))
                })
                || body.iter().any(|stmt| any_over_stmt(stmt, func))
                || decorator_list
                    .iter()
                    .any(|decorator| any_over_expr(&decorator.expression, func))
        }
        Stmt::Return(ast::StmtReturn { value, range: _ }) => value
            .as_ref()
            .is_some_and(|value| any_over_expr(value, func)),
        Stmt::Delete(ast::StmtDelete { targets, range: _ }) => {
            targets.iter().any(|expr| any_over_expr(expr, func))
        }
        Stmt::TypeAlias(ast::StmtTypeAlias {
            name,
            type_params,
            value,
            ..
        }) => {
            any_over_expr(name, func)
                || type_params.as_ref().is_some_and(|type_params| {
                    type_params
                        .iter()
                        .any(|type_param| any_over_type_param(type_param, func))
                })
                || any_over_expr(value, func)
        }
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            targets.iter().any(|expr| any_over_expr(expr, func)) || any_over_expr(value, func)
        }
        Stmt::AugAssign(ast::StmtAugAssign { target, value, .. }) => {
            any_over_expr(target, func) || any_over_expr(value, func)
        }
        Stmt::AnnAssign(ast::StmtAnnAssign {
            target,
            annotation,
            value,
            ..
        }) => {
            any_over_expr(target, func)
                || any_over_expr(annotation, func)
                || value
                    .as_ref()
                    .is_some_and(|value| any_over_expr(value, func))
        }
        Stmt::For(ast::StmtFor {
            target,
            iter,
            body,
            orelse,
            ..
        }) => {
            any_over_expr(target, func)
                || any_over_expr(iter, func)
                || any_over_body(body, func)
                || any_over_body(orelse, func)
        }
        Stmt::While(ast::StmtWhile {
            test,
            body,
            orelse,
            range: _,
        }) => any_over_expr(test, func) || any_over_body(body, func) || any_over_body(orelse, func),
        Stmt::If(ast::StmtIf {
            test,
            body,
            elif_else_clauses,
            range: _,
        }) => {
            any_over_expr(test, func)
                || any_over_body(body, func)
                || elif_else_clauses.iter().any(|clause| {
                    clause
                        .test
                        .as_ref()
                        .is_some_and(|test| any_over_expr(test, func))
                        || any_over_body(&clause.body, func)
                })
        }
        Stmt::With(ast::StmtWith { items, body, .. }) => {
            items.iter().any(|with_item| {
                any_over_expr(&with_item.context_expr, func)
                    || with_item
                        .optional_vars
                        .as_ref()
                        .is_some_and(|expr| any_over_expr(expr, func))
            }) || any_over_body(body, func)
        }
        Stmt::Raise(ast::StmtRaise {
            exc,
            cause,
            range: _,
        }) => {
            exc.as_ref().is_some_and(|value| any_over_expr(value, func))
                || cause
                    .as_ref()
                    .is_some_and(|value| any_over_expr(value, func))
        }
        Stmt::Try(ast::StmtTry {
            body,
            handlers,
            orelse,
            finalbody,
            is_star: _,
            range: _,
        }) => {
            any_over_body(body, func)
                || handlers.iter().any(|handler| {
                    let ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler {
                        type_,
                        body,
                        ..
                    }) = handler;
                    type_.as_ref().is_some_and(|expr| any_over_expr(expr, func))
                        || any_over_body(body, func)
                })
                || any_over_body(orelse, func)
                || any_over_body(finalbody, func)
        }
        Stmt::Assert(ast::StmtAssert {
            test,
            msg,
            range: _,
        }) => {
            any_over_expr(test, func)
                || msg.as_ref().is_some_and(|value| any_over_expr(value, func))
        }
        Stmt::Match(ast::StmtMatch {
            subject,
            cases,
            range: _,
        }) => {
            any_over_expr(subject, func)
                || cases.iter().any(|case| {
                    let MatchCase {
                        pattern,
                        guard,
                        body,
                        range: _,
                    } = case;
                    any_over_pattern(pattern, func)
                        || guard.as_ref().is_some_and(|expr| any_over_expr(expr, func))
                        || any_over_body(body, func)
                })
        }
        Stmt::Import(_) => false,
        Stmt::ImportFrom(_) => false,
        Stmt::Global(_) => false,
        Stmt::Nonlocal(_) => false,
        Stmt::Expr(ast::StmtExpr { value, range: _ }) => any_over_expr(value, func),
        Stmt::Pass(_) | Stmt::Break(_) | Stmt::Continue(_) => false,
        Stmt::IpyEscapeCommand(_) => false,
    }
}

pub fn any_over_body(body: &[Stmt], func: &dyn Fn(&Expr) -> bool) -> bool {
    body.iter().any(|stmt| any_over_stmt(stmt, func))
}

pub fn is_dunder(id: &str) -> bool {
    id.starts_with("__") && id.ends_with("__")
}

/// Whether a name starts and ends with a single underscore.
///
/// `_a__` is considered neither a dunder nor a sunder name.
pub fn is_sunder(id: &str) -> bool {
    id.starts_with('_') && id.ends_with('_') && !id.starts_with("__") && !id.ends_with("__")
}

/// Return `true` if the [`Stmt`] is an assignment to a dunder (like `__all__`).
pub fn is_assignment_to_a_dunder(stmt: &Stmt) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type
    // annotation. This is what pycodestyle (as of 2.9.1) does.
    match stmt {
        Stmt::Assign(ast::StmtAssign { targets, .. }) => {
            if let [Expr::Name(ast::ExprName { id, .. })] = targets.as_slice() {
                is_dunder(id)
            } else {
                false
            }
        }
        Stmt::AnnAssign(ast::StmtAnnAssign { target, .. }) => {
            if let Expr::Name(ast::ExprName { id, .. }) = target.as_ref() {
                is_dunder(id)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Return `true` if the [`Expr`] is a singleton (`None`, `True`, `False`, or
/// `...`).
pub const fn is_singleton(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::NoneLiteral(_) | Expr::BooleanLiteral(_) | Expr::EllipsisLiteral(_)
    )
}

/// Return `true` if the [`Expr`] is a literal or tuple of literals.
pub fn is_constant(expr: &Expr) -> bool {
    if let Expr::Tuple(tuple) = expr {
        tuple.iter().all(is_constant)
    } else {
        expr.is_literal_expr()
    }
}

/// Return `true` if the [`Expr`] is a non-singleton constant.
pub fn is_constant_non_singleton(expr: &Expr) -> bool {
    is_constant(expr) && !is_singleton(expr)
}

/// Return `true` if an [`Expr`] is a literal `True`.
pub const fn is_const_true(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: true, .. }),
    )
}

/// Return `true` if an [`Expr`] is a literal `False`.
pub const fn is_const_false(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::BooleanLiteral(ast::ExprBooleanLiteral { value: false, .. }),
    )
}

/// Return `true` if the [`Expr`] is a mutable iterable initializer, like `{}` or `[]`.
pub const fn is_mutable_iterable_initializer(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Set(_)
            | Expr::SetComp(_)
            | Expr::List(_)
            | Expr::ListComp(_)
            | Expr::Dict(_)
            | Expr::DictComp(_)
    )
}

/// Extract the names of all handled exceptions.
pub fn extract_handled_exceptions(handlers: &[ExceptHandler]) -> Vec<&Expr> {
    let mut handled_exceptions = Vec::new();
    for handler in handlers {
        match handler {
            ExceptHandler::ExceptHandler(ast::ExceptHandlerExceptHandler { type_, .. }) => {
                if let Some(type_) = type_ {
                    if let Expr::Tuple(tuple) = &**type_ {
                        for type_ in tuple {
                            handled_exceptions.push(type_);
                        }
                    } else {
                        handled_exceptions.push(type_);
                    }
                }
            }
        }
    }
    handled_exceptions
}

/// Given an [`Expr`] that can be callable or not (like a decorator, which could
/// be used with or without explicit call syntax), return the underlying
/// callable.
pub fn map_callable(decorator: &Expr) -> &Expr {
    if let Expr::Call(ast::ExprCall { func, .. }) = decorator {
        // Ex) `@decorator()`
        func
    } else {
        // Ex) `@decorator`
        decorator
    }
}

/// Given an [`Expr`] that can be a [`ExprSubscript`][ast::ExprSubscript] or not
/// (like an annotation that may be generic or not), return the underlying expr.
pub fn map_subscript(expr: &Expr) -> &Expr {
    if let Expr::Subscript(ast::ExprSubscript { value, .. }) = expr {
        // Ex) `Iterable[T]`  => return `Iterable`
        value
    } else {
        // Ex) `Iterable`  => return `Iterable`
        expr
    }
}

/// Given an [`Expr`] that can be starred, return the underlying starred expression.
pub fn map_starred(expr: &Expr) -> &Expr {
    if let Expr::Starred(ast::ExprStarred { value, .. }) = expr {
        // Ex) `*args`
        value
    } else {
        // Ex) `args`
        expr
    }
}

/// Return `true` if the body uses `locals()`, `globals()`, `vars()`, `eval()`.
///
/// Accepts a closure that determines whether a given name (e.g., `"list"`) is a Python builtin.
pub fn uses_magic_variable_access<F>(body: &[Stmt], is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    any_over_body(body, &|expr| {
        if let Expr::Call(ast::ExprCall { func, .. }) = expr {
            if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                if matches!(id.as_str(), "locals" | "globals" | "vars" | "exec" | "eval") {
                    if is_builtin(id.as_str()) {
                        return true;
                    }
                }
            }
        }
        false
    })
}

/// Format the module reference name for a relative import.
///
/// # Examples
///
/// ```rust
/// # use ruff_python_ast::helpers::format_import_from;
///
/// assert_eq!(format_import_from(0, None), "".to_string());
/// assert_eq!(format_import_from(1, None), ".".to_string());
/// assert_eq!(format_import_from(1, Some("foo")), ".foo".to_string());
/// ```
pub fn format_import_from(level: u32, module: Option<&str>) -> Cow<str> {
    match (level, module) {
        (0, Some(module)) => Cow::Borrowed(module),
        (level, module) => {
            let mut module_name =
                String::with_capacity((level as usize) + module.map_or(0, str::len));
            for _ in 0..level {
                module_name.push('.');
            }
            if let Some(module) = module {
                module_name.push_str(module);
            }
            Cow::Owned(module_name)
        }
    }
}

/// Format the member reference name for a relative import.
///
/// # Examples
///
/// ```rust
/// # use ruff_python_ast::helpers::format_import_from_member;
///
/// assert_eq!(format_import_from_member(0, None, "bar"), "bar".to_string());
/// assert_eq!(format_import_from_member(1, None, "bar"), ".bar".to_string());
/// assert_eq!(format_import_from_member(1, Some("foo"), "bar"), ".foo.bar".to_string());
/// ```
pub fn format_import_from_member(level: u32, module: Option<&str>, member: &str) -> String {
    let mut qualified_name =
        String::with_capacity((level as usize) + module.map_or(0, str::len) + 1 + member.len());
    if level > 0 {
        for _ in 0..level {
            qualified_name.push('.');
        }
    }
    if let Some(module) = module {
        qualified_name.push_str(module);
        qualified_name.push('.');
    }
    qualified_name.push_str(member);
    qualified_name
}

/// Create a module path from a (package, path) pair.
///
/// For example, if the package is `foo/bar` and the path is `foo/bar/baz.py`,
/// the call path is `["baz"]`.
pub fn to_module_path(package: &Path, path: &Path) -> Option<Vec<String>> {
    path.strip_prefix(package.parent()?)
        .ok()?
        .iter()
        .map(Path::new)
        .map(Path::file_stem)
        .map(|path| path.and_then(|path| path.to_os_string().into_string().ok()))
        .collect::<Option<Vec<String>>>()
}

/// Format the call path for a relative import.
///
/// # Examples
///
/// ```rust
/// # use ruff_python_ast::helpers::collect_import_from_member;
///
/// assert_eq!(collect_import_from_member(0, None, "bar").segments(), ["bar"]);
/// assert_eq!(collect_import_from_member(1, None, "bar").segments(), [".", "bar"]);
/// assert_eq!(collect_import_from_member(1, Some("foo"), "bar").segments(), [".", "foo", "bar"]);
/// ```
pub fn collect_import_from_member<'a>(
    level: u32,
    module: Option<&'a str>,
    member: &'a str,
) -> QualifiedName<'a> {
    let mut qualified_name_builder = QualifiedNameBuilder::with_capacity(
        level as usize
            + module
                .map(|module| module.split('.').count())
                .unwrap_or_default()
            + 1,
    );

    // Include the dots as standalone segments.
    if level > 0 {
        for _ in 0..level {
            qualified_name_builder.push(".");
        }
    }

    // Add the remaining segments.
    if let Some(module) = module {
        qualified_name_builder.extend(module.split('.'));
    }

    // Add the member.
    qualified_name_builder.push(member);

    qualified_name_builder.build()
}

/// Format the call path for a relative import, or `None` if the relative import extends beyond
/// the root module.
pub fn from_relative_import<'a>(
    // The path from which the import is relative.
    module: &'a [String],
    // The path of the import itself (e.g., given `from ..foo import bar`, `[".", ".", "foo", "bar]`).
    import: &[&'a str],
    // The remaining segments to the call path (e.g., given `bar.baz`, `["baz"]`).
    tail: &[&'a str],
) -> Option<QualifiedName<'a>> {
    let mut qualified_name_builder =
        QualifiedNameBuilder::with_capacity(module.len() + import.len() + tail.len());

    // Start with the module path.
    qualified_name_builder.extend(module.iter().map(String::as_str));

    // Remove segments based on the number of dots.
    for segment in import {
        if *segment == "." {
            if qualified_name_builder.is_empty() {
                return None;
            }
            qualified_name_builder.pop();
        } else {
            qualified_name_builder.push(segment);
        }
    }

    // Add the remaining segments.
    qualified_name_builder.extend_from_slice(tail);

    Some(qualified_name_builder.build())
}

/// Given an imported module (based on its relative import level and module name), return the
/// fully-qualified module path.
pub fn resolve_imported_module_path<'a>(
    level: u32,
    module: Option<&'a str>,
    module_path: Option<&[String]>,
) -> Option<Cow<'a, str>> {
    if level == 0 {
        return Some(Cow::Borrowed(module.unwrap_or("")));
    }

    let module_path = module_path?;

    if level as usize >= module_path.len() {
        return None;
    }

    let mut qualified_path = module_path[..module_path.len() - level as usize].join(".");
    if let Some(module) = module {
        if !qualified_path.is_empty() {
            qualified_path.push('.');
        }
        qualified_path.push_str(module);
    }
    Some(Cow::Owned(qualified_path))
}

/// A [`Visitor`] to collect all [`Expr::Name`] nodes in an AST.
#[derive(Debug, Default)]
pub struct NameFinder<'a> {
    /// A map from identifier to defining expression.
    pub names: FxHashMap<&'a str, &'a ast::ExprName>,
}

impl<'a> Visitor<'a> for NameFinder<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(name) = expr {
            self.names.insert(&name.id, name);
        }
        crate::visitor::walk_expr(self, expr);
    }
}

/// A [`Visitor`] to collect all stored [`Expr::Name`] nodes in an AST.
#[derive(Debug, Default)]
pub struct StoredNameFinder<'a> {
    /// A map from identifier to defining expression.
    pub names: FxHashMap<&'a str, &'a ast::ExprName>,
}

impl<'a> Visitor<'a> for StoredNameFinder<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Name(name) = expr {
            if name.ctx.is_store() {
                self.names.insert(&name.id, name);
            }
        }
        crate::visitor::walk_expr(self, expr);
    }
}

/// A [`Visitor`] that collects all `return` statements in a function or method.
#[derive(Default)]
pub struct ReturnStatementVisitor<'a> {
    pub returns: Vec<&'a ast::StmtReturn>,
    pub is_generator: bool,
}

impl<'a> Visitor<'a> for ReturnStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => {
                // Don't recurse.
            }
            Stmt::Return(stmt) => self.returns.push(stmt),
            _ => crate::visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &'a Expr) {
        if let Expr::Yield(_) | Expr::YieldFrom(_) = expr {
            self.is_generator = true;
        } else {
            crate::visitor::walk_expr(self, expr);
        }
    }
}

/// A [`StatementVisitor`] that collects all `raise` statements in a function or method.
#[derive(Default)]
pub struct RaiseStatementVisitor<'a> {
    pub raises: Vec<(TextRange, Option<&'a Expr>, Option<&'a Expr>)>,
}

impl<'a> StatementVisitor<'a> for RaiseStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match stmt {
            Stmt::Raise(ast::StmtRaise {
                exc,
                cause,
                range: _,
            }) => {
                self.raises
                    .push((stmt.range(), exc.as_deref(), cause.as_deref()));
            }
            Stmt::ClassDef(_) | Stmt::FunctionDef(_) | Stmt::Try(_) => {}
            Stmt::If(ast::StmtIf {
                body,
                elif_else_clauses,
                ..
            }) => {
                crate::statement_visitor::walk_body(self, body);
                for clause in elif_else_clauses {
                    self.visit_elif_else_clause(clause);
                }
            }
            Stmt::While(ast::StmtWhile { body, .. })
            | Stmt::With(ast::StmtWith { body, .. })
            | Stmt::For(ast::StmtFor { body, .. }) => {
                crate::statement_visitor::walk_body(self, body);
            }
            Stmt::Match(ast::StmtMatch { cases, .. }) => {
                for case in cases {
                    crate::statement_visitor::walk_body(self, &case.body);
                }
            }
            _ => {}
        }
    }
}

/// A [`Visitor`] that detects the presence of `await` expressions in the current scope.
#[derive(Debug, Default)]
pub struct AwaitVisitor {
    pub seen_await: bool,
}

impl Visitor<'_> for AwaitVisitor {
    fn visit_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::FunctionDef(_) | Stmt::ClassDef(_) => (),
            Stmt::With(ast::StmtWith { is_async: true, .. }) => {
                self.seen_await = true;
            }
            Stmt::For(ast::StmtFor { is_async: true, .. }) => {
                self.seen_await = true;
            }
            _ => crate::visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: &Expr) {
        if let Expr::Await(ast::ExprAwait { .. }) = expr {
            self.seen_await = true;
        } else {
            crate::visitor::walk_expr(self, expr);
        }
    }

    fn visit_comprehension(&mut self, comprehension: &'_ crate::Comprehension) {
        if comprehension.is_async {
            self.seen_await = true;
        } else {
            crate::visitor::walk_comprehension(self, comprehension);
        }
    }
}

/// Return `true` if a `Stmt` is a docstring.
pub fn is_docstring_stmt(stmt: &Stmt) -> bool {
    if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = stmt {
        value.is_string_literal_expr()
    } else {
        false
    }
}

/// Check if a node is part of a conditional branch.
pub fn on_conditional_branch<'a>(parents: &mut impl Iterator<Item = &'a Stmt>) -> bool {
    parents.any(|parent| {
        if matches!(parent, Stmt::If(_) | Stmt::While(_) | Stmt::Match(_)) {
            return true;
        }
        if let Stmt::Expr(ast::StmtExpr { value, range: _ }) = parent {
            if value.is_if_expr() {
                return true;
            }
        }
        false
    })
}

/// Check if a node is in a nested block.
pub fn in_nested_block<'a>(mut parents: impl Iterator<Item = &'a Stmt>) -> bool {
    parents.any(|parent| {
        matches!(
            parent,
            Stmt::Try(_) | Stmt::If(_) | Stmt::With(_) | Stmt::Match(_)
        )
    })
}

/// Check if a node represents an unpacking assignment.
pub fn is_unpacking_assignment(parent: &Stmt, child: &Expr) -> bool {
    match parent {
        Stmt::With(ast::StmtWith { items, .. }) => items.iter().any(|item| {
            if let Some(optional_vars) = &item.optional_vars {
                if optional_vars.is_tuple_expr() {
                    if any_over_expr(optional_vars, &|expr| expr == child) {
                        return true;
                    }
                }
            }
            false
        }),
        Stmt::Assign(ast::StmtAssign { targets, value, .. }) => {
            // In `(a, b) = (1, 2)`, `(1, 2)` is the target, and it is a tuple.
            let value_is_tuple = matches!(
                value.as_ref(),
                Expr::Set(_) | Expr::List(_) | Expr::Tuple(_)
            );
            // In `(a, b) = coords = (1, 2)`, `(a, b)` and `coords` are the targets, and
            // `(a, b)` is a tuple. (We use "tuple" as a placeholder for any
            // unpackable expression.)
            let targets_are_tuples = targets
                .iter()
                .all(|item| matches!(item, Expr::Set(_) | Expr::List(_) | Expr::Tuple(_)));
            // If we're looking at `a` in `(a, b) = coords = (1, 2)`, then we should
            // identify that the current expression is in a tuple.
            let child_in_tuple = targets_are_tuples
                || targets.iter().any(|item| {
                    matches!(item, Expr::Set(_) | Expr::List(_) | Expr::Tuple(_))
                        && any_over_expr(item, &|expr| expr == child)
                });

            // If our child is a tuple, and value is not, it's always an unpacking
            // expression. Ex) `x, y = tup`
            if child_in_tuple && !value_is_tuple {
                return true;
            }

            // If our child isn't a tuple, but value is, it's never an unpacking expression.
            // Ex) `coords = (1, 2)`
            if !child_in_tuple && value_is_tuple {
                return false;
            }

            // If our target and the value are both tuples, then it's an unpacking
            // expression assuming there's at least one non-tuple child.
            // Ex) Given `(x, y) = coords = 1, 2`, `(x, y)` is considered an unpacking
            // expression. Ex) Given `(x, y) = (a, b) = 1, 2`, `(x, y)` isn't
            // considered an unpacking expression.
            if child_in_tuple && value_is_tuple {
                return !targets_are_tuples;
            }

            false
        }
        _ => false,
    }
}

#[derive(Copy, Clone, Debug, PartialEq, is_macro::Is)]
pub enum Truthiness {
    /// The expression is `True`.
    True,
    /// The expression is `False`.
    False,
    /// The expression evaluates to a `False`-like value (e.g., `None`, `0`, `[]`, `""`).
    Falsey,
    /// The expression evaluates to a `True`-like value (e.g., `1`, `"foo"`).
    Truthy,
    /// The expression evaluates to `None`.
    None,
    /// The expression evaluates to an unknown value (e.g., a variable `x` of unknown type).
    Unknown,
}

impl Truthiness {
    /// Return the truthiness of an expression.
    pub fn from_expr<F>(expr: &Expr, is_builtin: F) -> Self
    where
        F: Fn(&str) -> bool,
    {
        match expr {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => {
                if value.is_empty() {
                    Self::Falsey
                } else {
                    Self::Truthy
                }
            }
            Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => {
                if value.is_empty() {
                    Self::Falsey
                } else {
                    Self::Truthy
                }
            }
            Expr::NumberLiteral(ast::ExprNumberLiteral { value, .. }) => match value {
                ast::Number::Int(int) => {
                    if *int == 0 {
                        Self::Falsey
                    } else {
                        Self::Truthy
                    }
                }
                ast::Number::Float(float) => {
                    if *float == 0.0 {
                        Self::Falsey
                    } else {
                        Self::Truthy
                    }
                }
                ast::Number::Complex { real, imag, .. } => {
                    if *real == 0.0 && *imag == 0.0 {
                        Self::Falsey
                    } else {
                        Self::Truthy
                    }
                }
            },
            Expr::BooleanLiteral(ast::ExprBooleanLiteral { value, .. }) => {
                if *value {
                    Self::True
                } else {
                    Self::False
                }
            }
            Expr::NoneLiteral(_) => Self::None,
            Expr::EllipsisLiteral(_) => Self::Truthy,
            Expr::FString(f_string) => {
                if is_empty_f_string(f_string) {
                    Self::Falsey
                } else if is_non_empty_f_string(f_string) {
                    Self::Truthy
                } else {
                    Self::Unknown
                }
            }
            Expr::List(ast::ExprList { elts, .. })
            | Expr::Set(ast::ExprSet { elts, .. })
            | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                if elts.is_empty() {
                    return Self::Falsey;
                }

                if elts.iter().all(Expr::is_starred_expr) {
                    // [*foo] / [*foo, *bar]
                    Self::Unknown
                } else {
                    Self::Truthy
                }
            }
            Expr::Dict(dict) => {
                if dict.is_empty() {
                    return Self::Falsey;
                }

                if dict.items.iter().all(|item| {
                    matches!(
                        item,
                        DictItem {
                            key: None,
                            value: Expr::Name(..)
                        }
                    )
                }) {
                    // {**foo} / {**foo, **bar}
                    Self::Unknown
                } else {
                    Self::Truthy
                }
            }
            Expr::Call(ast::ExprCall {
                func, arguments, ..
            }) => {
                if let Expr::Name(ast::ExprName { id, .. }) = func.as_ref() {
                    if is_iterable_initializer(id.as_str(), |id| is_builtin(id)) {
                        if arguments.is_empty() {
                            // Ex) `list()`
                            Self::Falsey
                        } else if arguments.args.len() == 1 && arguments.keywords.is_empty() {
                            // Ex) `list([1, 2, 3])`
                            Self::from_expr(&arguments.args[0], is_builtin)
                        } else {
                            Self::Unknown
                        }
                    } else {
                        Self::Unknown
                    }
                } else {
                    Self::Unknown
                }
            }
            _ => Self::Unknown,
        }
    }

    pub fn into_bool(self) -> Option<bool> {
        match self {
            Self::True | Self::Truthy => Some(true),
            Self::False | Self::Falsey => Some(false),
            Self::None => Some(false),
            Self::Unknown => None,
        }
    }
}

/// Returns `true` if the expression definitely resolves to a non-empty string, when used as an
/// f-string expression, or `false` if the expression may resolve to an empty string.
fn is_non_empty_f_string(expr: &ast::ExprFString) -> bool {
    fn inner(expr: &Expr) -> bool {
        match expr {
            // When stringified, these expressions are always non-empty.
            Expr::Lambda(_) => true,
            Expr::Dict(_) => true,
            Expr::Set(_) => true,
            Expr::ListComp(_) => true,
            Expr::SetComp(_) => true,
            Expr::DictComp(_) => true,
            Expr::Compare(_) => true,
            Expr::NumberLiteral(_) => true,
            Expr::BooleanLiteral(_) => true,
            Expr::NoneLiteral(_) => true,
            Expr::EllipsisLiteral(_) => true,
            Expr::List(_) => true,
            Expr::Tuple(_) => true,

            // These expressions must resolve to the inner expression.
            Expr::If(ast::ExprIf { body, orelse, .. }) => inner(body) && inner(orelse),
            Expr::Named(ast::ExprNamed { value, .. }) => inner(value),

            // These expressions are complex. We can't determine whether they're empty or not.
            Expr::BoolOp(ast::ExprBoolOp { .. }) => false,
            Expr::BinOp(ast::ExprBinOp { .. }) => false,
            Expr::UnaryOp(ast::ExprUnaryOp { .. }) => false,
            Expr::Generator(_) => false,
            Expr::Await(_) => false,
            Expr::Yield(_) => false,
            Expr::YieldFrom(_) => false,
            Expr::Call(_) => false,
            Expr::Attribute(_) => false,
            Expr::Subscript(_) => false,
            Expr::Starred(_) => false,
            Expr::Name(_) => false,
            Expr::Slice(_) => false,
            Expr::IpyEscapeCommand(_) => false,

            // These literals may or may not be empty.
            Expr::FString(f_string) => is_non_empty_f_string(f_string),
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => !value.is_empty(),
            Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => !value.is_empty(),
        }
    }

    expr.value.iter().any(|part| match part {
        ast::FStringPart::Literal(string_literal) => !string_literal.is_empty(),
        ast::FStringPart::FString(f_string) => {
            f_string.elements.iter().all(|element| match element {
                FStringElement::Literal(string_literal) => !string_literal.is_empty(),
                FStringElement::Expression(f_string) => inner(&f_string.expression),
            })
        }
    })
}

/// Returns `true` if the expression definitely resolves to the empty string, when used as an f-string
/// expression.
fn is_empty_f_string(expr: &ast::ExprFString) -> bool {
    fn inner(expr: &Expr) -> bool {
        match expr {
            Expr::StringLiteral(ast::ExprStringLiteral { value, .. }) => value.is_empty(),
            Expr::BytesLiteral(ast::ExprBytesLiteral { value, .. }) => value.is_empty(),
            Expr::FString(ast::ExprFString { value, .. }) => {
                value
                    .elements()
                    .all(|f_string_element| match f_string_element {
                        FStringElement::Literal(ast::FStringLiteralElement { value, .. }) => {
                            value.is_empty()
                        }
                        FStringElement::Expression(ast::FStringExpressionElement {
                            expression,
                            ..
                        }) => inner(expression),
                    })
            }
            _ => false,
        }
    }

    expr.value.iter().all(|part| match part {
        ast::FStringPart::Literal(string_literal) => string_literal.is_empty(),
        ast::FStringPart::FString(f_string) => {
            f_string.elements.iter().all(|element| match element {
                FStringElement::Literal(string_literal) => string_literal.is_empty(),
                FStringElement::Expression(f_string) => inner(&f_string.expression),
            })
        }
    })
}

pub fn generate_comparison(
    left: &Expr,
    ops: &[CmpOp],
    comparators: &[Expr],
    parent: AnyNodeRef,
    comment_ranges: &CommentRanges,
    source: &str,
) -> String {
    let start = left.start();
    let end = comparators.last().map_or_else(|| left.end(), Ranged::end);
    let mut contents = String::with_capacity(usize::from(end - start));

    // Add the left side of the comparison.
    contents.push_str(
        &source[parenthesized_range(left.into(), parent, comment_ranges, source)
            .unwrap_or(left.range())],
    );

    for (op, comparator) in ops.iter().zip(comparators) {
        // Add the operator.
        contents.push_str(match op {
            CmpOp::Eq => " == ",
            CmpOp::NotEq => " != ",
            CmpOp::Lt => " < ",
            CmpOp::LtE => " <= ",
            CmpOp::Gt => " > ",
            CmpOp::GtE => " >= ",
            CmpOp::In => " in ",
            CmpOp::NotIn => " not in ",
            CmpOp::Is => " is ",
            CmpOp::IsNot => " is not ",
        });

        // Add the right side of the comparison.
        contents.push_str(
            &source[parenthesized_range(comparator.into(), parent, comment_ranges, source)
                .unwrap_or(comparator.range())],
        );
    }

    contents
}

/// Format the expression as a PEP 604-style optional.
pub fn pep_604_optional(expr: &Expr) -> Expr {
    ast::ExprBinOp {
        left: Box::new(expr.clone()),
        op: Operator::BitOr,
        right: Box::new(Expr::NoneLiteral(ast::ExprNoneLiteral::default())),
        range: TextRange::default(),
    }
    .into()
}

/// Format the expressions as a PEP 604-style union.
pub fn pep_604_union(elts: &[Expr]) -> Expr {
    match elts {
        [] => Expr::Tuple(ast::ExprTuple {
            elts: vec![],
            ctx: ExprContext::Load,
            range: TextRange::default(),
            parenthesized: true,
        }),
        [Expr::Tuple(ast::ExprTuple { elts, .. })] => pep_604_union(elts),
        [elt] => elt.clone(),
        [rest @ .., elt] => Expr::BinOp(ast::ExprBinOp {
            left: Box::new(pep_604_union(rest)),
            op: Operator::BitOr,
            right: Box::new(pep_604_union(&[elt.clone()])),
            range: TextRange::default(),
        }),
    }
}

/// Format the expression as a `typing.Optional`-style optional.
pub fn typing_optional(elt: Expr, binding: Name) -> Expr {
    Expr::Subscript(ast::ExprSubscript {
        value: Box::new(Expr::Name(ast::ExprName {
            id: binding,
            range: TextRange::default(),
            ctx: ExprContext::Load,
        })),
        slice: Box::new(elt),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    })
}

/// Format the expressions as a `typing.Union`-style union.
///
/// Note: It is a syntax error to have `Union[]` so the caller
/// should ensure that the `elts` argument is nonempty.
pub fn typing_union(elts: &[Expr], binding: Name) -> Expr {
    Expr::Subscript(ast::ExprSubscript {
        value: Box::new(Expr::Name(ast::ExprName {
            id: binding,
            range: TextRange::default(),
            ctx: ExprContext::Load,
        })),
        slice: Box::new(Expr::Tuple(ast::ExprTuple {
            range: TextRange::default(),
            elts: elts.to_vec(),
            ctx: ExprContext::Load,
            parenthesized: false,
        })),
        ctx: ExprContext::Load,
        range: TextRange::default(),
    })
}

/// Determine the indentation level of an own-line comment, defined as the minimum indentation of
/// all comments between the preceding node and the comment, including the comment itself. In
/// other words, we don't allow successive comments to ident _further_ than any preceding comments.
///
/// For example, given:
/// ```python
/// if True:
///     pass
///     # comment
/// ```
///
/// The indentation would be 4, as the comment is indented by 4 spaces.
///
/// Given:
/// ```python
/// if True:
///     pass
/// # comment
/// else:
///     pass
/// ```
///
/// The indentation would be 0, as the comment is not indented at all.
///
/// Given:
/// ```python
/// if True:
///     pass
///     # comment
///         # comment
/// ```
///
/// Both comments would be marked as indented at 4 spaces, as the indentation of the first comment
/// is used for the second comment.
///
/// This logic avoids pathological cases like:
/// ```python
/// try:
///     if True:
///         if True:
///             pass
///
///         # a
///             # b
///         # c
/// except Exception:
///     pass
/// ```
///
/// If we don't use the minimum indentation of any preceding comments, we would mark `# b` as
/// indented to the same depth as `pass`, which could in turn lead to us treating it as a trailing
/// comment of `pass`, despite there being a comment between them that "resets" the indentation.
pub fn comment_indentation_after(
    preceding: AnyNodeRef,
    comment_range: TextRange,
    source: &str,
) -> TextSize {
    let tokenizer = SimpleTokenizer::new(
        source,
        TextRange::new(source.full_line_end(preceding.end()), comment_range.end()),
    );

    tokenizer
        .filter_map(|token| {
            if token.kind() == SimpleTokenKind::Comment {
                indentation_at_offset(token.start(), source).map(TextLen::text_len)
            } else {
                None
            }
        })
        .min()
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::cell::RefCell;
    use std::vec;

    use ruff_text_size::TextRange;

    use crate::helpers::{any_over_stmt, any_over_type_param, resolve_imported_module_path};
    use crate::{
        Expr, ExprContext, ExprName, ExprNumberLiteral, Identifier, Int, Number, Stmt,
        StmtTypeAlias, TypeParam, TypeParamParamSpec, TypeParamTypeVar, TypeParamTypeVarTuple,
        TypeParams,
    };

    #[test]
    fn resolve_import() {
        // Return the module directly.
        assert_eq!(
            resolve_imported_module_path(0, Some("foo"), None),
            Some(Cow::Borrowed("foo"))
        );

        // Construct the module path from the calling module's path.
        assert_eq!(
            resolve_imported_module_path(
                1,
                Some("foo"),
                Some(&["bar".to_string(), "baz".to_string()])
            ),
            Some(Cow::Owned("bar.foo".to_string()))
        );

        // We can't return the module if it's a relative import, and we don't know the calling
        // module's path.
        assert_eq!(resolve_imported_module_path(1, Some("foo"), None), None);

        // We can't return the module if it's a relative import, and the path goes beyond the
        // calling module's path.
        assert_eq!(
            resolve_imported_module_path(1, Some("foo"), Some(&["bar".to_string()])),
            None,
        );
        assert_eq!(
            resolve_imported_module_path(2, Some("foo"), Some(&["bar".to_string()])),
            None
        );
    }

    #[test]
    fn any_over_stmt_type_alias() {
        let seen = RefCell::new(Vec::new());
        let name = Expr::Name(ExprName {
            id: "x".into(),
            range: TextRange::default(),
            ctx: ExprContext::Load,
        });
        let constant_one = Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(Int::from(1u8)),
            range: TextRange::default(),
        });
        let constant_two = Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(Int::from(2u8)),
            range: TextRange::default(),
        });
        let constant_three = Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(Int::from(3u8)),
            range: TextRange::default(),
        });
        let type_var_one = TypeParam::TypeVar(TypeParamTypeVar {
            range: TextRange::default(),
            bound: Some(Box::new(constant_one.clone())),
            default: None,
            name: Identifier::new("x", TextRange::default()),
        });
        let type_var_two = TypeParam::TypeVar(TypeParamTypeVar {
            range: TextRange::default(),
            bound: None,
            default: Some(Box::new(constant_two.clone())),
            name: Identifier::new("x", TextRange::default()),
        });
        let type_alias = Stmt::TypeAlias(StmtTypeAlias {
            name: Box::new(name.clone()),
            type_params: Some(Box::new(TypeParams {
                type_params: vec![type_var_one, type_var_two],
                range: TextRange::default(),
            })),
            value: Box::new(constant_three.clone()),
            range: TextRange::default(),
        });
        assert!(!any_over_stmt(&type_alias, &|expr| {
            seen.borrow_mut().push(expr.clone());
            false
        }));
        assert_eq!(
            seen.take(),
            vec![name, constant_one, constant_two, constant_three]
        );
    }

    #[test]
    fn any_over_type_param_type_var() {
        let type_var_no_bound = TypeParam::TypeVar(TypeParamTypeVar {
            range: TextRange::default(),
            bound: None,
            default: None,
            name: Identifier::new("x", TextRange::default()),
        });
        assert!(!any_over_type_param(&type_var_no_bound, &|_expr| true));

        let constant = Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(Int::ONE),
            range: TextRange::default(),
        });

        let type_var_with_bound = TypeParam::TypeVar(TypeParamTypeVar {
            range: TextRange::default(),
            bound: Some(Box::new(constant.clone())),
            default: None,
            name: Identifier::new("x", TextRange::default()),
        });
        assert!(
            any_over_type_param(&type_var_with_bound, &|expr| {
                assert_eq!(
                    *expr, constant,
                    "the received expression should be the unwrapped bound"
                );
                true
            }),
            "if true is returned from `func` it should be respected"
        );

        let type_var_with_default = TypeParam::TypeVar(TypeParamTypeVar {
            range: TextRange::default(),
            default: Some(Box::new(constant.clone())),
            bound: None,
            name: Identifier::new("x", TextRange::default()),
        });
        assert!(
            any_over_type_param(&type_var_with_default, &|expr| {
                assert_eq!(
                    *expr, constant,
                    "the received expression should be the unwrapped default"
                );
                true
            }),
            "if true is returned from `func` it should be respected"
        );
    }

    #[test]
    fn any_over_type_param_type_var_tuple() {
        let type_var_tuple = TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
            range: TextRange::default(),
            name: Identifier::new("x", TextRange::default()),
            default: None,
        });
        assert!(
            !any_over_type_param(&type_var_tuple, &|_expr| true),
            "this TypeVarTuple has no expressions to visit"
        );

        let constant = Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(Int::ONE),
            range: TextRange::default(),
        });

        let type_var_tuple_with_default = TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
            range: TextRange::default(),
            default: Some(Box::new(constant.clone())),
            name: Identifier::new("x", TextRange::default()),
        });
        assert!(
            any_over_type_param(&type_var_tuple_with_default, &|expr| {
                assert_eq!(
                    *expr, constant,
                    "the received expression should be the unwrapped default"
                );
                true
            }),
            "if true is returned from `func` it should be respected"
        );
    }

    #[test]
    fn any_over_type_param_param_spec() {
        let type_param_spec = TypeParam::ParamSpec(TypeParamParamSpec {
            range: TextRange::default(),
            name: Identifier::new("x", TextRange::default()),
            default: None,
        });
        assert!(
            !any_over_type_param(&type_param_spec, &|_expr| true),
            "this ParamSpec has no expressions to visit"
        );

        let constant = Expr::NumberLiteral(ExprNumberLiteral {
            value: Number::Int(Int::ONE),
            range: TextRange::default(),
        });

        let param_spec_with_default = TypeParam::TypeVarTuple(TypeParamTypeVarTuple {
            range: TextRange::default(),
            default: Some(Box::new(constant.clone())),
            name: Identifier::new("x", TextRange::default()),
        });
        assert!(
            any_over_type_param(&param_spec_with_default, &|expr| {
                assert_eq!(
                    *expr, constant,
                    "the received expression should be the unwrapped default"
                );
                true
            }),
            "if true is returned from `func` it should be respected"
        );
    }
}
