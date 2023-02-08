use std::path::Path;

use itertools::Itertools;
use log::error;
use once_cell::sync::Lazy;
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use rustpython_parser::ast::{
    Arguments, Constant, Excepthandler, ExcepthandlerKind, Expr, ExprKind, Keyword, KeywordData,
    Located, Location, Stmt, StmtKind,
};
use rustpython_parser::lexer;
use rustpython_parser::lexer::Tok;
use rustpython_parser::token::StringKind;
use smallvec::{smallvec, SmallVec};

use crate::ast::types::{Binding, BindingKind, CallPath, Range};
use crate::ast::visitor;
use crate::ast::visitor::Visitor;
use crate::checkers::ast::Checker;
use crate::source_code::{Generator, Indexer, Locator, Stylist};

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

fn collect_call_path_inner<'a>(expr: &'a Expr, parts: &mut CallPath<'a>) -> bool {
    match &expr.node {
        ExprKind::Attribute { value, attr, .. } => {
            if collect_call_path_inner(value, parts) {
                parts.push(attr);
                true
            } else {
                false
            }
        }
        ExprKind::Name { id, .. } => {
            parts.push(id);
            true
        }
        _ => false,
    }
}

/// Convert an `Expr` to its [`CallPath`] segments (like `["typing", "List"]`).
pub fn collect_call_path(expr: &Expr) -> CallPath {
    let mut segments = smallvec![];
    collect_call_path_inner(expr, &mut segments);
    segments
}

/// Convert an `Expr` to its call path (like `List`, or `typing.List`).
pub fn compose_call_path(expr: &Expr) -> Option<String> {
    let call_path = collect_call_path(expr);
    if call_path.is_empty() {
        None
    } else {
        Some(format_call_path(&call_path))
    }
}

/// Format a call path for display.
pub fn format_call_path(call_path: &[&str]) -> String {
    if call_path
        .first()
        .expect("Unable to format empty call path")
        .is_empty()
    {
        call_path[1..].join(".")
    } else {
        call_path.join(".")
    }
}

/// Return `true` if the `Expr` contains a reference to `${module}.${target}`.
pub fn contains_call_path(checker: &Checker, expr: &Expr, target: &[&str]) -> bool {
    any_over_expr(expr, &|expr| {
        checker
            .resolve_call_path(expr)
            .map_or(false, |call_path| call_path.as_slice() == target)
    })
}

/// Return `true` if the `Expr` contains an expression that appears to include a
/// side-effect (like a function call).
pub fn contains_effect(checker: &Checker, expr: &Expr) -> bool {
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
                    let is_empty_initializer = (id == "set"
                        || id == "list"
                        || id == "tuple"
                        || id == "dict"
                        || id == "frozenset")
                        && checker.is_builtin(id);
                    return !is_empty_initializer;
                }
            }
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
        // TODO(charlie): Handle match statements.
        StmtKind::Match { .. } => false,
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
pub fn extract_handler_names(handlers: &[Excepthandler]) -> Vec<CallPath> {
    // TODO(charlie): Use `resolve_call_path` to avoid false positives for
    // overridden builtins.
    let mut handler_names = vec![];
    for handler in handlers {
        match &handler.node {
            ExcepthandlerKind::ExceptHandler { type_, .. } => {
                if let Some(type_) = type_ {
                    if let ExprKind::Tuple { elts, .. } = &type_.node {
                        for type_ in elts {
                            let call_path = collect_call_path(type_);
                            if !call_path.is_empty() {
                                handler_names.push(call_path);
                            }
                        }
                    } else {
                        let call_path = collect_call_path(type_);
                        if !call_path.is_empty() {
                            handler_names.push(call_path);
                        }
                    }
                }
            }
        }
    }
    handler_names
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
    for tok in lexer::make_tokenizer(locator.slice_source_code_range(&range)) {
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
pub fn uses_magic_variable_access(checker: &Checker, body: &[Stmt]) -> bool {
    any_over_body(body, &|expr| {
        if let ExprKind::Call { func, .. } = &expr.node {
            checker.resolve_call_path(func).map_or(false, |call_path| {
                call_path.as_slice() == ["", "locals"]
                    || call_path.as_slice() == ["", "globals"]
                    || call_path.as_slice() == ["", "vars"]
                    || call_path.as_slice() == ["", "eval"]
                    || call_path.as_slice() == ["", "exec"]
            })
        } else {
            false
        }
    })
}

/// Format the module name for a relative import.
pub fn format_import_from(level: Option<&usize>, module: Option<&str>) -> String {
    let mut module_name = String::with_capacity(16);
    if let Some(level) = level {
        for _ in 0..*level {
            module_name.push('.');
        }
    }
    if let Some(module) = module {
        module_name.push_str(module);
    }
    module_name
}

/// Format the member reference name for a relative import.
pub fn format_import_from_member(
    level: Option<&usize>,
    module: Option<&str>,
    member: &str,
) -> String {
    let mut full_name = String::with_capacity(
        level.map_or(0, |level| *level)
            + module.as_ref().map_or(0, |module| module.len())
            + 1
            + member.len(),
    );
    if let Some(level) = level {
        for _ in 0..*level {
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

/// Split a target string (like `typing.List`) into (`typing`, `List`).
pub fn to_call_path(target: &str) -> CallPath {
    if target.contains('.') {
        target.split('.').collect()
    } else {
        smallvec!["", target]
    }
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
        .map(std::path::Path::file_stem)
        .map(|path| path.and_then(|path| path.to_os_string().into_string().ok()))
        .collect::<Option<Vec<String>>>()
}

/// Create a call path from a relative import.
pub fn from_relative_import<'a>(module: &'a [String], name: &'a str) -> CallPath<'a> {
    let mut call_path: CallPath = SmallVec::with_capacity(module.len() + 1);

    // Start with the module path.
    call_path.extend(module.iter().map(String::as_str));

    // Remove segments based on the number of dots.
    for _ in 0..name.chars().take_while(|c| *c == '.').count() {
        call_path.pop();
    }

    // Add the remaining segments.
    call_path.extend(name.trim_start_matches('.').split('.'));

    call_path
}

/// A [`Visitor`] that collects all return statements in a function or method.
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
            StmtKind::Return { value } => self.returns.push(value.as_ref().map(|expr| &**expr)),
            _ => visitor::walk_stmt(self, stmt),
        }
    }
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
    let prefix = locator.slice_source_code_range(&range);
    prefix.chars().any(|char| !char.is_whitespace())
}

/// Return `true` if a [`Located`] has trailing content.
pub fn match_trailing_content<T>(located: &Located<T>, locator: &Locator) -> bool {
    let range = Range::new(
        located.end_location.unwrap(),
        Location::new(located.end_location.unwrap().row() + 1, 0),
    );
    let suffix = locator.slice_source_code_range(&range);
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
    let suffix = locator.slice_source_code_range(&range);
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
    let suffix =
        locator.slice_source_code_at(Location::new(stmt.end_location.unwrap().row() + 1, 0));
    suffix
        .lines()
        .take_while(|line| line.trim().is_empty())
        .count()
}

/// Return the range of the first parenthesis pair after a given [`Location`].
pub fn match_parens(start: Location, locator: &Locator) -> Option<Range> {
    let contents = locator.slice_source_code_at(start);
    let mut fix_start = None;
    let mut fix_end = None;
    let mut count: usize = 0;
    for (start, tok, end) in lexer::make_tokenizer_located(contents, start).flatten() {
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
        let contents = locator.slice_source_code_range(&Range::from_located(stmt));
        for (start, tok, end) in lexer::make_tokenizer_located(contents, stmt.location).flatten() {
            if matches!(tok, Tok::Name { .. }) {
                return Range::new(start, end);
            }
        }
        error!("Failed to find identifier for {:?}", stmt);
    }
    Range::from_located(stmt)
}

/// Like `identifier_range`, but accepts a `Binding`.
pub fn binding_range(binding: &Binding, locator: &Locator) -> Range {
    if matches!(
        binding.kind,
        BindingKind::ClassDefinition | BindingKind::FunctionDefinition
    ) {
        binding
            .source
            .as_ref()
            .map_or(binding.range, |source| identifier_range(source, locator))
    } else {
        binding.range
    }
}

// Return the ranges of `Name` tokens within a specified node.
pub fn find_names<T>(located: &Located<T>, locator: &Locator) -> Vec<Range> {
    let contents = locator.slice_source_code_range(&Range::from_located(located));
    lexer::make_tokenizer_located(contents, located.location)
        .flatten()
        .filter(|(_, tok, _)| matches!(tok, Tok::Name { .. }))
        .map(|(start, _, end)| Range {
            location: start,
            end_location: end,
        })
        .collect()
}

/// Return the `Range` of `name` in `Excepthandler`.
pub fn excepthandler_name_range(handler: &Excepthandler, locator: &Locator) -> Option<Range> {
    let ExcepthandlerKind::ExceptHandler {
        name, type_, body, ..
    } = &handler.node;
    match (name, type_) {
        (Some(_), Some(type_)) => {
            let type_end_location = type_.end_location.unwrap();
            let contents =
                locator.slice_source_code_range(&Range::new(type_end_location, body[0].location));
            let range = lexer::make_tokenizer_located(contents, type_end_location)
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
    let contents = locator.slice_source_code_range(&Range {
        location: handler.location,
        end_location: end,
    });
    let range = lexer::make_tokenizer_located(contents, handler.location)
        .flatten()
        .find(|(_, kind, _)| matches!(kind, Tok::Except { .. }))
        .map(|(location, _, end_location)| Range {
            location,
            end_location,
        })
        .expect("Failed to find `except` range");
    range
}

/// Find f-strings that don't contain any formatted values in a `JoinedStr`.
pub fn find_useless_f_strings(expr: &Expr, locator: &Locator) -> Vec<(Range, Range)> {
    let contents = locator.slice_source_code_range(&Range::from_located(expr));
    lexer::make_tokenizer_located(contents, expr.location)
        .flatten()
        .filter_map(|(location, tok, end_location)| match tok {
            Tok::String {
                kind: StringKind::FString | StringKind::RawFString,
                ..
            } => {
                let first_char = locator.slice_source_code_range(&Range {
                    location,
                    end_location: Location::new(location.row(), location.column() + 1),
                });
                // f"..."  => f_position = 0
                // fr"..." => f_position = 0
                // rf"..." => f_position = 1
                let f_position = usize::from(!(first_char == "f" || first_char == "F"));
                Some((
                    Range {
                        location: Location::new(location.row(), location.column() + f_position),
                        end_location: Location::new(
                            location.row(),
                            location.column() + f_position + 1,
                        ),
                    },
                    Range {
                        location,
                        end_location,
                    },
                ))
            }
            _ => None,
        })
        .collect()
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
            let contents = locator.slice_source_code_range(&Range {
                location: body_end,
                end_location: orelse
                    .first()
                    .expect("Expected orelse to be non-empty")
                    .location,
            });
            let range = lexer::make_tokenizer_located(contents, body_end)
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
    let contents = locator.slice_source_code_range(&range);
    let range = lexer::make_tokenizer_located(contents, range.location)
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
    let contents = locator.slice_source_code_range(&Range::new(start, end));
    let range = lexer::make_tokenizer_located(contents, start)
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
    pub fn new(args: &'a [Expr], keywords: &'a [Keyword]) -> Self {
        let mut result = SimpleCallArgs::default();

        for arg in args {
            match &arg.node {
                ExprKind::Starred { .. } => {
                    break;
                }
                _ => {
                    result.args.push(arg);
                }
            }
        }

        for keyword in keywords {
            if let Some(arg) = &keyword.node.arg {
                result.kwargs.insert(arg, &keyword.node.value);
            }
        }

        result
    }

    /// Get the argument with the given name or position.
    /// If the argument is not found with either name or position, return
    /// `None`.
    pub fn get_argument(&self, name: &'a str, position: Option<usize>) -> Option<&'a Expr> {
        if let Some(kwarg) = self.kwargs.get(name) {
            return Some(kwarg);
        }
        if let Some(position) = position {
            if position < self.args.len() {
                return Some(self.args[position]);
            }
        }
        None
    }

    /// Get the number of positional and keyword arguments used.
    pub fn len(&self) -> usize {
        self.args.len() + self.kwargs.len()
    }
}

/// Return `true` if the given `Expr` is a potential logging call. Matches
/// `logging.error`, `logger.error`, `self.logger.error`, etc., but not
/// arbitrary `foo.error` calls.
pub fn is_logger_candidate(func: &Expr) -> bool {
    if let ExprKind::Attribute { value, .. } = &func.node {
        let call_path = collect_call_path(value);
        if let Some(tail) = call_path.last() {
            if tail.starts_with("log") || tail.ends_with("logger") || tail.ends_with("logging") {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use rustpython_parser::ast::Location;
    use rustpython_parser::parser;

    use crate::ast::helpers::{
        elif_else_range, else_range, first_colon_range, identifier_range, match_trailing_content,
    };
    use crate::ast::types::Range;
    use crate::source_code::Locator;

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
    fn test_else_range() -> Result<()> {
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
    fn test_first_colon_range() {
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
    fn test_elif_else_range() -> Result<()> {
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
}
