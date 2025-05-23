use ast::{ExprContext, Operator};
use ruff_diagnostics::{AlwaysFixableViolation, Diagnostic, Edit, Fix, FixAvailability, Violation};
use ruff_macros::{ViolationMetadata, derive_message_formats};
use ruff_python_ast as ast;
use ruff_python_ast::{Expr, Stmt};
use ruff_python_semantic::{Binding, SemanticModel, TypingOnlyBindingsStatus};
use ruff_python_stdlib::typing::{is_pep_593_generic_type, is_standard_library_literal};
use ruff_text_size::Ranged;

use crate::checkers::ast::Checker;
use crate::registry::Rule;
use crate::rules::flake8_type_checking::helpers::quote_type_expression;
use ruff_python_ast::PythonVersion;

/// ## What it does
/// Checks if [PEP 613] explicit type aliases contain references to
/// symbols that are not available at runtime.
///
/// ## Why is this bad?
/// Referencing type-checking only symbols results in a `NameError` at runtime.
///
/// ## Example
/// ```python
/// from typing import TYPE_CHECKING, TypeAlias
///
/// if TYPE_CHECKING:
///     from foo import Foo
/// OptFoo: TypeAlias = Foo | None
/// ```
///
/// Use instead:
/// ```python
/// from typing import TYPE_CHECKING, TypeAlias
///
/// if TYPE_CHECKING:
///     from foo import Foo
/// OptFoo: TypeAlias = "Foo | None"
/// ```
///
/// ## Fix safety
/// This rule's fix is currently always marked as unsafe, since runtime
/// typing libraries may try to access/resolve the type alias in a way
/// that we can't statically determine during analysis and relies on the
/// type alias not containing any forward references.
///
/// ## References
/// - [PEP 613 – Explicit Type Aliases](https://peps.python.org/pep-0613/)
///
/// [PEP 613]: https://peps.python.org/pep-0613/
#[derive(ViolationMetadata)]
pub(crate) struct UnquotedTypeAlias;

impl Violation for UnquotedTypeAlias {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    #[derive_message_formats]
    fn message(&self) -> String {
        "Add quotes to type alias".to_string()
    }

    fn fix_title(&self) -> Option<String> {
        Some("Add quotes".to_string())
    }
}

/// ## What it does
/// Checks for unnecessary quotes in [PEP 613] explicit type aliases
/// and [PEP 695] type statements.
///
/// ## Why is this bad?
/// Unnecessary string forward references can lead to additional overhead
/// in runtime libraries making use of type hints. They can also have bad
/// interactions with other runtime uses like [PEP 604] type unions.
///
/// PEP-613 type aliases are only flagged by the rule if Ruff can have high
/// confidence that the quotes are unnecessary. Specifically, any PEP-613
/// type alias where the type expression on the right-hand side contains
/// subscripts or attribute accesses will not be flagged. This is because
/// type aliases can reference types that are, for example, generic in stub
/// files but not at runtime. That can mean that a type checker expects the
/// referenced type to be subscripted with type arguments despite the fact
/// that doing so would fail at runtime if the type alias value was not
/// quoted. Similarly, a type alias might need to reference a module-level
/// attribute that exists in a stub file but not at runtime, meaning that
/// the type alias value would need to be quoted to avoid a runtime error.
///
/// ## Example
/// Given:
/// ```python
/// OptInt: TypeAlias = "int | None"
/// ```
///
/// Use instead:
/// ```python
/// OptInt: TypeAlias = int | None
/// ```
///
/// Given:
/// ```python
/// type OptInt = "int | None"
/// ```
///
/// Use instead:
/// ```python
/// type OptInt = int | None
/// ```
///
/// ## Fix safety
/// This rule's fix is marked as safe, unless the type annotation contains comments.
///
/// ## See also
/// This rule only applies to type aliases in non-stub files. For removing quotes in other
/// contexts or in stub files, see:
///
/// - [`quoted-annotation-in-stub`][PYI020]: A rule that
///   removes all quoted annotations from stub files
/// - [`quoted-annotation`][UP037]: A rule that removes unnecessary quotes
///   from *annotations* in runtime files.
///
/// ## References
/// - [PEP 613 – Explicit Type Aliases](https://peps.python.org/pep-0613/)
/// - [PEP 695: Generic Type Alias](https://peps.python.org/pep-0695/#generic-type-alias)
/// - [PEP 604 – Allow writing union types as `X | Y`](https://peps.python.org/pep-0604/)
///
/// [PEP 604]: https://peps.python.org/pep-0604/
/// [PEP 613]: https://peps.python.org/pep-0613/
/// [PEP 695]: https://peps.python.org/pep-0695/#generic-type-alias
/// [PYI020]: https://docs.astral.sh/ruff/rules/quoted-annotation-in-stub/
/// [UP037]: https://docs.astral.sh/ruff/rules/quoted-annotation/
#[derive(ViolationMetadata)]
pub(crate) struct QuotedTypeAlias;

impl AlwaysFixableViolation for QuotedTypeAlias {
    #[derive_message_formats]
    fn message(&self) -> String {
        "Remove quotes from type alias".to_string()
    }

    fn fix_title(&self) -> String {
        "Remove quotes".to_string()
    }
}

/// TC007
pub(crate) fn unquoted_type_alias(checker: &Checker, binding: &Binding) {
    if binding.context.is_typing() {
        return;
    }

    if !binding.is_annotated_type_alias() {
        return;
    }

    let Some(Stmt::AnnAssign(ast::StmtAnnAssign {
        value: Some(expr), ..
    })) = binding.statement(checker.semantic())
    else {
        return;
    };

    let mut names = Vec::new();
    collect_typing_references(checker, expr, &mut names);
    if names.is_empty() {
        return;
    }

    // We generate a diagnostic for every name that needs to be quoted
    // but we currently emit a single shared fix that quotes the entire
    // expression.
    //
    // Eventually we may try to be more clever and come up with the
    // minimal set of subexpressions that need to be quoted.
    let parent = expr.range().start();
    let edit = quote_type_expression(
        expr,
        checker.semantic(),
        checker.stylist(),
        checker.locator(),
        checker.default_string_flags(),
    );
    for name in names {
        checker.report_diagnostic(
            Diagnostic::new(UnquotedTypeAlias, name.range())
                .with_parent(parent)
                .with_fix(Fix::unsafe_edit(edit.clone())),
        );
    }
}

/// Traverses the type expression and collects `[Expr::Name]` nodes that are
/// not available at runtime and thus need to be quoted, unless they would
/// become available through `[Rule::RuntimeImportInTypeCheckingBlock]`.
fn collect_typing_references<'a>(
    checker: &Checker,
    expr: &'a Expr,
    names: &mut Vec<&'a ast::ExprName>,
) {
    match expr {
        Expr::BinOp(ast::ExprBinOp { left, right, .. }) => {
            collect_typing_references(checker, left, names);
            collect_typing_references(checker, right, names);
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx: ExprContext::Load,
            ..
        })
        | Expr::Attribute(ast::ExprAttribute { value, .. }) => {
            collect_typing_references(checker, value, names);
        }
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            collect_typing_references(checker, value, names);
            if let Some(qualified_name) = checker.semantic().resolve_qualified_name(value) {
                if is_standard_library_literal(qualified_name.segments()) {
                    return;
                }
                if is_pep_593_generic_type(qualified_name.segments()) {
                    // First argument is a type (including forward references); the
                    // rest are arbitrary Python objects.
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        let mut iter = elts.iter();
                        if let Some(expr) = iter.next() {
                            collect_typing_references(checker, expr, names);
                        }
                    }
                    return;
                }
            }
            collect_typing_references(checker, slice, names);
        }
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for elt in elts {
                collect_typing_references(checker, elt, names);
            }
        }
        Expr::Name(name) => {
            let Some(binding_id) = checker.semantic().resolve_name(name) else {
                return;
            };
            if checker
                .semantic()
                .simulate_runtime_load(name, TypingOnlyBindingsStatus::Disallowed)
                .is_some()
            {
                return;
            }

            // if TC004 is enabled we shouldn't emit a TC007 for a reference to
            // a binding that would emit a TC004, otherwise the fixes will never
            // stabilize and keep going in circles
            if checker.enabled(Rule::RuntimeImportInTypeCheckingBlock)
                && checker
                    .semantic()
                    .binding(binding_id)
                    .references()
                    .any(|id| checker.semantic().reference(id).in_runtime_context())
            {
                return;
            }
            names.push(name);
        }
        _ => {}
    }
}

/// TC008
pub(crate) fn quoted_type_alias(
    checker: &Checker,
    expr: &Expr,
    annotation_expr: &ast::ExprStringLiteral,
) {
    if checker.enabled(Rule::RuntimeStringUnion) {
        // this should return a TC010 error instead
        if let Some(Expr::BinOp(ast::ExprBinOp {
            op: Operator::BitOr,
            ..
        })) = checker.semantic().current_expression_parent()
        {
            return;
        }
    }

    // explicit type aliases require some additional checks to avoid false positives
    if checker.semantic().in_annotated_type_alias_value()
        && quotes_are_unremovable(checker.semantic(), expr, checker.target_version())
    {
        return;
    }

    let range = annotation_expr.range();
    let mut diagnostic = Diagnostic::new(QuotedTypeAlias, range);
    let edit = Edit::range_replacement(annotation_expr.value.to_string(), range);
    if checker.comment_ranges().intersects(range) {
        diagnostic.set_fix(Fix::unsafe_edit(edit));
    } else {
        diagnostic.set_fix(Fix::safe_edit(edit));
    }
    checker.report_diagnostic(diagnostic);
}

/// Traverses the type expression and checks if the expression can safely
/// be unquoted
fn quotes_are_unremovable(
    semantic: &SemanticModel,
    expr: &Expr,
    target_version: PythonVersion,
) -> bool {
    match expr {
        Expr::BinOp(ast::ExprBinOp {
            left, right, op, ..
        }) => {
            match op {
                Operator::BitOr => {
                    if target_version < PythonVersion::PY310 {
                        return true;
                    }
                    quotes_are_unremovable(semantic, left, target_version)
                        || quotes_are_unremovable(semantic, right, target_version)
                }
                // for now we'll treat uses of other operators as unremovable quotes
                // since that would make it an invalid type expression anyways. We skip
                // walking the nested non-type expressions from `typing.Annotated`, so
                // we don't produce false negatives in this branch.
                _ => true,
            }
        }
        Expr::Starred(ast::ExprStarred {
            value,
            ctx: ExprContext::Load,
            ..
        }) => quotes_are_unremovable(semantic, value, target_version),
        Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
            // for subscripts we don't know whether it's safe to do at runtime
            // since the operation may only be available at type checking time.
            // E.g. stubs only generics.
            if !semantic.in_type_checking_block() {
                return true;
            }
            if quotes_are_unremovable(semantic, value, target_version) {
                return true;
            }
            // for `typing.Annotated`, only analyze the first argument, since the rest may
            // contain arbitrary expressions.
            if let Some(qualified_name) = semantic.resolve_qualified_name(value) {
                if semantic.match_typing_qualified_name(&qualified_name, "Annotated") {
                    if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice.as_ref() {
                        return !elts.is_empty()
                            && quotes_are_unremovable(semantic, &elts[0], target_version);
                    }
                    return false;
                }
            }
            quotes_are_unremovable(semantic, slice, target_version)
        }
        Expr::Attribute(ast::ExprAttribute { value, .. }) => {
            // for attributes we also don't know whether it's safe
            if !semantic.in_type_checking_block() {
                return true;
            }
            quotes_are_unremovable(semantic, value, target_version)
        }
        Expr::List(ast::ExprList { elts, .. }) | Expr::Tuple(ast::ExprTuple { elts, .. }) => {
            for elt in elts {
                if quotes_are_unremovable(semantic, elt, target_version) {
                    return true;
                }
            }
            false
        }
        Expr::Name(name) => {
            semantic.resolve_name(name).is_some()
                && semantic
                    .simulate_runtime_load(name, semantic.in_type_checking_block().into())
                    .is_none()
        }
        _ => false,
    }
}
