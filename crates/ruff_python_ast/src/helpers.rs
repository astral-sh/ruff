use std::borrow::Cow;
use std::path::Path;

use itertools::Itertools;
use log::error;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{
    Arguments, Cmpop, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword,
    KeywordData, Located, Location, MatchCase, Pattern, PatternKind, Stmt, StmtKind,
};
use rustpython_parser::{lexer, Mode, Tok};
use smallvec::SmallVec;

use crate::call_path::CallPath;
use crate::source_code::{Generator, Indexer, Locator, Stylist};
use crate::types::Range;
use crate::visitor;
use crate::visitor::Visitor;

/// Create an `Expr` with default location from an `ExprKind`.
pub fn create_expr(node: ExprKind) -> Expr {
    Expr::new(Location::default(), Location::default(), node)
}

/// Create a `Stmt` with a default location from a `StmtKind`.
pub fn create_stmt(node: StmtKind) -> Stmt {
    Stmt::new(Location::default(), Location::default(), node)
}

/// Generate source code from an [`Expr`].
pub fn unparse_expr(expr: &Expr, stylist: &Stylist) -> String {
    let mut generator: Generator = stylist.into();
    generator.unparse_expr(expr, 0);
    generator.generate()
}

/// Generate source code from a [`Stmt`].
pub fn unparse_stmt(stmt: &Stmt, stylist: &Stylist) -> String {
    let mut generator: Generator = stylist.into();
    generator.unparse_stmt(stmt);
    generator.generate()
}

/// Generate source code from an [`Constant`].
pub fn unparse_constant(constant: &Constant, stylist: &Stylist) -> String {
    let mut generator: Generator = stylist.into();
    generator.unparse_constant(constant);
    generator.generate()
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
        if let ExprKind::Call {
            func,
            args,
            keywords,
        } = &expr.node
        {
            if args.is_empty() && keywords.is_empty() {
                if let ExprKind::Name { id, .. } = &func.node {
                    if !matches!(id.as_str(), "set" | "list" | "tuple" | "dict" | "frozenset") {
                        return true;
                    }
                    if !is_builtin(id) {
                        return true;
                    }
                    return false;
                }
            }
        }

        // Avoid false positive for overloaded operators.
        if let ExprKind::BinOp { left, right, .. } = &expr.node {
            if !matches!(
                left.node,
                ExprKind::Constant { .. }
                    | ExprKind::JoinedStr { .. }
                    | ExprKind::List { .. }
                    | ExprKind::Tuple { .. }
                    | ExprKind::Set { .. }
                    | ExprKind::Dict { .. }
                    | ExprKind::ListComp { .. }
                    | ExprKind::SetComp { .. }
                    | ExprKind::DictComp { .. }
            ) {
                return true;
            }
            if !matches!(
                right.node,
                ExprKind::Constant { .. }
                    | ExprKind::JoinedStr { .. }
                    | ExprKind::List { .. }
                    | ExprKind::Tuple { .. }
                    | ExprKind::Set { .. }
                    | ExprKind::Dict { .. }
                    | ExprKind::ListComp { .. }
                    | ExprKind::SetComp { .. }
                    | ExprKind::DictComp { .. }
            ) {
                return true;
            }
            return false;
        }

        // Otherwise, avoid all complex expressions.
        matches!(
            expr.node,
            ExprKind::Await { .. }
                | ExprKind::Call { .. }
                | ExprKind::DictComp { .. }
                | ExprKind::GeneratorExp { .. }
                | ExprKind::ListComp { .. }
                | ExprKind::SetComp { .. }
                | ExprKind::Subscript { .. }
                | ExprKind::Yield { .. }
                | ExprKind::YieldFrom { .. }
        )
    })
}

/// Call `func` over every `Expr` in `expr`, returning `true` if any expression
/// returns `true`..
pub fn any_over_expr<F>(expr: &Expr, func: &F) -> bool
where
    F: Fn(&Expr) -> bool,
{
    if func(expr) {
        return true;
    }
    match &expr.node {
        ExprKind::BoolOp { values, .. } | ExprKind::JoinedStr { values } => {
            values.iter().any(|expr| any_over_expr(expr, func))
        }
        ExprKind::NamedExpr { target, value } => {
            any_over_expr(target, func) || any_over_expr(value, func)
        }
        ExprKind::BinOp { left, right, .. } => {
            any_over_expr(left, func) || any_over_expr(right, func)
        }
        ExprKind::UnaryOp { operand, .. } => any_over_expr(operand, func),
        ExprKind::Lambda { body, .. } => any_over_expr(body, func),
        ExprKind::IfExp { test, body, orelse } => {
            any_over_expr(test, func) || any_over_expr(body, func) || any_over_expr(orelse, func)
        }
        ExprKind::Dict { keys, values } => values
            .iter()
            .chain(keys.iter().flatten())
            .any(|expr| any_over_expr(expr, func)),
        ExprKind::Set { elts } | ExprKind::List { elts, .. } | ExprKind::Tuple { elts, .. } => {
            elts.iter().any(|expr| any_over_expr(expr, func))
        }
        ExprKind::ListComp { elt, generators }
        | ExprKind::SetComp { elt, generators }
        | ExprKind::GeneratorExp { elt, generators } => {
            any_over_expr(elt, func)
                || generators.iter().any(|generator| {
                    any_over_expr(&generator.target, func)
                        || any_over_expr(&generator.iter, func)
                        || generator.ifs.iter().any(|expr| any_over_expr(expr, func))
                })
        }
        ExprKind::DictComp {
            key,
            value,
            generators,
        } => {
            any_over_expr(key, func)
                || any_over_expr(value, func)
                || generators.iter().any(|generator| {
                    any_over_expr(&generator.target, func)
                        || any_over_expr(&generator.iter, func)
                        || generator.ifs.iter().any(|expr| any_over_expr(expr, func))
                })
        }
        ExprKind::Await { value }
        | ExprKind::YieldFrom { value }
        | ExprKind::Attribute { value, .. }
        | ExprKind::Starred { value, .. } => any_over_expr(value, func),
        ExprKind::Yield { value } => value
            .as_ref()
            .map_or(false, |value| any_over_expr(value, func)),
        ExprKind::Compare {
            left, comparators, ..
        } => any_over_expr(left, func) || comparators.iter().any(|expr| any_over_expr(expr, func)),
        ExprKind::Call {
            func: call_func,
            args,
            keywords,
        } => {
            any_over_expr(call_func, func)
                || args.iter().any(|expr| any_over_expr(expr, func))
                || keywords
                    .iter()
                    .any(|keyword| any_over_expr(&keyword.node.value, func))
        }
        ExprKind::FormattedValue {
            value, format_spec, ..
        } => {
            any_over_expr(value, func)
                || format_spec
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
        }
        ExprKind::Subscript { value, slice, .. } => {
            any_over_expr(value, func) || any_over_expr(slice, func)
        }
        ExprKind::Slice { lower, upper, step } => {
            lower
                .as_ref()
                .map_or(false, |value| any_over_expr(value, func))
                || upper
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
                || step
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
        }
        ExprKind::Name { .. } | ExprKind::Constant { .. } => false,
    }
}

pub fn any_over_pattern<F>(pattern: &Pattern, func: &F) -> bool
where
    F: Fn(&Expr) -> bool,
{
    match &pattern.node {
        PatternKind::MatchValue { value } => any_over_expr(value, func),
        PatternKind::MatchSingleton { .. } => false,
        PatternKind::MatchSequence { patterns } => patterns
            .iter()
            .any(|pattern| any_over_pattern(pattern, func)),
        PatternKind::MatchMapping { keys, patterns, .. } => {
            keys.iter().any(|key| any_over_expr(key, func))
                || patterns
                    .iter()
                    .any(|pattern| any_over_pattern(pattern, func))
        }
        PatternKind::MatchClass {
            cls,
            patterns,
            kwd_patterns,
            ..
        } => {
            any_over_expr(cls, func)
                || patterns
                    .iter()
                    .any(|pattern| any_over_pattern(pattern, func))
                || kwd_patterns
                    .iter()
                    .any(|pattern| any_over_pattern(pattern, func))
        }
        PatternKind::MatchStar { .. } => false,
        PatternKind::MatchAs { pattern, .. } => pattern
            .as_ref()
            .map_or(false, |pattern| any_over_pattern(pattern, func)),
        PatternKind::MatchOr { patterns } => patterns
            .iter()
            .any(|pattern| any_over_pattern(pattern, func)),
    }
}

pub fn any_over_stmt<F>(stmt: &Stmt, func: &F) -> bool
where
    F: Fn(&Expr) -> bool,
{
    match &stmt.node {
        StmtKind::FunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        }
        | StmtKind::AsyncFunctionDef {
            args,
            body,
            decorator_list,
            returns,
            ..
        } => {
            args.defaults.iter().any(|expr| any_over_expr(expr, func))
                || args
                    .kw_defaults
                    .iter()
                    .any(|expr| any_over_expr(expr, func))
                || args.args.iter().any(|arg| {
                    arg.node
                        .annotation
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
                })
                || args.kwonlyargs.iter().any(|arg| {
                    arg.node
                        .annotation
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
                })
                || args.posonlyargs.iter().any(|arg| {
                    arg.node
                        .annotation
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
                })
                || args.vararg.as_ref().map_or(false, |arg| {
                    arg.node
                        .annotation
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
                })
                || args.kwarg.as_ref().map_or(false, |arg| {
                    arg.node
                        .annotation
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
                })
                || body.iter().any(|stmt| any_over_stmt(stmt, func))
                || decorator_list.iter().any(|expr| any_over_expr(expr, func))
                || returns
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
        }
        StmtKind::ClassDef {
            bases,
            keywords,
            body,
            decorator_list,
            ..
        } => {
            bases.iter().any(|expr| any_over_expr(expr, func))
                || keywords
                    .iter()
                    .any(|keyword| any_over_expr(&keyword.node.value, func))
                || body.iter().any(|stmt| any_over_stmt(stmt, func))
                || decorator_list.iter().any(|expr| any_over_expr(expr, func))
        }
        StmtKind::Return { value } => value
            .as_ref()
            .map_or(false, |value| any_over_expr(value, func)),
        StmtKind::Delete { targets } => targets.iter().any(|expr| any_over_expr(expr, func)),
        StmtKind::Assign { targets, value, .. } => {
            targets.iter().any(|expr| any_over_expr(expr, func)) || any_over_expr(value, func)
        }
        StmtKind::AugAssign { target, value, .. } => {
            any_over_expr(target, func) || any_over_expr(value, func)
        }
        StmtKind::AnnAssign {
            target,
            annotation,
            value,
            ..
        } => {
            any_over_expr(target, func)
                || any_over_expr(annotation, func)
                || value
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
        }
        StmtKind::For {
            target,
            iter,
            body,
            orelse,
            ..
        }
        | StmtKind::AsyncFor {
            target,
            iter,
            body,
            orelse,
            ..
        } => {
            any_over_expr(target, func)
                || any_over_expr(iter, func)
                || any_over_body(body, func)
                || any_over_body(orelse, func)
        }
        StmtKind::While { test, body, orelse } => {
            any_over_expr(test, func) || any_over_body(body, func) || any_over_body(orelse, func)
        }
        StmtKind::If { test, body, orelse } => {
            any_over_expr(test, func) || any_over_body(body, func) || any_over_body(orelse, func)
        }
        StmtKind::With { items, body, .. } | StmtKind::AsyncWith { items, body, .. } => {
            items.iter().any(|withitem| {
                any_over_expr(&withitem.context_expr, func)
                    || withitem
                        .optional_vars
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
            }) || any_over_body(body, func)
        }
        StmtKind::Raise { exc, cause } => {
            exc.as_ref()
                .map_or(false, |value| any_over_expr(value, func))
                || cause
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
        }
        StmtKind::Try {
            body,
            handlers,
            orelse,
            finalbody,
        }
        | StmtKind::TryStar {
            body,
            handlers,
            orelse,
            finalbody,
        } => {
            any_over_body(body, func)
                || handlers.iter().any(|handler| {
                    let ExcepthandlerKind::ExceptHandler { type_, body, .. } = &handler.node;
                    type_
                        .as_ref()
                        .map_or(false, |expr| any_over_expr(expr, func))
                        || any_over_body(body, func)
                })
                || any_over_body(orelse, func)
                || any_over_body(finalbody, func)
        }
        StmtKind::Assert { test, msg } => {
            any_over_expr(test, func)
                || msg
                    .as_ref()
                    .map_or(false, |value| any_over_expr(value, func))
        }
        StmtKind::Match { subject, cases } => {
            any_over_expr(subject, func)
                || cases.iter().any(|case| {
                    let MatchCase {
                        pattern,
                        guard,
                        body,
                    } = case;
                    any_over_pattern(pattern, func)
                        || guard
                            .as_ref()
                            .map_or(false, |expr| any_over_expr(expr, func))
                        || any_over_body(body, func)
                })
        }
        StmtKind::Import { .. } => false,
        StmtKind::ImportFrom { .. } => false,
        StmtKind::Global { .. } => false,
        StmtKind::Nonlocal { .. } => false,
        StmtKind::Expr { value } => any_over_expr(value, func),
        StmtKind::Pass => false,
        StmtKind::Break => false,
        StmtKind::Continue => false,
    }
}

pub fn any_over_body<F>(body: &[Stmt], func: &F) -> bool
where
    F: Fn(&Expr) -> bool,
{
    body.iter().any(|stmt| any_over_stmt(stmt, func))
}

static DUNDER_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"__[^\s]+__").unwrap());

/// Return `true` if the [`Stmt`] is an assignment to a dunder (like `__all__`).
pub fn is_assignment_to_a_dunder(stmt: &Stmt) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type
    // annotation. This is what pycodestyle (as of 2.9.1) does.
    match &stmt.node {
        StmtKind::Assign { targets, .. } => {
            if targets.len() != 1 {
                return false;
            }
            match &targets[0].node {
                ExprKind::Name { id, .. } => DUNDER_REGEX.is_match(id),
                _ => false,
            }
        }
        StmtKind::AnnAssign { target, .. } => match &target.node {
            ExprKind::Name { id, .. } => DUNDER_REGEX.is_match(id),
            _ => false,
        },
        _ => false,
    }
}

/// Return `true` if the [`Expr`] is a singleton (`None`, `True`, `False`, or
/// `...`).
pub const fn is_singleton(expr: &Expr) -> bool {
    matches!(
        expr.node,
        ExprKind::Constant {
            value: Constant::None | Constant::Bool(_) | Constant::Ellipsis,
            ..
        }
    )
}

/// Return `true` if the [`Expr`] is a constant or tuple of constants.
pub fn is_constant(expr: &Expr) -> bool {
    match &expr.node {
        ExprKind::Constant { .. } => true,
        ExprKind::Tuple { elts, .. } => elts.iter().all(is_constant),
        _ => false,
    }
}

/// Return `true` if the [`Expr`] is a non-singleton constant.
pub fn is_constant_non_singleton(expr: &Expr) -> bool {
    is_constant(expr) && !is_singleton(expr)
}

/// Return the [`Keyword`] with the given name, if it's present in the list of
/// [`Keyword`] arguments.
pub fn find_keyword<'a>(keywords: &'a [Keyword], keyword_name: &str) -> Option<&'a Keyword> {
    keywords.iter().find(|keyword| {
        let KeywordData { arg, .. } = &keyword.node;
        arg.as_ref().map_or(false, |arg| arg == keyword_name)
    })
}

/// Return `true` if an [`Expr`] is `None`.
pub const fn is_const_none(expr: &Expr) -> bool {
    matches!(
        &expr.node,
        ExprKind::Constant {
            value: Constant::None,
            kind: None
        },
    )
}

/// Return `true` if an [`Expr`] is `True`.
pub const fn is_const_true(expr: &Expr) -> bool {
    matches!(
        &expr.node,
        ExprKind::Constant {
            value: Constant::Bool(true),
            kind: None
        },
    )
}

/// Return `true` if a keyword argument is present with a non-`None` value.
pub fn has_non_none_keyword(keywords: &[Keyword], keyword: &str) -> bool {
    find_keyword(keywords, keyword).map_or(false, |keyword| {
        let KeywordData { value, .. } = &keyword.node;
        !is_const_none(value)
    })
}

/// Extract the names of all handled exceptions.
pub fn extract_handled_exceptions(handlers: &[Excepthandler]) -> Vec<&Expr> {
    let mut handled_exceptions = Vec::new();
    for handler in handlers {
        match &handler.node {
            ExcepthandlerKind::ExceptHandler { type_, .. } => {
                if let Some(type_) = type_ {
                    if let ExprKind::Tuple { elts, .. } = &type_.node {
                        for type_ in elts {
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

/// Return the set of all bound argument names.
pub fn collect_arg_names<'a>(arguments: &'a Arguments) -> FxHashSet<&'a str> {
    let mut arg_names: FxHashSet<&'a str> = FxHashSet::default();
    for arg in &arguments.posonlyargs {
        arg_names.insert(arg.node.arg.as_str());
    }
    for arg in &arguments.args {
        arg_names.insert(arg.node.arg.as_str());
    }
    if let Some(arg) = &arguments.vararg {
        arg_names.insert(arg.node.arg.as_str());
    }
    for arg in &arguments.kwonlyargs {
        arg_names.insert(arg.node.arg.as_str());
    }
    if let Some(arg) = &arguments.kwarg {
        arg_names.insert(arg.node.arg.as_str());
    }
    arg_names
}

/// Given an [`Expr`] that can be callable or not (like a decorator, which could
/// be used with or without explicit call syntax), return the underlying
/// callable.
pub fn map_callable(decorator: &Expr) -> &Expr {
    if let ExprKind::Call { func, .. } = &decorator.node {
        func
    } else {
        decorator
    }
}

/// Returns `true` if a statement or expression includes at least one comment.
pub fn has_comments<T>(located: &Located<T>, locator: &Locator) -> bool {
    let start = if match_leading_content(located, locator) {
        located.location
    } else {
        Location::new(located.location.row(), 0)
    };
    let end = if match_trailing_content(located, locator) {
        located.end_location.unwrap()
    } else {
        Location::new(located.end_location.unwrap().row() + 1, 0)
    };
    has_comments_in(Range::new(start, end), locator)
}

/// Returns `true` if a [`Range`] includes at least one comment.
pub fn has_comments_in(range: Range, locator: &Locator) -> bool {
    for tok in lexer::lex_located(locator.slice(range), Mode::Module, range.location) {
        match tok {
            Ok((_, tok, _)) => {
                if matches!(tok, Tok::Comment(..)) {
                    return true;
                }
            }
            Err(_) => {
                return false;
            }
        }
    }
    false
}

/// Return `true` if the body uses `locals()`, `globals()`, `vars()`, `eval()`.
///
/// Accepts a closure that determines whether a given name (e.g., `"list"`) is a Python builtin.
pub fn uses_magic_variable_access<F>(body: &[Stmt], is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    any_over_body(body, &|expr| {
        if let ExprKind::Call { func, .. } = &expr.node {
            if let ExprKind::Name { id, .. } = &func.node {
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
/// assert_eq!(format_import_from(None, None), "".to_string());
/// assert_eq!(format_import_from(Some(1), None), ".".to_string());
/// assert_eq!(format_import_from(Some(1), Some("foo")), ".foo".to_string());
/// ```
pub fn format_import_from(level: Option<usize>, module: Option<&str>) -> String {
    let mut module_name = String::with_capacity(16);
    if let Some(level) = level {
        for _ in 0..level {
            module_name.push('.');
        }
    }
    if let Some(module) = module {
        module_name.push_str(module);
    }
    module_name
}

/// Format the member reference name for a relative import.
///
/// # Examples
///
/// ```rust
/// # use ruff_python_ast::helpers::format_import_from_member;
///
/// assert_eq!(format_import_from_member(None, None, "bar"), "bar".to_string());
/// assert_eq!(format_import_from_member(Some(1), None, "bar"), ".bar".to_string());
/// assert_eq!(format_import_from_member(Some(1), Some("foo"), "bar"), ".foo.bar".to_string());
/// ```
pub fn format_import_from_member(
    level: Option<usize>,
    module: Option<&str>,
    member: &str,
) -> String {
    let mut full_name = String::with_capacity(
        level.map_or(0, |level| level)
            + module.as_ref().map_or(0, |module| module.len())
            + 1
            + member.len(),
    );
    if let Some(level) = level {
        for _ in 0..level {
            full_name.push('.');
        }
    }
    if let Some(module) = module {
        full_name.push_str(module);
        full_name.push('.');
    }
    full_name.push_str(member);
    full_name
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

/// Create a [`CallPath`] from a relative import reference name (like `".foo.bar"`).
///
/// Returns an empty [`CallPath`] if the import is invalid (e.g., a relative import that
/// extends beyond the top-level module).
///
/// # Examples
///
/// ```rust
/// # use smallvec::{smallvec, SmallVec};
/// # use ruff_python_ast::helpers::from_relative_import;
///
/// assert_eq!(from_relative_import(&[], "bar"), SmallVec::from_buf(["bar"]));
/// assert_eq!(from_relative_import(&["foo".to_string()], "bar"), SmallVec::from_buf(["foo", "bar"]));
/// assert_eq!(from_relative_import(&["foo".to_string()], "bar.baz"), SmallVec::from_buf(["foo", "bar", "baz"]));
/// assert_eq!(from_relative_import(&["foo".to_string()], ".bar"), SmallVec::from_buf(["bar"]));
/// assert!(from_relative_import(&["foo".to_string()], "..bar").is_empty());
/// assert!(from_relative_import(&["foo".to_string()], "...bar").is_empty());
/// ```
pub fn from_relative_import<'a>(module: &'a [String], name: &'a str) -> CallPath<'a> {
    let mut call_path: CallPath = SmallVec::with_capacity(module.len() + 1);

    // Start with the module path.
    call_path.extend(module.iter().map(String::as_str));

    // Remove segments based on the number of dots.
    for _ in 0..name.chars().take_while(|c| *c == '.').count() {
        if call_path.is_empty() {
            return SmallVec::new();
        }
        call_path.pop();
    }

    // Add the remaining segments.
    call_path.extend(name.trim_start_matches('.').split('.'));

    call_path
}

/// Given an imported module (based on its relative import level and module name), return the
/// fully-qualified module path.
pub fn resolve_imported_module_path<'a>(
    level: Option<usize>,
    module: Option<&'a str>,
    module_path: Option<&[String]>,
) -> Option<Cow<'a, str>> {
    let Some(level) = level else {
        return Some(Cow::Borrowed(module.unwrap_or("")));
    };

    if level == 0 {
        return Some(Cow::Borrowed(module.unwrap_or("")));
    }

    let Some(module_path) = module_path else {
        return None;
    };

    if level >= module_path.len() {
        return None;
    }

    let mut qualified_path = module_path[..module_path.len() - level].join(".");
    if let Some(module) = module {
        if !qualified_path.is_empty() {
            qualified_path.push('.');
        }
        qualified_path.push_str(module);
    }
    Some(Cow::Owned(qualified_path))
}

/// A [`Visitor`] that collects all `return` statements in a function or method.
#[derive(Default)]
pub struct ReturnStatementVisitor<'a> {
    pub returns: Vec<Option<&'a Expr>>,
}

impl<'a, 'b> Visitor<'b> for ReturnStatementVisitor<'a>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::FunctionDef { .. } | StmtKind::AsyncFunctionDef { .. } => {
                // Don't recurse.
            }
            StmtKind::Return { value } => self.returns.push(value.as_deref()),
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// A [`Visitor`] that collects all `raise` statements in a function or method.
#[derive(Default)]
pub struct RaiseStatementVisitor<'a> {
    pub raises: Vec<(Range, Option<&'a Expr>, Option<&'a Expr>)>,
}

impl<'a, 'b> Visitor<'b> for RaiseStatementVisitor<'b>
where
    'b: 'a,
{
    fn visit_stmt(&mut self, stmt: &'b Stmt) {
        match &stmt.node {
            StmtKind::Raise { exc, cause } => {
                self.raises
                    .push((Range::from(stmt), exc.as_deref(), cause.as_deref()));
            }
            StmtKind::ClassDef { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::Try { .. }
            | StmtKind::TryStar { .. } => {}
            StmtKind::If { body, orelse, .. } => {
                visitor::walk_body(self, body);
                visitor::walk_body(self, orelse);
            }
            StmtKind::While { body, .. }
            | StmtKind::With { body, .. }
            | StmtKind::AsyncWith { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::AsyncFor { body, .. } => {
                visitor::walk_body(self, body);
            }
            StmtKind::Match { cases, .. } => {
                for case in cases {
                    visitor::walk_body(self, &case.body);
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct GlobalStatementVisitor<'a> {
    globals: FxHashMap<&'a str, &'a Stmt>,
}

impl<'a> Visitor<'a> for GlobalStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: &'a Stmt) {
        match &stmt.node {
            StmtKind::Global { names } => {
                for name in names {
                    self.globals.insert(name, stmt);
                }
            }
            StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
            | StmtKind::ClassDef { .. } => {
                // Don't recurse.
            }
            _ => visitor::walk_stmt(self, stmt),
        }
    }
}

/// Extract a map from global name to its last-defining [`Stmt`].
pub fn extract_globals(body: &[Stmt]) -> FxHashMap<&str, &Stmt> {
    let mut visitor = GlobalStatementVisitor::default();
    for stmt in body {
        visitor.visit_stmt(stmt);
    }
    visitor.globals
}

/// Convert a location within a file (relative to `base`) to an absolute
/// position.
pub fn to_absolute(relative: Location, base: Location) -> Location {
    if relative.row() == 1 {
        Location::new(
            relative.row() + base.row() - 1,
            relative.column() + base.column(),
        )
    } else {
        Location::new(relative.row() + base.row() - 1, relative.column())
    }
}

pub fn to_relative(absolute: Location, base: Location) -> Location {
    if absolute.row() == base.row() {
        Location::new(
            absolute.row() - base.row() + 1,
            absolute.column() - base.column(),
        )
    } else {
        Location::new(absolute.row() - base.row() + 1, absolute.column())
    }
}

/// Return `true` if a [`Located`] has leading content.
pub fn match_leading_content<T>(located: &Located<T>, locator: &Locator) -> bool {
    let range = Range::new(Location::new(located.location.row(), 0), located.location);
    let prefix = locator.slice(range);
    prefix.chars().any(|char| !char.is_whitespace())
}

/// Return `true` if a [`Located`] has trailing content.
pub fn match_trailing_content<T>(located: &Located<T>, locator: &Locator) -> bool {
    let range = Range::new(
        located.end_location.unwrap(),
        Location::new(located.end_location.unwrap().row() + 1, 0),
    );
    let suffix = locator.slice(range);
    for char in suffix.chars() {
        if char == '#' {
            return false;
        }
        if !char.is_whitespace() {
            return true;
        }
    }
    false
}

/// If a [`Located`] has a trailing comment, return the index of the hash.
pub fn match_trailing_comment<T>(located: &Located<T>, locator: &Locator) -> Option<usize> {
    let range = Range::new(
        located.end_location.unwrap(),
        Location::new(located.end_location.unwrap().row() + 1, 0),
    );
    let suffix = locator.slice(range);
    for (i, char) in suffix.chars().enumerate() {
        if char == '#' {
            return Some(i);
        }
        if !char.is_whitespace() {
            return None;
        }
    }
    None
}

/// Return the number of trailing empty lines following a statement.
pub fn count_trailing_lines(stmt: &Stmt, locator: &Locator) -> usize {
    let suffix = locator.after(Location::new(stmt.end_location.unwrap().row() + 1, 0));
    suffix
        .lines()
        .take_while(|line| line.trim().is_empty())
        .count()
}

/// Return the range of the first parenthesis pair after a given [`Location`].
pub fn match_parens(start: Location, locator: &Locator) -> Option<Range> {
    let contents = locator.after(start);
    let mut fix_start = None;
    let mut fix_end = None;
    let mut count: usize = 0;
    for (start, tok, end) in lexer::lex_located(contents, Mode::Module, start).flatten() {
        if matches!(tok, Tok::Lpar) {
            if count == 0 {
                fix_start = Some(start);
            }
            count += 1;
        }
        if matches!(tok, Tok::Rpar) {
            count -= 1;
            if count == 0 {
                fix_end = Some(end);
                break;
            }
        }
    }
    match (fix_start, fix_end) {
        (Some(start), Some(end)) => Some(Range::new(start, end)),
        _ => None,
    }
}

/// Return the appropriate visual `Range` for any message that spans a `Stmt`.
/// Specifically, this method returns the range of a function or class name,
/// rather than that of the entire function or class body.
pub fn identifier_range(stmt: &Stmt, locator: &Locator) -> Range {
    if matches!(
        stmt.node,
        StmtKind::ClassDef { .. }
            | StmtKind::FunctionDef { .. }
            | StmtKind::AsyncFunctionDef { .. }
    ) {
        let contents = locator.slice(stmt);
        for (start, tok, end) in lexer::lex_located(contents, Mode::Module, stmt.location).flatten()
        {
            if matches!(tok, Tok::Name { .. }) {
                return Range::new(start, end);
            }
        }
        error!("Failed to find identifier for {:?}", stmt);
    }
    Range::from(stmt)
}

/// Return the ranges of [`Tok::Name`] tokens within a specified node.
pub fn find_names<'a, T>(
    located: &'a Located<T>,
    locator: &'a Locator,
) -> impl Iterator<Item = Range> + 'a {
    let contents = locator.slice(located);
    lexer::lex_located(contents, Mode::Module, located.location)
        .flatten()
        .filter(|(_, tok, _)| matches!(tok, Tok::Name { .. }))
        .map(|(start, _, end)| Range {
            location: start,
            end_location: end,
        })
}

/// Return the `Range` of `name` in `Excepthandler`.
pub fn excepthandler_name_range(handler: &Excepthandler, locator: &Locator) -> Option<Range> {
    let ExcepthandlerKind::ExceptHandler {
        name, type_, body, ..
    } = &handler.node;
    match (name, type_) {
        (Some(_), Some(type_)) => {
            let type_end_location = type_.end_location.unwrap();
            let contents = locator.slice(Range::new(type_end_location, body[0].location));
            let range = lexer::lex_located(contents, Mode::Module, type_end_location)
                .flatten()
                .tuple_windows()
                .find(|(tok, next_tok)| {
                    matches!(tok.1, Tok::As) && matches!(next_tok.1, Tok::Name { .. })
                })
                .map(|((..), (location, _, end_location))| Range::new(location, end_location));
            range
        }
        _ => None,
    }
}

/// Return the `Range` of `except` in `Excepthandler`.
pub fn except_range(handler: &Excepthandler, locator: &Locator) -> Range {
    let ExcepthandlerKind::ExceptHandler { body, type_, .. } = &handler.node;
    let end = if let Some(type_) = type_ {
        type_.location
    } else {
        body.first()
            .expect("Expected body to be non-empty")
            .location
    };
    let contents = locator.slice(Range {
        location: handler.location,
        end_location: end,
    });
    let range = lexer::lex_located(contents, Mode::Module, handler.location)
        .flatten()
        .find(|(_, kind, _)| matches!(kind, Tok::Except { .. }))
        .map(|(location, _, end_location)| Range {
            location,
            end_location,
        })
        .expect("Failed to find `except` range");
    range
}

/// Return the `Range` of `else` in `For`, `AsyncFor`, and `While` statements.
pub fn else_range(stmt: &Stmt, locator: &Locator) -> Option<Range> {
    match &stmt.node {
        StmtKind::For { body, orelse, .. }
        | StmtKind::AsyncFor { body, orelse, .. }
        | StmtKind::While { body, orelse, .. }
            if !orelse.is_empty() =>
        {
            let body_end = body
                .last()
                .expect("Expected body to be non-empty")
                .end_location
                .unwrap();
            let contents = locator.slice(Range {
                location: body_end,
                end_location: orelse
                    .first()
                    .expect("Expected orelse to be non-empty")
                    .location,
            });
            let range = lexer::lex_located(contents, Mode::Module, body_end)
                .flatten()
                .find(|(_, kind, _)| matches!(kind, Tok::Else))
                .map(|(location, _, end_location)| Range {
                    location,
                    end_location,
                });
            range
        }
        _ => None,
    }
}

/// Return the `Range` of the first `Tok::Colon` token in a `Range`.
pub fn first_colon_range(range: Range, locator: &Locator) -> Option<Range> {
    let contents = locator.slice(range);
    let range = lexer::lex_located(contents, Mode::Module, range.location)
        .flatten()
        .find(|(_, kind, _)| matches!(kind, Tok::Colon))
        .map(|(location, _, end_location)| Range {
            location,
            end_location,
        });
    range
}

/// Return the `Range` of the first `Elif` or `Else` token in an `If` statement.
pub fn elif_else_range(stmt: &Stmt, locator: &Locator) -> Option<Range> {
    let StmtKind::If { body, orelse, .. } = &stmt.node else {
        return None;
    };

    let start = body
        .last()
        .expect("Expected body to be non-empty")
        .end_location
        .unwrap();
    let end = match &orelse[..] {
        [Stmt {
            node: StmtKind::If { test, .. },
            ..
        }] => test.location,
        [stmt, ..] => stmt.location,
        _ => return None,
    };
    let contents = locator.slice(Range::new(start, end));
    let range = lexer::lex_located(contents, Mode::Module, start)
        .flatten()
        .find(|(_, kind, _)| matches!(kind, Tok::Elif | Tok::Else))
        .map(|(location, _, end_location)| Range {
            location,
            end_location,
        });
    range
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements preceding it.
pub fn preceded_by_continuation(stmt: &Stmt, indexer: &Indexer) -> bool {
    stmt.location.row() > 1
        && indexer
            .continuation_lines()
            .contains(&(stmt.location.row() - 1))
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements preceding it.
pub fn preceded_by_multi_statement_line(stmt: &Stmt, locator: &Locator, indexer: &Indexer) -> bool {
    match_leading_content(stmt, locator) || preceded_by_continuation(stmt, indexer)
}

/// Return `true` if a `Stmt` appears to be part of a multi-statement line, with
/// other statements following it.
pub fn followed_by_multi_statement_line(stmt: &Stmt, locator: &Locator) -> bool {
    match_trailing_content(stmt, locator)
}

/// Return `true` if a `Stmt` is a docstring.
pub const fn is_docstring_stmt(stmt: &Stmt) -> bool {
    if let StmtKind::Expr { value } = &stmt.node {
        matches!(
            value.node,
            ExprKind::Constant {
                value: Constant::Str { .. },
                ..
            }
        )
    } else {
        false
    }
}

#[derive(Default)]
/// A simple representation of a call's positional and keyword arguments.
pub struct SimpleCallArgs<'a> {
    pub args: Vec<&'a Expr>,
    pub kwargs: FxHashMap<&'a str, &'a Expr>,
}

impl<'a> SimpleCallArgs<'a> {
    pub fn new(
        args: impl IntoIterator<Item = &'a Expr>,
        keywords: impl IntoIterator<Item = &'a Keyword>,
    ) -> Self {
        let args = args
            .into_iter()
            .take_while(|arg| !matches!(arg.node, ExprKind::Starred { .. }))
            .collect();

        let kwargs = keywords
            .into_iter()
            .filter_map(|keyword| {
                let node = &keyword.node;
                node.arg.as_ref().map(|arg| (arg.as_ref(), &node.value))
            })
            .collect();

        SimpleCallArgs { args, kwargs }
    }

    /// Get the argument with the given name.
    /// If the argument is not found by name, return
    /// `None`.
    pub fn keyword_argument(&self, name: &str) -> Option<&'a Expr> {
        self.kwargs.get(name).copied()
    }

    /// Get the argument with the given name or position.
    /// If the argument is not found with either name or position, return
    /// `None`.
    pub fn argument(&self, name: &str, position: usize) -> Option<&'a Expr> {
        self.keyword_argument(name)
            .or_else(|| self.args.get(position).copied())
    }

    /// Return the number of positional and keyword arguments.
    pub fn len(&self) -> usize {
        self.args.len() + self.kwargs.len()
    }

    /// Return `true` if there are no positional or keyword arguments.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Check if a node is parent of a conditional branch.
pub fn on_conditional_branch<'a>(parents: &mut impl Iterator<Item = &'a Stmt>) -> bool {
    parents.any(|parent| {
        if matches!(
            parent.node,
            StmtKind::If { .. } | StmtKind::While { .. } | StmtKind::Match { .. }
        ) {
            return true;
        }
        if let StmtKind::Expr { value } = &parent.node {
            if matches!(value.node, ExprKind::IfExp { .. }) {
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
            parent.node,
            StmtKind::Try { .. }
                | StmtKind::TryStar { .. }
                | StmtKind::If { .. }
                | StmtKind::With { .. }
                | StmtKind::Match { .. }
        )
    })
}

/// Check if a node represents an unpacking assignment.
pub fn is_unpacking_assignment(parent: &Stmt, child: &Expr) -> bool {
    match &parent.node {
        StmtKind::With { items, .. } => items.iter().any(|item| {
            if let Some(optional_vars) = &item.optional_vars {
                if matches!(optional_vars.node, ExprKind::Tuple { .. }) {
                    if any_over_expr(optional_vars, &|expr| expr == child) {
                        return true;
                    }
                }
            }
            false
        }),
        StmtKind::Assign { targets, value, .. } => {
            // In `(a, b) = (1, 2)`, `(1, 2)` is the target, and it is a tuple.
            let value_is_tuple = matches!(
                &value.node,
                ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
            );
            // In `(a, b) = coords = (1, 2)`, `(a, b)` and `coords` are the targets, and
            // `(a, b)` is a tuple. (We use "tuple" as a placeholder for any
            // unpackable expression.)
            let targets_are_tuples = targets.iter().all(|item| {
                matches!(
                    item.node,
                    ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
                )
            });
            // If we're looking at `a` in `(a, b) = coords = (1, 2)`, then we should
            // identify that the current expression is in a tuple.
            let child_in_tuple = targets_are_tuples
                || targets.iter().any(|item| {
                    matches!(
                        item.node,
                        ExprKind::Set { .. } | ExprKind::List { .. } | ExprKind::Tuple { .. }
                    ) && any_over_expr(item, &|expr| expr == child)
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

pub type LocatedCmpop<U = ()> = Located<Cmpop, U>;

/// Extract all [`Cmpop`] operators from a source code snippet, with appropriate
/// ranges.
///
/// `RustPython` doesn't include line and column information on [`Cmpop`] nodes.
/// `CPython` doesn't either. This method iterates over the token stream and
/// re-identifies [`Cmpop`] nodes, annotating them with valid ranges.
pub fn locate_cmpops(contents: &str) -> Vec<LocatedCmpop> {
    let mut tok_iter = lexer::lex(contents, Mode::Module).flatten().peekable();
    let mut ops: Vec<LocatedCmpop> = vec![];
    let mut count: usize = 0;
    loop {
        let Some((start, tok, end)) = tok_iter.next() else {
            break;
        };
        if matches!(tok, Tok::Lpar) {
            count += 1;
            continue;
        } else if matches!(tok, Tok::Rpar) {
            count -= 1;
            continue;
        }
        if count == 0 {
            match tok {
                Tok::Not => {
                    if let Some((_, _, end)) =
                        tok_iter.next_if(|(_, tok, _)| matches!(tok, Tok::In))
                    {
                        ops.push(LocatedCmpop::new(start, end, Cmpop::NotIn));
                    }
                }
                Tok::In => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::In));
                }
                Tok::Is => {
                    let op = if let Some((_, _, end)) =
                        tok_iter.next_if(|(_, tok, _)| matches!(tok, Tok::Not))
                    {
                        LocatedCmpop::new(start, end, Cmpop::IsNot)
                    } else {
                        LocatedCmpop::new(start, end, Cmpop::Is)
                    };
                    ops.push(op);
                }
                Tok::NotEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::NotEq));
                }
                Tok::EqEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::Eq));
                }
                Tok::GreaterEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::GtE));
                }
                Tok::Greater => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::Gt));
                }
                Tok::LessEqual => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::LtE));
                }
                Tok::Less => {
                    ops.push(LocatedCmpop::new(start, end, Cmpop::Lt));
                }
                _ => {}
            }
        }
    }
    ops
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use anyhow::Result;
    use rustpython_parser as parser;
    use rustpython_parser::ast::{Cmpop, Location};

    use crate::helpers::{
        elif_else_range, else_range, first_colon_range, identifier_range, locate_cmpops,
        match_trailing_content, resolve_imported_module_path, LocatedCmpop,
    };
    use crate::source_code::Locator;
    use crate::types::Range;

    #[test]
    fn trailing_content() -> Result<()> {
        let contents = "x = 1";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        let contents = "x = 1; y = 2";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert!(match_trailing_content(stmt, &locator));

        let contents = "x = 1  ";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        let contents = "x = 1  # Comment";
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        let contents = r#"
x = 1
y = 2
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert!(!match_trailing_content(stmt, &locator));

        Ok(())
    }

    #[test]
    fn extract_identifier_range() -> Result<()> {
        let contents = "def f(): pass".trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range::new(Location::new(1, 4), Location::new(1, 5),)
        );

        let contents = r#"
def \
  f():
  pass
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range::new(Location::new(2, 2), Location::new(2, 3),)
        );

        let contents = "class Class(): pass".trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range::new(Location::new(1, 6), Location::new(1, 11),)
        );

        let contents = "class Class: pass".trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range::new(Location::new(1, 6), Location::new(1, 11),)
        );

        let contents = r#"
@decorator()
class Class():
  pass
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range::new(Location::new(2, 6), Location::new(2, 11),)
        );

        let contents = r#"x = y + 1"#.trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        assert_eq!(
            identifier_range(stmt, &locator),
            Range::new(Location::new(1, 0), Location::new(1, 9),)
        );

        Ok(())
    }

    #[test]
    fn resolve_import() {
        // Return the module directly.
        assert_eq!(
            resolve_imported_module_path(None, Some("foo"), None),
            Some(Cow::Borrowed("foo"))
        );

        // Construct the module path from the calling module's path.
        assert_eq!(
            resolve_imported_module_path(
                Some(1),
                Some("foo"),
                Some(&["bar".to_string(), "baz".to_string()])
            ),
            Some(Cow::Owned("bar.foo".to_string()))
        );

        // We can't return the module if it's a relative import, and we don't know the calling
        // module's path.
        assert_eq!(
            resolve_imported_module_path(Some(1), Some("foo"), None),
            None
        );

        // We can't return the module if it's a relative import, and the path goes beyond the
        // calling module's path.
        assert_eq!(
            resolve_imported_module_path(Some(1), Some("foo"), Some(&["bar".to_string()])),
            None,
        );
        assert_eq!(
            resolve_imported_module_path(Some(2), Some("foo"), Some(&["bar".to_string()])),
            None
        );
    }

    #[test]
    fn extract_else_range() -> Result<()> {
        let contents = r#"
for x in y:
    pass
else:
    pass
"#
        .trim();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        let range = else_range(stmt, &locator).unwrap();
        assert_eq!(range.location.row(), 3);
        assert_eq!(range.location.column(), 0);
        assert_eq!(range.end_location.row(), 3);
        assert_eq!(range.end_location.column(), 4);
        Ok(())
    }

    #[test]
    fn extract_first_colon_range() {
        let contents = "with a: pass";
        let locator = Locator::new(contents);
        let range = first_colon_range(
            Range::new(Location::new(1, 0), Location::new(1, contents.len())),
            &locator,
        )
        .unwrap();
        assert_eq!(range.location.row(), 1);
        assert_eq!(range.location.column(), 6);
        assert_eq!(range.end_location.row(), 1);
        assert_eq!(range.end_location.column(), 7);
    }

    #[test]
    fn extract_elif_else_range() -> Result<()> {
        let contents = "
if a:
    ...
elif b:
    ...
"
        .trim_start();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        let range = elif_else_range(stmt, &locator).unwrap();
        assert_eq!(range.location.row(), 3);
        assert_eq!(range.location.column(), 0);
        assert_eq!(range.end_location.row(), 3);
        assert_eq!(range.end_location.column(), 4);
        let contents = "
if a:
    ...
else:
    ...
"
        .trim_start();
        let program = parser::parse_program(contents, "<filename>")?;
        let stmt = program.first().unwrap();
        let locator = Locator::new(contents);
        let range = elif_else_range(stmt, &locator).unwrap();
        assert_eq!(range.location.row(), 3);
        assert_eq!(range.location.column(), 0);
        assert_eq!(range.end_location.row(), 3);
        assert_eq!(range.end_location.column(), 4);
        Ok(())
    }

    #[test]
    fn extract_cmpop_location() {
        assert_eq!(
            locate_cmpops("x == 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::Eq
            )]
        );

        assert_eq!(
            locate_cmpops("x != 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::NotEq
            )]
        );

        assert_eq!(
            locate_cmpops("x is 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::Is
            )]
        );

        assert_eq!(
            locate_cmpops("x is not 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 8),
                Cmpop::IsNot
            )]
        );

        assert_eq!(
            locate_cmpops("x in 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::In
            )]
        );

        assert_eq!(
            locate_cmpops("x not in 1"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 8),
                Cmpop::NotIn
            )]
        );

        assert_eq!(
            locate_cmpops("x != (1 is not 2)"),
            vec![LocatedCmpop::new(
                Location::new(1, 2),
                Location::new(1, 4),
                Cmpop::NotEq
            )]
        );
    }
}
