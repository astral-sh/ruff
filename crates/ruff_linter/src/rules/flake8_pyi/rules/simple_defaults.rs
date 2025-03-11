use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix, Violation};
use ruff_macros::{derive_message_formats, ViolationMetadata};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Expr, Operator, Parameters, Stmt, UnaryOp};
use ruff_python_semantic::{analyze::class::is_enumeration, ScopeKind, SemanticModel};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::importer::ImportRequest;
use crate::rules::flake8_pyi::rules::TypingModule;
use crate::Locator;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks for typed function arguments in stubs with complex default values.
///
/// ## Why is this bad?
/// Stub (`.pyi`) files exist as "data files" for static analysis tools, and
/// are not evaluated at runtime. While simple default values may be useful for
/// some tools that consume stubs, such as IDEs, they are ignored by type
/// checkers.
///
/// Instead of including and reproducing a complex value, use `...` to indicate
/// that the assignment has a default value, but that the value is "complex" or
/// varies according to the current platform or Python version. For the
/// purposes of this rule, any default value counts as "complex" unless it is
/// a literal `int`, `float`, `complex`, `bytes`, `str`, `bool`, `None`, `...`,
/// or a simple container literal.
///
/// ## Example
///
/// ```pyi
/// def foo(arg: list[int] = list(range(10_000))) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def foo(arg: list[int] = ...) -> None: ...
/// ```
///
/// ## References
/// - [`flake8-pyi`](https://github.com/PyCQA/flake8-pyi/blob/main/ERRORCODES.md)
#[derive(ViolationMetadata)]
pub(crate) struct TypedArgumentDefaultInStub;

impl AlwaysFixableViolation for TypedArgumentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Only simple default values allowed for typed arguments".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace default value with `...`".to_string()
    }
}

/// ## What it does
/// Checks for untyped function arguments in stubs with default values that
/// are not "simple" /// (i.e., `int`, `float`, `complex`, `bytes`, `str`,
/// `bool`, `None`, `...`, or simple container literals).
///
/// ## Why is this bad?
/// Stub (`.pyi`) files exist to define type hints, and are not evaluated at
/// runtime. As such, function arguments in stub files should not have default
/// values, as they are ignored by type checkers.
///
/// However, the use of default values may be useful for IDEs and other
/// consumers of stub files, and so "simple" values may be worth including and
/// are permitted by this rule.
///
/// Instead of including and reproducing a complex value, use `...` to indicate
/// that the assignment has a default value, but that the value is non-simple
/// or varies according to the current platform or Python version.
///
/// ## Example
///
/// ```pyi
/// def foo(arg=[]) -> None: ...
/// ```
///
/// Use instead:
///
/// ```pyi
/// def foo(arg=...) -> None: ...
/// ```
///
/// ## References
/// - [`flake8-pyi`](https://github.com/PyCQA/flake8-pyi/blob/main/ERRORCODES.md)
#[derive(ViolationMetadata)]
pub(crate) struct ArgumentDefaultInStub;

impl AlwaysFixableViolation for ArgumentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Only simple default values allowed for arguments".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace default value with `...`".to_string()
    }
}

/// ## What it does
/// Checks for assignments in stubs with default values that are not "simple"
/// (i.e., `int`, `float`, `complex`, `bytes`, `str`, `bool`, `None`, `...`, or
/// simple container literals).
///
/// ## Why is this bad?
/// Stub (`.pyi`) files exist to define type hints, and are not evaluated at
/// runtime. As such, assignments in stub files should not include values,
/// as they are ignored by type checkers.
///
/// However, the use of such values may be useful for IDEs and other consumers
/// of stub files, and so "simple" values may be worth including and are
/// permitted by this rule.
///
/// Instead of including and reproducing a complex value, use `...` to indicate
/// that the assignment has a default value, but that the value is non-simple
/// or varies according to the current platform or Python version.
///
/// ## Example
/// ```pyi
/// foo: str = "..."
/// ```
///
/// Use instead:
/// ```pyi
/// foo: str = ...
/// ```
///
/// ## References
/// - [`flake8-pyi`](https://github.com/PyCQA/flake8-pyi/blob/main/ERRORCODES.md)
#[derive(ViolationMetadata)]
pub(crate) struct AssignmentDefaultInStub;

impl AlwaysFixableViolation for AssignmentDefaultInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Only simple default values allowed for assignments".to_string()
    }

    fn fix_title(&self) -> String {
        "Replace default value with `...`".to_string()
    }
}

/// ## What it does
/// Checks for unannotated assignments in stub (`.pyi`) files.
///
/// ## Why is this bad?
/// Stub files exist to provide type hints, and are never executed. As such,
/// all assignments in stub files should be annotated with a type.
#[derive(ViolationMetadata)]
pub(crate) struct UnannotatedAssignmentInStub {
    name: String,
}

impl Violation for UnannotatedAssignmentInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnannotatedAssignmentInStub { name } = self;
        format!("Need type annotation for `{name}`")
    }
}

/// ## What it does
/// Checks that `__all__`, `__match_args__`, and `__slots__` variables are
/// assigned to values when defined in stub files.
///
/// ## Why is this bad?
/// Special variables like `__all__` have the same semantics in stub files
/// as they do in Python modules, and so should be consistent with their
/// runtime counterparts.
///
/// ## Example
/// ```pyi
/// __all__: list[str]
/// ```
///
/// Use instead:
/// ```pyi
/// __all__: list[str] = ["foo", "bar"]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct UnassignedSpecialVariableInStub {
    name: String,
}

impl Violation for UnassignedSpecialVariableInStub {
    #[derive_message_formats]
    fn message(&self) -> String {
        let UnassignedSpecialVariableInStub { name } = self;
        format!("`{name}` in a stub file must have a value, as it has the same semantics as `{name}` at runtime")
    }
}

/// ## What it does
/// Checks for type alias definitions that are not annotated with
/// `typing.TypeAlias`.
///
/// ## Why is this bad?
/// In Python, a type alias is defined by assigning a type to a variable (e.g.,
/// `Vector = list[float]`).
///
/// It's best to annotate type aliases with the `typing.TypeAlias` type to
/// make it clear that the statement is a type alias declaration, as opposed
/// to a normal variable assignment.
///
/// ## Example
/// ```pyi
/// Vector = list[float]
/// ```
///
/// Use instead:
/// ```pyi
/// from typing import TypeAlias
///
/// Vector: TypeAlias = list[float]
/// ```
#[derive(ViolationMetadata)]
pub(crate) struct TypeAliasWithoutAnnotation {
    module: TypingModule,
    name: String,
    value: String,
}

impl AlwaysFixableViolation for TypeAliasWithoutAnnotation {
    #[derive_message_formats]
    fn message(&self) -> String {
        let TypeAliasWithoutAnnotation {
            module,
            name,
            value,
        } = self;
        format!("Use `{module}.TypeAlias` for type alias, e.g., `{name}: TypeAlias = {value}`")
    }

    fn fix_title(&self) -> String {
        "Add `TypeAlias` annotation".to_string()
    }
}

fn is_allowed_negated_math_attribute(qualified_name: &QualifiedName) -> bool {
    matches!(
        qualified_name.segments(),
        ["math", "inf" | "e" | "pi" | "tau"]
    )
}

fn is_allowed_math_attribute(qualified_name: &QualifiedName) -> bool {
    matches!(
        qualified_name.segments(),
        ["math", "inf" | "nan" | "e" | "pi" | "tau"]
            | [
                "sys",
                "stdin"
                    | "stdout"
                    | "stderr"
                    | "version"
                    | "version_info"
                    | "platform"
                    | "executable"
                    | "prefix"
                    | "exec_prefix"
                    | "base_prefix"
                    | "byteorder"
                    | "maxsize"
                    | "hexversion"
                    | "winver"
            ]
    )
}

fn is_valid_default_value_with_annotation(
    default: &Expr,
    allow_container: bool,
    locator: &Locator,
    semantic: &SemanticModel,
) -> bool {
    match default {
        Expr::StringLiteral(_)
        | Expr::BytesLiteral(_)
        | Expr::NumberLiteral(_)
        | Expr::BooleanLiteral(_)
        | Expr::NoneLiteral(_)
        | Expr::EllipsisLiteral(_) => {
            return true;
        }
        Expr::List(ast::ExprList { elts, .. })
        | Expr::Tuple(ast::ExprTuple { elts, .. })
        | Expr::Set(ast::ExprSet { elts, range: _ }) => {
            return allow_container
                && elts.len() <= 10
                && elts
                    .iter()
                    .all(|e| is_valid_default_value_with_annotation(e, false, locator, semantic));
        }
        Expr::Dict(dict) => {
            return allow_container
                && dict.len() <= 10
                && dict.iter().all(|ast::DictItem { key, value }| {
                    key.as_ref().is_some_and(|key| {
                        is_valid_default_value_with_annotation(key, false, locator, semantic)
                    }) && is_valid_default_value_with_annotation(value, false, locator, semantic)
                });
        }
        Expr::UnaryOp(ast::ExprUnaryOp {
            op: UnaryOp::USub,
            operand,
            range: _,
        }) => {
            match operand.as_ref() {
                // Ex) `-1`, `-3.14`, `2j`
                Expr::NumberLiteral(_) => return true,
                // Ex) `-math.inf`, `-math.pi`, etc.
                Expr::Attribute(_) => {
                    if semantic
                        .resolve_qualified_name(operand)
                        .as_ref()
                        .is_some_and(is_allowed_negated_math_attribute)
                    {
                        return true;
                    }
                }
                _ => {}
            }
        }
        Expr::BinOp(ast::ExprBinOp {
            left,
            op: Operator::Add | Operator::Sub,
            right,
            range: _,
        }) => {
            // Ex) `1 + 2j`, `1 - 2j`, `-1 - 2j`, `-1 + 2j`
            if let Expr::NumberLiteral(ast::ExprNumberLiteral {
                value: ast::Number::Complex { .. },
                ..
            }) = right.as_ref()
            {
                // Ex) `1 + 2j`, `1 - 2j`
                if let Expr::NumberLiteral(ast::ExprNumberLiteral {
                    value: ast::Number::Int(..) | ast::Number::Float(..),
                    ..
                }) = left.as_ref()
                {
                    return locator.slice(left.as_ref()).len() <= 10;
                } else if let Expr::UnaryOp(ast::ExprUnaryOp {
                    op: UnaryOp::USub,
                    operand,
                    range: _,
                }) = left.as_ref()
                {
                    // Ex) `-1 + 2j`, `-1 - 2j`
                    if let Expr::NumberLiteral(ast::ExprNumberLiteral {
                        value: ast::Number::Int(..) | ast::Number::Float(..),
                        ..
                    }) = operand.as_ref()
                    {
                        return locator.slice(operand.as_ref()).len() <= 10;
                    }
                }
            }
        }
        // Ex) `math.inf`, `sys.stdin`, etc.
        Expr::Attribute(_) => {
            if semantic
                .resolve_qualified_name(default)
                .as_ref()
                .is_some_and(is_allowed_math_attribute)
            {
                return true;
            }
        }
        _ => {}
    }
    false
}

/// Returns `true` if an [`Expr`] appears to be a valid PEP 604 union. (e.g. `int | None`)
fn is_valid_pep_604_union(annotation: &Expr) -> bool {
    /// Returns `true` if an [`Expr`] appears to be a valid PEP 604 union member.
    fn is_valid_pep_604_union_member(value: &Expr) -> bool {
        match value {
            Expr::BinOp(ast::ExprBinOp {
                left,
                op: Operator::BitOr,
                right,
                range: _,
            }) => is_valid_pep_604_union_member(left) && is_valid_pep_604_union_member(right),
            Expr::Name(_) | Expr::Subscript(_) | Expr::Attribute(_) | Expr::NoneLiteral(_) => true,
            _ => false,
        }
    }

    // The top-level expression must be a bit-or operation.
    let Expr::BinOp(ast::ExprBinOp {
        left,
        op: Operator::BitOr,
        right,
        range: _,
    }) = annotation
    else {
        return false;
    };

    // The left and right operands must be valid union members.
    is_valid_pep_604_union_member(left) && is_valid_pep_604_union_member(right)
}

/// Returns `true` if an [`Expr`] appears to be a valid default value without an annotation.
fn is_valid_default_value_without_annotation(default: &Expr) -> bool {
    matches!(
        default,
        Expr::Call(_)
            | Expr::Name(_)
            | Expr::Attribute(_)
            | Expr::Subscript(_)
            | Expr::EllipsisLiteral(_)
            | Expr::NoneLiteral(_)
    ) || is_valid_pep_604_union(default)
}

/// Returns `true` if an [`Expr`] appears to be `TypeVar`, `TypeVarTuple`, `NewType`, or `ParamSpec`
/// call.
///
/// See also [`ruff_python_semantic::analyze::typing::TypeVarLikeChecker::is_type_var_like_call`].
fn is_type_var_like_call(expr: &Expr, semantic: &SemanticModel) -> bool {
    let Expr::Call(ast::ExprCall { func, .. }) = expr else {
        return false;
    };
    semantic
        .resolve_qualified_name(func)
        .is_some_and(|qualified_name| {
            matches!(
                qualified_name.segments(),
                [
                    "typing" | "typing_extensions",
                    "TypeVar" | "TypeVarTuple" | "NewType" | "ParamSpec"
                ]
            )
        })
}

/// Returns `true` if this is a "special" assignment which must have a value (e.g., an assignment to
/// `__all__`).
fn is_special_assignment(target: &Expr, semantic: &SemanticModel) -> bool {
    if let Expr::Name(ast::ExprName { id, .. }) = target {
        match id.as_str() {
            "__all__" => semantic.current_scope().kind.is_module(),
            "__match_args__" | "__slots__" => semantic.current_scope().kind.is_class(),
            _ => false,
        }
    } else {
        false
    }
}

/// Returns `true` if this is an assignment to a simple `Final`-annotated variable.
fn is_final_assignment(annotation: &Expr, value: &Expr, semantic: &SemanticModel) -> bool {
    if matches!(value, Expr::Name(_) | Expr::Attribute(_)) {
        if semantic.match_typing_expr(annotation, "Final") {
            return true;
        }
    }
    false
}

/// Returns `true` if an [`Expr`] is a value that should be annotated with `typing.TypeAlias`.
///
/// This is relatively conservative, as it's hard to reliably detect whether a right-hand side is a
/// valid type alias. In particular, this function checks for uses of `typing.Any`, `None`,
/// parameterized generics, and PEP 604-style unions.
fn is_annotatable_type_alias(value: &Expr, semantic: &SemanticModel) -> bool {
    if value.is_none_literal_expr() {
        if let ScopeKind::Class(class_def) = semantic.current_scope().kind {
            !is_enumeration(class_def, semantic)
        } else {
            true
        }
    } else {
        value.is_subscript_expr()
            || is_valid_pep_604_union(value)
            || semantic.match_typing_expr(value, "Any")
    }
}

/// PYI011
pub(crate) fn typed_argument_simple_defaults(checker: &Checker, parameters: &Parameters) {
    for parameter in parameters.iter_non_variadic_params() {
        let Some(default) = parameter.default() else {
            continue;
        };
        if parameter.annotation().is_some() {
            if !is_valid_default_value_with_annotation(
                default,
                true,
                checker.locator(),
                checker.semantic(),
            ) {
                let mut diagnostic = Diagnostic::new(TypedArgumentDefaultInStub, default.range());

                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    "...".to_string(),
                    default.range(),
                )));

                checker.report_diagnostic(diagnostic);
            }
        }
    }
}

/// PYI014
pub(crate) fn argument_simple_defaults(checker: &Checker, parameters: &Parameters) {
    for parameter in parameters.iter_non_variadic_params() {
        let Some(default) = parameter.default() else {
            continue;
        };
        if parameter.annotation().is_none() {
            if !is_valid_default_value_with_annotation(
                default,
                true,
                checker.locator(),
                checker.semantic(),
            ) {
                let mut diagnostic = Diagnostic::new(ArgumentDefaultInStub, default.range());

                diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
                    "...".to_string(),
                    default.range(),
                )));

                checker.report_diagnostic(diagnostic);
            }
        }
    }
}

/// PYI015
pub(crate) fn assignment_default_in_stub(checker: &Checker, targets: &[Expr], value: &Expr) {
    let [target] = targets else {
        return;
    };
    if !target.is_name_expr() {
        return;
    }
    if is_special_assignment(target, checker.semantic()) {
        return;
    }
    if is_type_var_like_call(value, checker.semantic()) {
        return;
    }
    if is_valid_default_value_without_annotation(value) {
        return;
    }
    if is_valid_default_value_with_annotation(value, true, checker.locator(), checker.semantic()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssignmentDefaultInStub, value.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "...".to_string(),
        value.range(),
    )));
    checker.report_diagnostic(diagnostic);
}

/// PYI015
pub(crate) fn annotated_assignment_default_in_stub(
    checker: &Checker,
    target: &Expr,
    value: &Expr,
    annotation: &Expr,
) {
    if checker
        .semantic()
        .match_typing_expr(annotation, "TypeAlias")
    {
        return;
    }
    if is_special_assignment(target, checker.semantic()) {
        return;
    }
    if is_type_var_like_call(value, checker.semantic()) {
        return;
    }
    if is_final_assignment(annotation, value, checker.semantic()) {
        return;
    }
    if is_valid_default_value_with_annotation(value, true, checker.locator(), checker.semantic()) {
        return;
    }

    let mut diagnostic = Diagnostic::new(AssignmentDefaultInStub, value.range());
    diagnostic.set_fix(Fix::safe_edit(Edit::range_replacement(
        "...".to_string(),
        value.range(),
    )));
    checker.report_diagnostic(diagnostic);
}

/// PYI052
pub(crate) fn unannotated_assignment_in_stub(checker: &Checker, targets: &[Expr], value: &Expr) {
    let [target] = targets else {
        return;
    };
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };
    let semantic = checker.semantic();
    if is_special_assignment(target, semantic) {
        return;
    }
    if is_type_var_like_call(value, semantic) {
        return;
    }
    if is_valid_default_value_without_annotation(value) {
        return;
    }
    if !is_valid_default_value_with_annotation(value, true, checker.locator(), semantic) {
        return;
    }

    if let ScopeKind::Class(class_def) = semantic.current_scope().kind {
        if is_enumeration(class_def, semantic) {
            return;
        }
    }
    checker.report_diagnostic(Diagnostic::new(
        UnannotatedAssignmentInStub {
            name: id.to_string(),
        },
        value.range(),
    ));
}

/// PYI035
pub(crate) fn unassigned_special_variable_in_stub(checker: &Checker, target: &Expr, stmt: &Stmt) {
    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if !is_special_assignment(target, checker.semantic()) {
        return;
    }

    checker.report_diagnostic(Diagnostic::new(
        UnassignedSpecialVariableInStub {
            name: id.to_string(),
        },
        stmt.range(),
    ));
}

/// PYI026
pub(crate) fn type_alias_without_annotation(checker: &Checker, value: &Expr, targets: &[Expr]) {
    let [target] = targets else {
        return;
    };

    let Expr::Name(ast::ExprName { id, .. }) = target else {
        return;
    };

    if !is_annotatable_type_alias(value, checker.semantic()) {
        return;
    }

    let module = if checker.target_version() >= PythonVersion::PY310 {
        TypingModule::Typing
    } else {
        TypingModule::TypingExtensions
    };

    let mut diagnostic = Diagnostic::new(
        TypeAliasWithoutAnnotation {
            module,
            name: id.to_string(),
            value: checker.generator().expr(value),
        },
        target.range(),
    );
    diagnostic.try_set_fix(|| {
        let (import_edit, binding) = checker.importer().get_or_import_symbol(
            &ImportRequest::import(module.as_str(), "TypeAlias"),
            target.start(),
            checker.semantic(),
        )?;
        Ok(Fix::safe_edits(
            Edit::range_replacement(format!("{id}: {binding}"), target.range()),
            [import_edit],
        ))
    });
    checker.report_diagnostic(diagnostic);
}
