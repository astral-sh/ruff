use crate::AstNode as _;
use crate::flat::{self};
use std::borrow::Cow;
use std::path::Path;

use rustc_hash::FxHashMap;

use ruff_python_trivia::{SimpleTokenKind, SimpleTokenizer, indentation_at_offset};
use ruff_source_file::LineRanges;
use ruff_text_size::{Ranged, TextLen, TextRange, TextSize};

use crate::flat::SyntaxNode;
use crate::flat::statement_visitor::StatementVisitor;
use crate::flat::visitor::Visitor;
use crate::name::{QualifiedName, QualifiedNameBuilder};
use crate::token::{Tokens, parenthesized_range};
use crate::{self as ast, CmpOp, Expr, InterpolatedStringElement, NodeKind, Stmt};

/// Return `true` if the `Stmt` is a compound statement (as opposed to a simple statement).
pub fn is_compound_statement(stmt: Stmt<'_>) -> bool {
    matches!(
        stmt.classify(),
        flat::StmtKind::FunctionDef(_)
            | flat::StmtKind::ClassDef(_)
            | flat::StmtKind::While(_)
            | flat::StmtKind::For(_)
            | flat::StmtKind::Match(_)
            | flat::StmtKind::With(_)
            | flat::StmtKind::If(_)
            | flat::StmtKind::Try(_)
    )
}

pub(crate) fn is_iterable_initializer<F>(id: &str, is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    matches!(id, "list" | "tuple" | "set" | "dict" | "frozenset") && is_builtin(id)
}

/// Whether an expression has no side effects, may have side effects,
/// or is assumed to have side effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SideEffect {
    /// The expression is definitely side-effect-free.
    Absent,
    /// The expression may have side effects (e.g., f-string interpolation
    /// may invoke `__format__` or `__str__`).
    Possible,
    /// The expression is assumed to have side effects.
    Present,
}

impl SideEffect {
    pub const fn is_present(self) -> bool {
        matches!(self, Self::Present)
    }

    pub const fn is_absent(self) -> bool {
        matches!(self, Self::Absent)
    }

    #[must_use]
    pub const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Present, _) | (_, Self::Present) => Self::Present,
            (Self::Possible, _) | (_, Self::Possible) => Self::Possible,
            _ => Self::Absent,
        }
    }

    /// Classify a single expression node's side effect.
    fn from_expr(expr: Expr<'_>, is_builtin: &dyn Fn(&str) -> bool) -> Self {
        match expr.fields() {
            // Empty initializers for known builtins are side-effect-free.
            ast::ExprFields::Call(ast::ExprCallFields {
                func, arguments, ..
            }) if arguments.is_empty() => {
                if let ast::ExprFields::Name(ast::ExprNameFields { id, .. }) = func.fields() {
                    if is_iterable_initializer(id.as_str(), |id| is_builtin(id)) {
                        return Self::Absent;
                    }
                }
                Self::Present
            }

            // Overloaded operators: only side-effect-free if both sides are literals.
            ast::ExprFields::BinOp(ast::ExprBinOpFields { left, right, .. }) => {
                if is_known_safe_binop_operand(left) && is_known_safe_binop_operand(right) {
                    Self::Absent
                } else {
                    Self::Present
                }
            }

            // Non-literal f-string interpolation may invoke `__format__`/`__str__`.
            ast::ExprFields::FString(ast::ExprFStringFields { value, .. }) => {
                if value.elements().any(has_uncertain_interpolation) {
                    Self::Possible
                } else {
                    Self::Absent
                }
            }
            ast::ExprFields::TString(ast::ExprTStringFields { value, .. }) => {
                if value.elements().any(has_uncertain_interpolation) {
                    Self::Possible
                } else {
                    Self::Absent
                }
            }

            // Named expressions (walrus operator) are assignments.
            ast::ExprFields::Named(_) => Self::Present,

            // Complex expressions that are assumed to have side effects.
            ast::ExprFields::Await(_)
            | ast::ExprFields::Call(_)
            | ast::ExprFields::DictComp(_)
            | ast::ExprFields::Generator(_)
            | ast::ExprFields::ListComp(_)
            | ast::ExprFields::SetComp(_)
            | ast::ExprFields::Subscript(_)
            | ast::ExprFields::Yield(_)
            | ast::ExprFields::YieldFrom(_)
            | ast::ExprFields::IpyEscapeCommand(_) => Self::Present,

            // Side-effect-free expressions — continue walking child nodes.
            ast::ExprFields::BoolOp(_)
            | ast::ExprFields::Compare(_)
            | ast::ExprFields::Dict(_)
            | ast::ExprFields::If(_)
            | ast::ExprFields::Lambda(_)
            | ast::ExprFields::List(_)
            | ast::ExprFields::Set(_)
            | ast::ExprFields::Slice(_)
            | ast::ExprFields::Starred(_)
            | ast::ExprFields::Tuple(_)
            | ast::ExprFields::UnaryOp(_)
            | ast::ExprFields::Attribute(_)
            | ast::ExprFields::Name(_)
            | ast::ExprFields::StringLiteral(_)
            | ast::ExprFields::BytesLiteral(_)
            | ast::ExprFields::NumberLiteral(_)
            | ast::ExprFields::BooleanLiteral(_)
            | ast::ExprFields::NoneLiteral(_)
            | ast::ExprFields::EllipsisLiteral(_) => Self::Absent,
        }
    }
}

fn is_known_safe_binop_operand(expr: Expr<'_>) -> bool {
    match expr.classify() {
        flat::ExprKind::StringLiteral(_)
        | flat::ExprKind::BytesLiteral(_)
        | flat::ExprKind::NumberLiteral(_)
        | flat::ExprKind::BooleanLiteral(_)
        | flat::ExprKind::NoneLiteral(_)
        | flat::ExprKind::EllipsisLiteral(_)
        | flat::ExprKind::FString(_)
        | flat::ExprKind::List(_)
        | flat::ExprKind::Tuple(_)
        | flat::ExprKind::Set(_)
        | flat::ExprKind::Dict(_)
        | flat::ExprKind::ListComp(_)
        | flat::ExprKind::SetComp(_)
        | flat::ExprKind::DictComp(_) => true,

        flat::ExprKind::BoolOp(_)
        | flat::ExprKind::Named(_)
        | flat::ExprKind::BinOp(_)
        | flat::ExprKind::UnaryOp(_)
        | flat::ExprKind::Lambda(_)
        | flat::ExprKind::If(_)
        | flat::ExprKind::Compare(_)
        | flat::ExprKind::Call(_)
        | flat::ExprKind::Generator(_)
        | flat::ExprKind::Await(_)
        | flat::ExprKind::Yield(_)
        | flat::ExprKind::YieldFrom(_)
        | flat::ExprKind::Attribute(_)
        | flat::ExprKind::Subscript(_)
        | flat::ExprKind::Starred(_)
        | flat::ExprKind::Name(_)
        | flat::ExprKind::Slice(_)
        | flat::ExprKind::IpyEscapeCommand(_)
        | flat::ExprKind::TString(_) => false,
    }
}

fn is_definitely_side_effect_free_interpolation_expr(expr: Expr<'_>) -> bool {
    matches!(
        expr.classify(),
        flat::ExprKind::NumberLiteral(_)
            | flat::ExprKind::BooleanLiteral(_)
            | flat::ExprKind::NoneLiteral(_)
            | flat::ExprKind::EllipsisLiteral(_)
            | flat::ExprKind::StringLiteral(_)
            | flat::ExprKind::BytesLiteral(_)
    )
}

fn has_uncertain_interpolation(element: InterpolatedStringElement<'_>) -> bool {
    match element.classify() {
        flat::InterpolatedStringElementKind::Literal(_) => false,
        flat::InterpolatedStringElementKind::Interpolation(interp) => {
            !is_definitely_side_effect_free_interpolation_expr(interp.expression())
                || interp
                    .format_spec()
                    .as_ref()
                    .is_some_and(|spec| spec.elements().iter().any(has_uncertain_interpolation))
        }
    }
}

/// Return `true` if the `Expr` contains an expression that appears to include a
/// side-effect (like a function call).
///
/// Accepts a closure that determines whether a given name (e.g., `"list"`) is a Python builtin.
pub fn contains_effect<F>(expr: Expr<'_>, is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    side_effect(expr, is_builtin).is_present()
}

/// Return whether `expr` has no side effects, maybe has side effects, or definitely
/// has side effects.
///
/// Unlike [`contains_effect`], which returns a simple `bool`, this function distinguishes
/// between expressions that are definitely side-effect-free, definitely side-effectful,
/// and those that may invoke user-defined code (e.g., formatting a non-literal f-string
/// interpolation can call `__format__` or `__str__`).
pub fn side_effect<F>(expr: Expr<'_>, is_builtin: F) -> SideEffect
where
    F: Fn(&str) -> bool,
{
    let mut effect = SideEffect::Absent;
    any_over_expr(expr, |expr| {
        match SideEffect::from_expr(expr, &is_builtin) {
            SideEffect::Present => {
                effect = SideEffect::Present;
                true
            }
            SideEffect::Possible => {
                effect = effect.merge(SideEffect::Possible);
                false
            }
            SideEffect::Absent => false,
        }
    });
    effect
}

pub use crate::flat::helpers::any_over_expr;

pub fn any_over_body<'a>(
    body: flat::Suite<'a>,
    mut predicate: impl FnMut(Expr<'a>) -> bool,
) -> bool {
    body.iter()
        .any(|stmt| crate::flat::helpers::any_expression(stmt.syntax_node(), &mut predicate))
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
pub fn is_assignment_to_a_dunder(stmt: Stmt<'_>) -> bool {
    // Check whether it's an assignment to a dunder, with or without a type
    // annotation. This is what pycodestyle (as of 2.9.1) does.
    let target = match stmt.classify() {
        flat::StmtKind::Assign(assign) => {
            let Some([target]) = assign.targets().as_array() else {
                return false;
            };
            target
        }
        flat::StmtKind::AnnAssign(assign) => assign.target(),
        _ => return false,
    };
    target
        .as_name_expr()
        .is_some_and(|name| is_dunder(name.id()))
}

/// Return `true` if the [`Expr`] is a singleton (`None`, `True`, `False`, or
/// `...`).
fn is_singleton(expr: Expr<'_>) -> bool {
    matches!(
        expr.classify(),
        flat::ExprKind::NoneLiteral(_)
            | flat::ExprKind::BooleanLiteral(_)
            | flat::ExprKind::EllipsisLiteral(_)
    )
}

/// Return `true` if the [`Expr`] is a literal or tuple of literals.
pub fn is_constant(expr: Expr<'_>) -> bool {
    if let flat::ExprKind::Tuple(tuple) = expr.classify() {
        tuple.iter().all(is_constant)
    } else {
        expr.is_literal_expr()
    }
}

/// Return `true` if the [`Expr`] is a non-singleton constant.
pub fn is_constant_non_singleton(expr: Expr<'_>) -> bool {
    is_constant(expr) && !is_singleton(expr)
}

/// Return `true` if an [`Expr`] is a literal `True`.
pub use crate::flat::helpers::is_const_true;

/// Return `true` if an [`Expr`] is a literal `False`.
pub use crate::flat::helpers::is_const_false;

/// Return `true` if the [`Expr`] is a mutable iterable initializer, like `{}` or `[]`.
pub fn is_mutable_iterable_initializer(expr: Expr<'_>) -> bool {
    matches!(
        expr.classify(),
        flat::ExprKind::Set(_)
            | flat::ExprKind::SetComp(_)
            | flat::ExprKind::List(_)
            | flat::ExprKind::ListComp(_)
            | flat::ExprKind::Dict(_)
            | flat::ExprKind::DictComp(_)
    )
}

/// Extract the names of all handled exceptions.
pub use crate::flat::helpers::extract_handled_exceptions;

/// Given an [`Expr`] that can be callable or not (like a decorator, which could
/// be used with or without explicit call syntax), return the underlying
/// callable.
pub use crate::flat::helpers::map_callable;

/// Given an [`Expr`] that can be a [`ExprSubscript`][ast::ExprSubscript] or not
/// (like an annotation that may be generic or not), return the underlying expr.
pub use crate::flat::helpers::map_subscript;

/// Given an [`Expr`] that can be starred, return the underlying starred expression.
pub fn map_starred(expr: Expr<'_>) -> Expr<'_> {
    if let ast::ExprFields::Starred(ast::ExprStarredFields { value, .. }) = expr.fields() {
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
pub fn uses_magic_variable_access<F>(body: flat::Suite<'_>, is_builtin: F) -> bool
where
    F: Fn(&str) -> bool,
{
    any_over_body(body, |expr| {
        if let ast::ExprFields::Call(ast::ExprCallFields { func, .. }) = expr.fields() {
            if let ast::ExprFields::Name(ast::ExprNameFields { id, .. }) = func.fields() {
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
pub fn format_import_from(level: u32, module: Option<&str>) -> Cow<'_, str> {
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

/// A [`Visitor`] to collect all [`ExprName`](crate::ExprName) nodes in an AST.
#[derive(Debug, Default)]
pub struct NameFinder<'a> {
    /// A map from identifier to defining expression.
    pub names: FxHashMap<&'a str, ast::ExprName<'a>>,
}

impl<'a> Visitor<'a> for NameFinder<'a> {
    fn visit_expr(&mut self, expr: Expr<'a>) {
        if let flat::ExprKind::Name(name) = expr.classify() {
            self.names.insert(name.id(), name);
        }
        crate::flat::visitor::walk_expr(self, expr);
    }
}

/// A [`Visitor`] to collect all stored [`ExprName`](crate::ExprName) nodes in an AST.
#[derive(Debug, Default)]
pub struct StoredNameFinder<'a> {
    /// A map from identifier to defining expression.
    pub names: FxHashMap<&'a str, ast::ExprName<'a>>,
}

impl<'a> Visitor<'a> for StoredNameFinder<'a> {
    fn visit_expr(&mut self, expr: Expr<'a>) {
        if let flat::ExprKind::Name(name) = expr.classify() {
            if name.ctx().is_store() {
                self.names.insert(name.id(), name);
            }
        }
        crate::flat::visitor::walk_expr(self, expr);
    }
}

/// A [`Visitor`] that collects all `return` statements in a function or method.
#[derive(Default)]
pub struct ReturnStatementVisitor<'a> {
    pub returns: Vec<ast::StmtReturn<'a>>,
    pub is_generator: bool,
}

impl<'a> ReturnStatementVisitor<'a> {
    /// Collect returns in the current scope and determine whether it yields. The body row
    /// range can rule out both kinds of node before following any child handles.
    pub fn for_function(function: ast::StmtFunctionDef<'a>) -> Self {
        let mut visitor = Self::default();
        if function.body_may_contain_yield() {
            Visitor::visit_body(&mut visitor, function.body());
        } else if function
            .body_row_kinds()
            .is_none_or(|mut kinds| kinds.any(|kind| kind == NodeKind::StmtReturn))
        {
            StatementVisitor::visit_body(&mut visitor, function.body());
        }
        visitor
    }

    fn collect_stmt(&mut self, stmt: Stmt<'a>, walk: impl FnOnce(&mut Self, Stmt<'a>)) {
        match stmt.classify() {
            flat::StmtKind::FunctionDef(_) | flat::StmtKind::ClassDef(_) => {
                // Don't recurse.
            }
            flat::StmtKind::Return(stmt) => self.returns.push(stmt),
            _ => walk(self, stmt),
        }
    }
}

impl<'a> Visitor<'a> for ReturnStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        self.collect_stmt(stmt, crate::flat::visitor::walk_stmt);
    }

    fn visit_expr(&mut self, expr: Expr<'a>) {
        if let flat::ExprKind::Yield(_) | flat::ExprKind::YieldFrom(_) = expr.classify() {
            self.is_generator = true;
        } else {
            crate::flat::visitor::walk_expr(self, expr);
        }
    }
}

impl<'a> StatementVisitor<'a> for ReturnStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        self.collect_stmt(stmt, crate::flat::statement_visitor::walk_stmt);
    }
}

/// A [`StatementVisitor`] that collects all `raise` statements in a function or method.
#[derive(Default)]
pub struct RaiseStatementVisitor<'a> {
    pub raises: Vec<crate::StmtRaise<'a>>,
}

impl<'a> StatementVisitor<'a> for RaiseStatementVisitor<'a> {
    fn visit_stmt(&mut self, stmt: Stmt<'a>) {
        match stmt.fields() {
            ast::StmtFields::Raise(raise) => {
                self.raises.push(raise.node);
            }
            ast::StmtFields::ClassDef(_)
            | ast::StmtFields::FunctionDef(_)
            | ast::StmtFields::Try(_) => {}
            ast::StmtFields::If(ast::StmtIfFields {
                body,
                elif_else_clauses,
                ..
            }) => {
                crate::flat::statement_visitor::walk_body(self, body);
                for clause in elif_else_clauses {
                    self.visit_elif_else_clause(clause);
                }
            }
            ast::StmtFields::While(ast::StmtWhileFields { body, .. })
            | ast::StmtFields::With(ast::StmtWithFields { body, .. })
            | ast::StmtFields::For(ast::StmtForFields { body, .. }) => {
                crate::flat::statement_visitor::walk_body(self, body);
            }
            ast::StmtFields::Match(ast::StmtMatchFields { cases, .. }) => {
                for case in cases {
                    crate::flat::statement_visitor::walk_body(self, case.body());
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
    fn visit_stmt(&mut self, stmt: Stmt<'_>) {
        match stmt.fields() {
            ast::StmtFields::FunctionDef(_) | ast::StmtFields::ClassDef(_) => (),
            ast::StmtFields::With(ast::StmtWithFields { is_async: true, .. }) => {
                self.seen_await = true;
            }
            ast::StmtFields::For(ast::StmtForFields { is_async: true, .. }) => {
                self.seen_await = true;
            }
            _ => crate::flat::visitor::walk_stmt(self, stmt),
        }
    }

    fn visit_expr(&mut self, expr: Expr<'_>) {
        if let ast::ExprFields::Await(ast::ExprAwaitFields { .. }) = expr.fields() {
            self.seen_await = true;
        } else {
            crate::flat::visitor::walk_expr(self, expr);
        }
    }

    fn visit_comprehension(&mut self, comprehension: crate::Comprehension<'_>) {
        if comprehension.is_async() {
            self.seen_await = true;
        } else {
            crate::flat::visitor::walk_comprehension(self, comprehension);
        }
    }
}

/// Return `true` if a `Stmt` is a docstring.
pub fn is_docstring_stmt(stmt: Stmt<'_>) -> bool {
    stmt.as_expr_stmt()
        .is_some_and(|expr| expr.value().is_string_literal_expr())
}

/// Returns `true` if all statements in `body` are `pass` or `...` (ellipsis)
///
/// An empty body (`[]`) returns `false`
pub fn is_stub_body(body: flat::Suite<'_>) -> bool {
    !body.is_empty()
        && body.iter().all(|stmt| match stmt.classify() {
            flat::StmtKind::Pass(_) => true,
            flat::StmtKind::Expr(stmt) => stmt.value().is_ellipsis_literal_expr(),
            _ => false,
        })
}

/// Returns `body` without its leading docstring statement, if present.
pub fn body_without_leading_docstring(body: flat::Suite<'_>) -> flat::Suite<'_> {
    match body.split_first() {
        Some((first, rest)) if is_docstring_stmt(first) => rest,
        _ => body,
    }
}

/// Check if a node is part of a conditional branch.
pub fn on_conditional_branch<'a>(parents: &mut impl Iterator<Item = Stmt<'a>>) -> bool {
    parents.any(|parent| match parent.classify() {
        flat::StmtKind::If(_) | flat::StmtKind::While(_) | flat::StmtKind::Match(_) => true,
        flat::StmtKind::Expr(stmt) => stmt.value().is_if_expr(),
        _ => false,
    })
}

/// Check if a node is in a nested block.
pub fn in_nested_block<'a>(mut parents: impl Iterator<Item = Stmt<'a>>) -> bool {
    parents.any(|parent| {
        matches!(
            parent.classify(),
            flat::StmtKind::Try(_)
                | flat::StmtKind::If(_)
                | flat::StmtKind::With(_)
                | flat::StmtKind::Match(_)
        )
    })
}

/// Check if a node represents an unpacking assignment.
pub fn is_unpacking_assignment(parent: Stmt<'_>, child: Expr<'_>) -> bool {
    match parent.classify() {
        flat::StmtKind::With(with_stmt) => with_stmt.items().iter().any(|item| {
            item.optional_vars().is_some_and(|target| {
                target.is_tuple_expr() && any_over_expr(target, |expr| expr == child)
            })
        }),
        flat::StmtKind::Assign(assign) => {
            let targets = assign.targets();
            let value = assign.value();
            // In `(a, b) = (1, 2)`, `(1, 2)` is the target, and it is a tuple.
            let value_is_tuple = matches!(
                value.classify(),
                flat::ExprKind::Set(_) | flat::ExprKind::List(_) | flat::ExprKind::Tuple(_)
            );
            // In `(a, b) = coords = (1, 2)`, `(a, b)` and `coords` are the targets, and
            // `(a, b)` is a tuple. (We use "tuple" as a placeholder for any
            // unpackable expression.)
            let targets_are_tuples = targets.iter().all(|item| {
                matches!(
                    item.classify(),
                    flat::ExprKind::Set(_) | flat::ExprKind::List(_) | flat::ExprKind::Tuple(_)
                )
            });
            // If we're looking at `a` in `(a, b) = coords = (1, 2)`, then we should
            // identify that the current expression is in a tuple.
            let child_in_tuple = targets_are_tuples
                || targets.iter().any(|item| {
                    matches!(
                        item.classify(),
                        flat::ExprKind::Set(_) | flat::ExprKind::List(_) | flat::ExprKind::Tuple(_)
                    ) && any_over_expr(item, |expr| expr == child)
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
    /// Returns a boolean value when the truthiness is known.
    pub fn into_bool(self) -> Option<bool> {
        match self {
            Self::True | Self::Truthy => Some(true),
            Self::False | Self::Falsey => Some(false),
            Self::None => Some(false),
            Self::Unknown => None,
        }
    }
}

pub use crate::flat::helpers::is_empty_f_string;

pub fn generate_comparison<'a>(
    left: Expr<'_>,
    ops: &[CmpOp],
    comparators: impl IntoIterator<Item = Expr<'a>, IntoIter: DoubleEndedIterator + Clone>,
    parent: SyntaxNode,
    tokens: &Tokens,
    source: &str,
) -> String {
    let comparators = comparators.into_iter();
    let start = left.start();
    let end = comparators
        .clone()
        .next_back()
        .map_or_else(|| left.end(), |node| node.end());
    let mut contents = String::with_capacity(usize::from(end - start));

    // Add the left side of the comparison.
    contents.push_str(&source[parenthesized_range(left, parent, tokens).unwrap_or(left.range())]);

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
            &source[parenthesized_range(comparator, parent, tokens).unwrap_or(comparator.range())],
        );
    }

    contents
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
    preceding: SyntaxNode,
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

pub use crate::flat::helpers::is_dotted_name;

#[cfg(test)]
mod tests {
    use crate::helpers::resolve_imported_module_path;
    use std::borrow::Cow;
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
}
