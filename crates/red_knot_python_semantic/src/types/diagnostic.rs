use crate::declare_lint;
use crate::lint::{Level, LintRegistryBuilder, LintStatus};
use crate::types::string_annotation::{
    BYTE_STRING_TYPE_ANNOTATION, ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION, FSTRING_TYPE_ANNOTATION,
    IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION, INVALID_SYNTAX_IN_FORWARD_ANNOTATION,
    RAW_STRING_TYPE_ANNOTATION,
};
use crate::types::{ClassLiteralType, Type};
use ruff_db::diagnostic::{Diagnostic, DiagnosticId, Severity};
use ruff_db::files::File;
use ruff_python_ast::{self as ast, AnyNodeRef};
use ruff_text_size::{Ranged, TextRange};
use std::borrow::Cow;
use std::fmt::Formatter;
use std::ops::Deref;
use std::sync::Arc;

use super::context::InferContext;

/// Registers all known type check lints.
pub(crate) fn register_lints(registry: &mut LintRegistryBuilder) {
    registry.register_lint(&CALL_NON_CALLABLE);
    registry.register_lint(&CALL_POSSIBLY_UNBOUND_METHOD);
    registry.register_lint(&CONFLICTING_DECLARATIONS);
    registry.register_lint(&CONFLICTING_METACLASS);
    registry.register_lint(&CYCLIC_CLASS_DEFINITION);
    registry.register_lint(&DIVISION_BY_ZERO);
    registry.register_lint(&DUPLICATE_BASE);
    registry.register_lint(&INCONSISTENT_MRO);
    registry.register_lint(&INDEX_OUT_OF_BOUNDS);
    registry.register_lint(&INVALID_ASSIGNMENT);
    registry.register_lint(&INVALID_BASE);
    registry.register_lint(&INVALID_CONTEXT_MANAGER);
    registry.register_lint(&INVALID_DECLARATION);
    registry.register_lint(&INVALID_EXCEPTION_CAUGHT);
    registry.register_lint(&INVALID_PARAMETER_DEFAULT);
    registry.register_lint(&INVALID_RAISE);
    registry.register_lint(&INVALID_TYPE_FORM);
    registry.register_lint(&INVALID_TYPE_VARIABLE_CONSTRAINTS);
    registry.register_lint(&NON_SUBSCRIPTABLE);
    registry.register_lint(&NOT_ITERABLE);
    registry.register_lint(&POSSIBLY_UNBOUND_ATTRIBUTE);
    registry.register_lint(&POSSIBLY_UNBOUND_IMPORT);
    registry.register_lint(&POSSIBLY_UNRESOLVED_REFERENCE);
    registry.register_lint(&UNDEFINED_REVEAL);
    registry.register_lint(&UNRESOLVED_ATTRIBUTE);
    registry.register_lint(&UNRESOLVED_IMPORT);
    registry.register_lint(&UNRESOLVED_REFERENCE);
    registry.register_lint(&UNSUPPORTED_OPERATOR);
    registry.register_lint(&ZERO_STEPSIZE_IN_SLICE);

    // String annotations
    registry.register_lint(&BYTE_STRING_TYPE_ANNOTATION);
    registry.register_lint(&ESCAPE_CHARACTER_IN_FORWARD_ANNOTATION);
    registry.register_lint(&FSTRING_TYPE_ANNOTATION);
    registry.register_lint(&IMPLICIT_CONCATENATED_STRING_TYPE_ANNOTATION);
    registry.register_lint(&INVALID_SYNTAX_IN_FORWARD_ANNOTATION);
    registry.register_lint(&RAW_STRING_TYPE_ANNOTATION);
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to non-callable objects.
    ///
    /// ## Why is this bad?
    /// Calling a non-callable object will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// 4()  # TypeError: 'int' object is not callable
    /// ```
    pub(crate) static CALL_NON_CALLABLE = {
        summary: "detects calls to non-callable objects",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to possibly unbound methods.
    ///
    /// TODO #14889
    pub(crate) static CALL_POSSIBLY_UNBOUND_METHOD = {
        summary: "detects calls to possibly unbound methods",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static CONFLICTING_DECLARATIONS = {
        summary: "detects conflicting declarations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static CONFLICTING_METACLASS = {
        summary: "detects conflicting metaclasses",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for class definitions with a cyclic inheritance chain.
    ///
    /// ## Why is it bad?
    /// TODO #14889
    pub(crate) static CYCLIC_CLASS_DEFINITION = {
        summary: "detects cyclic class definitions",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// It detects division by zero.
    ///
    /// ## Why is this bad?
    /// Dividing by zero raises a `ZeroDivisionError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// 5 / 0
    /// ```
    pub(crate) static DIVISION_BY_ZERO = {
        summary: "detects division by zero",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static DUPLICATE_BASE = {
        summary: "detects class definitions with duplicate bases",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INCONSISTENT_MRO = {
        summary: "detects class definitions with an inconsistent MRO",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// TODO #14889
    pub(crate) static INDEX_OUT_OF_BOUNDS = {
        summary: "detects index out of bounds errors",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_ASSIGNMENT = {
        summary: "detects invalid assignments",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_BASE = {
        summary: "detects class definitions with an invalid base",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_CONTEXT_MANAGER = {
        summary: "detects expressions used in with statements that don't implement the context manager protocol",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_DECLARATION = {
        summary: "detects invalid declarations",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// Checks for exception handlers that catch non-exception classes.
    ///
    /// ## Why is this bad?
    /// Catching classes that do not inherit from `BaseException` will raise a TypeError at runtime.
    ///
    /// ## Example
    /// ```python
    /// try:
    ///     1 / 0
    /// except 1:
    ///     ...
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// try:
    ///     1 / 0
    /// except ZeroDivisionError:
    ///     ...
    /// ```
    ///
    /// ## References
    /// - [Python documentation: except clause](https://docs.python.org/3/reference/compound_stmts.html#except-clause)
    /// - [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
    ///
    /// ## Ruff rule
    ///  This rule corresponds to Ruff's [`except-with-non-exception-classes` (`B030`)](https://docs.astral.sh/ruff/rules/except-with-non-exception-classes)
    pub(crate) static INVALID_EXCEPTION_CAUGHT = {
        summary: "detects exception handlers that catch classes that do not inherit from `BaseException`",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for default values that can't be assigned to the parameter's annotated type.
    ///
    /// ## Why is this bad?
    /// TODO #14889
    pub(crate) static INVALID_PARAMETER_DEFAULT = {
        summary: "detects default values that can't be assigned to the parameter's annotated type",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// Checks for `raise` statements that raise non-exceptions or use invalid
    /// causes for their raised exceptions.
    ///
    /// ## Why is this bad?
    /// Only subclasses or instances of `BaseException` can be raised.
    /// For an exception's cause, the same rules apply, except that `None` is also
    /// permitted. Violating these rules results in a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// def f():
    ///     try:
    ///         something()
    ///     except NameError:
    ///         raise "oops!" from f
    ///
    /// def g():
    ///     raise NotImplemented from 42
    /// ```
    ///
    /// Use instead:
    /// ```python
    /// def f():
    ///     try:
    ///         something()
    ///     except NameError as e:
    ///         raise RuntimeError("oops!") from e
    ///
    /// def g():
    ///     raise NotImplementedError from None
    /// ```
    ///
    /// ## References
    /// - [Python documentation: The `raise` statement](https://docs.python.org/3/reference/simple_stmts.html#raise)
    /// - [Python documentation: Built-in Exceptions](https://docs.python.org/3/library/exceptions.html#built-in-exceptions)
    pub(crate) static INVALID_RAISE = {
        summary: "detects `raise` statements that raise invalid exceptions or use invalid causes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for invalid type expressions.
    ///
    /// ## Why is this bad?
    /// TODO #14889
    pub(crate) static INVALID_TYPE_FORM = {
        summary: "detects invalid type forms",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static INVALID_TYPE_VARIABLE_CONSTRAINTS = {
        summary: "detects invalid type variable constraints",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for subscripting objects that do not support subscripting.
    ///
    /// ## Why is this bad?
    /// Subscripting an object that does not support it will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// 4[1]  # TypeError: 'int' object is not subscriptable
    /// ```
    pub(crate) static NON_SUBSCRIPTABLE = {
        summary: "detects subscripting objects that do not support subscripting",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for objects that are not iterable but are used in a context that requires them to be.
    ///
    /// ## Why is this bad?
    /// Iterating over an object that is not iterable will raise a `TypeError` at runtime.
    ///
    /// ## Examples
    ///
    /// ```python
    /// for i in 34:  # TypeError: 'int' object is not iterable
    ///     pass
    /// ```
    pub(crate) static NOT_ITERABLE = {
        summary: "detects iteration over an object that is not iterable",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for possibly unbound attributes.
    ///
    /// TODO #14889
    pub(crate) static POSSIBLY_UNBOUND_ATTRIBUTE = {
        summary: "detects references to possibly unbound attributes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// TODO #14889
    pub(crate) static POSSIBLY_UNBOUND_IMPORT = {
        summary: "detects possibly unbound imports",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for references to names that are possibly not defined.
    ///
    /// ## Why is this bad?
    /// Using an undefined variable will raise a `NameError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// for i in range(0):
    ///     x = i
    ///
    /// print(x)  # NameError: name 'x' is not defined
    /// ```
    pub(crate) static POSSIBLY_UNRESOLVED_REFERENCE = {
        summary: "detects references to possibly undefined names",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for calls to `reveal_type` without importing it.
    ///
    /// ## Why is this bad?
    /// Using `reveal_type` without importing it will raise a `NameError` at runtime.
    ///
    /// ## Examples
    /// TODO #14889
    pub(crate) static UNDEFINED_REVEAL = {
        summary: "detects usages of `reveal_type` without importing it",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for unresolved attributes.
    ///
    /// TODO #14889
    pub(crate) static UNRESOLVED_ATTRIBUTE = {
        summary: "detects references to unresolved attributes",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for import statements for which the module cannot be resolved.
    ///
    /// ## Why is this bad?
    /// Importing a module that cannot be resolved will raise an `ImportError` at runtime.
    pub(crate) static UNRESOLVED_IMPORT = {
        summary: "detects unresolved imports",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for references to names that are not defined.
    ///
    /// ## Why is this bad?
    /// Using an undefined variable will raise a `NameError` at runtime.
    ///
    /// ## Example
    ///
    /// ```python
    /// print(x)  # NameError: name 'x' is not defined
    /// ```
    pub(crate) static UNRESOLVED_REFERENCE = {
        summary: "detects references to names that are not defined",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Warn,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for binary expressions, comparisons, and unary expressions where the operands don't support the operator.
    ///
    /// TODO #14889
    pub(crate) static UNSUPPORTED_OPERATOR = {
        summary: "detects binary, unary, or comparison expressions where the operands don't support the operator",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

declare_lint! {
    /// ## What it does
    /// Checks for step size 0 in slices.
    ///
    /// ## Why is this bad?
    /// A slice with a step size of zero will raise a `ValueError` at runtime.
    ///
    /// ## Examples
    /// ```python
    /// l = list(range(10))
    /// l[1:10:0]  # ValueError: slice step cannot be zero
    /// ```
    pub(crate) static ZERO_STEPSIZE_IN_SLICE = {
        summary: "detects a slice step size of zero",
        status: LintStatus::preview("1.0.0"),
        default_level: Level::Error,
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub struct TypeCheckDiagnostic {
    pub(super) id: DiagnosticId,
    pub(super) message: String,
    pub(super) range: TextRange,
    pub(super) severity: Severity,
    pub(super) file: File,
}

impl TypeCheckDiagnostic {
    pub fn id(&self) -> DiagnosticId {
        self.id
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn file(&self) -> File {
        self.file
    }
}

impl Diagnostic for TypeCheckDiagnostic {
    fn id(&self) -> DiagnosticId {
        self.id
    }

    fn message(&self) -> Cow<str> {
        TypeCheckDiagnostic::message(self).into()
    }

    fn file(&self) -> File {
        TypeCheckDiagnostic::file(self)
    }

    fn range(&self) -> Option<TextRange> {
        Some(Ranged::range(self))
    }

    fn severity(&self) -> Severity {
        self.severity
    }
}

impl Ranged for TypeCheckDiagnostic {
    fn range(&self) -> TextRange {
        self.range
    }
}

/// A collection of type check diagnostics.
///
/// The diagnostics are wrapped in an `Arc` because they need to be cloned multiple times
/// when going from `infer_expression` to `check_file`. We could consider
/// making [`TypeCheckDiagnostic`] a Salsa struct to have them Arena-allocated (once the Tables refactor is done).
/// Using Salsa struct does have the downside that it leaks the Salsa dependency into diagnostics and
/// each Salsa-struct comes with an overhead.
#[derive(Default, Eq, PartialEq)]
pub struct TypeCheckDiagnostics {
    inner: Vec<std::sync::Arc<TypeCheckDiagnostic>>,
}

impl TypeCheckDiagnostics {
    pub(super) fn push(&mut self, diagnostic: TypeCheckDiagnostic) {
        self.inner.push(Arc::new(diagnostic));
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.inner.shrink_to_fit();
    }
}

impl Extend<TypeCheckDiagnostic> for TypeCheckDiagnostics {
    fn extend<T: IntoIterator<Item = TypeCheckDiagnostic>>(&mut self, iter: T) {
        self.inner.extend(iter.into_iter().map(std::sync::Arc::new));
    }
}

impl Extend<std::sync::Arc<TypeCheckDiagnostic>> for TypeCheckDiagnostics {
    fn extend<T: IntoIterator<Item = Arc<TypeCheckDiagnostic>>>(&mut self, iter: T) {
        self.inner.extend(iter);
    }
}

impl<'a> Extend<&'a std::sync::Arc<TypeCheckDiagnostic>> for TypeCheckDiagnostics {
    fn extend<T: IntoIterator<Item = &'a Arc<TypeCheckDiagnostic>>>(&mut self, iter: T) {
        self.inner
            .extend(iter.into_iter().map(std::sync::Arc::clone));
    }
}

impl std::fmt::Debug for TypeCheckDiagnostics {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

impl Deref for TypeCheckDiagnostics {
    type Target = [std::sync::Arc<TypeCheckDiagnostic>];

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl IntoIterator for TypeCheckDiagnostics {
    type Item = Arc<TypeCheckDiagnostic>;
    type IntoIter = std::vec::IntoIter<std::sync::Arc<TypeCheckDiagnostic>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a TypeCheckDiagnostics {
    type Item = &'a Arc<TypeCheckDiagnostic>;
    type IntoIter = std::slice::Iter<'a, std::sync::Arc<TypeCheckDiagnostic>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

/// Emit a diagnostic declaring that the object represented by `node` is not iterable
pub(super) fn report_not_iterable(context: &InferContext, node: AnyNodeRef, not_iterable_ty: Type) {
    context.report_lint(
        &NOT_ITERABLE,
        node,
        format_args!(
            "Object of type `{}` is not iterable",
            not_iterable_ty.display(context.db())
        ),
    );
}

/// Emit a diagnostic declaring that the object represented by `node` is not iterable
/// because its `__iter__` method is possibly unbound.
pub(super) fn report_not_iterable_possibly_unbound(
    context: &InferContext,
    node: AnyNodeRef,
    element_ty: Type,
) {
    context.report_lint(
        &NOT_ITERABLE,
        node,
        format_args!(
            "Object of type `{}` is not iterable because its `__iter__` method is possibly unbound",
            element_ty.display(context.db())
        ),
    );
}

/// Emit a diagnostic declaring that an index is out of bounds for a tuple.
pub(super) fn report_index_out_of_bounds(
    context: &InferContext,
    kind: &'static str,
    node: AnyNodeRef,
    tuple_ty: Type,
    length: usize,
    index: i64,
) {
    context.report_lint(
        &INDEX_OUT_OF_BOUNDS,
        node,
        format_args!(
            "Index {index} is out of bounds for {kind} `{}` with length {length}",
            tuple_ty.display(context.db())
        ),
    );
}

/// Emit a diagnostic declaring that a type does not support subscripting.
pub(super) fn report_non_subscriptable(
    context: &InferContext,
    node: AnyNodeRef,
    non_subscriptable_ty: Type,
    method: &str,
) {
    context.report_lint(
        &NON_SUBSCRIPTABLE,
        node,
        format_args!(
            "Cannot subscript object of type `{}` with no `{method}` method",
            non_subscriptable_ty.display(context.db())
        ),
    );
}

pub(super) fn report_unresolved_module<'db>(
    context: &InferContext,
    import_node: impl Into<AnyNodeRef<'db>>,
    level: u32,
    module: Option<&str>,
) {
    context.report_lint(
        &UNRESOLVED_IMPORT,
        import_node.into(),
        format_args!(
            "Cannot resolve import `{}{}`",
            ".".repeat(level as usize),
            module.unwrap_or_default()
        ),
    );
}

pub(super) fn report_slice_step_size_zero(context: &InferContext, node: AnyNodeRef) {
    context.report_lint(
        &ZERO_STEPSIZE_IN_SLICE,
        node,
        format_args!("Slice step size can not be zero"),
    );
}

pub(super) fn report_invalid_assignment(
    context: &InferContext,
    node: AnyNodeRef,
    declared_ty: Type,
    assigned_ty: Type,
) {
    match declared_ty {
        Type::ClassLiteral(ClassLiteralType { class }) => {
            context.report_lint(&INVALID_ASSIGNMENT, node, format_args!(
                    "Implicit shadowing of class `{}`; annotate to make it explicit if this is intentional",
                    class.name(context.db())));
        }
        Type::FunctionLiteral(function) => {
            context.report_lint(&INVALID_ASSIGNMENT, node, format_args!(
                    "Implicit shadowing of function `{}`; annotate to make it explicit if this is intentional",
                    function.name(context.db())));
        }
        _ => {
            context.report_lint(
                &INVALID_ASSIGNMENT,
                node,
                format_args!(
                    "Object of type `{}` is not assignable to `{}`",
                    assigned_ty.display(context.db()),
                    declared_ty.display(context.db()),
                ),
            );
        }
    }
}

pub(super) fn report_possibly_unresolved_reference(
    context: &InferContext,
    expr_name_node: &ast::ExprName,
) {
    let ast::ExprName { id, .. } = expr_name_node;

    context.report_lint(
        &POSSIBLY_UNRESOLVED_REFERENCE,
        expr_name_node.into(),
        format_args!("Name `{id}` used when possibly not defined"),
    );
}

pub(super) fn report_unresolved_reference(context: &InferContext, expr_name_node: &ast::ExprName) {
    let ast::ExprName { id, .. } = expr_name_node;

    context.report_lint(
        &UNRESOLVED_REFERENCE,
        expr_name_node.into(),
        format_args!("Name `{id}` used when not defined"),
    );
}

pub(super) fn report_invalid_exception_caught(context: &InferContext, node: &ast::Expr, ty: Type) {
    context.report_lint(
        &INVALID_EXCEPTION_CAUGHT,
        node.into(),
        format_args!(
            "Cannot catch object of type `{}` in an exception handler \
            (must be a `BaseException` subclass or a tuple of `BaseException` subclasses)",
            ty.display(context.db())
        ),
    );
}

pub(crate) fn report_invalid_exception_raised(context: &InferContext, node: &ast::Expr, ty: Type) {
    context.report_lint(
        &INVALID_RAISE,
        node.into(),
        format_args!(
            "Cannot raise object of type `{}` (must be a `BaseException` subclass or instance)",
            ty.display(context.db())
        ),
    );
}

pub(crate) fn report_invalid_exception_cause(context: &InferContext, node: &ast::Expr, ty: Type) {
    context.report_lint(
        &INVALID_RAISE,
        node.into(),
        format_args!(
            "Cannot use object of type `{}` as exception cause \
            (must be a `BaseException` subclass or instance or `None`)",
            ty.display(context.db())
        ),
    );
}
