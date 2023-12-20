use anyhow::Result;

use ruff_diagnostics::Edit;
use ruff_python_ast::call_path::from_qualified_name;
use ruff_python_ast::helpers::map_callable;
use ruff_python_ast::{self as ast, Expr};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_semantic::{
    analyze, Binding, BindingKind, NodeId, ResolvedReference, SemanticModel,
};
use ruff_source_file::Locator;
use ruff_text_size::Ranged;

use crate::rules::flake8_type_checking::settings::Settings;

/// Returns `true` if the [`ResolvedReference`] is in a typing-only context _or_ a runtime-evaluated
/// context (with quoting enabled).
pub(crate) fn is_typing_reference(reference: &ResolvedReference, settings: &Settings) -> bool {
    reference.in_type_checking_block()
        || reference.in_typing_only_annotation()
        || reference.in_complex_string_type_definition()
        || reference.in_simple_string_type_definition()
        || (settings.quote_annotations && reference.in_runtime_evaluated_annotation())
}

/// Returns `true` if the [`Binding`] represents a runtime-required import.
pub(crate) fn is_valid_runtime_import(
    binding: &Binding,
    semantic: &SemanticModel,
    settings: &Settings,
) -> bool {
    if matches!(
        binding.kind,
        BindingKind::Import(..) | BindingKind::FromImport(..) | BindingKind::SubmoduleImport(..)
    ) {
        binding.context.is_runtime()
            && binding
                .references()
                .map(|reference_id| semantic.reference(reference_id))
                .any(|reference| !is_typing_reference(reference, settings))
    } else {
        false
    }
}

pub(crate) fn runtime_required_class(
    class_def: &ast::StmtClassDef,
    base_classes: &[String],
    decorators: &[String],
    semantic: &SemanticModel,
) -> bool {
    if runtime_required_base_class(class_def, base_classes, semantic) {
        return true;
    }
    if runtime_required_decorators(class_def, decorators, semantic) {
        return true;
    }
    false
}

/// Return `true` if a class is a subclass of a runtime-required base class.
fn runtime_required_base_class(
    class_def: &ast::StmtClassDef,
    base_classes: &[String],
    semantic: &SemanticModel,
) -> bool {
    analyze::class::any_over_body(class_def, semantic, &|call_path| {
        base_classes
            .iter()
            .any(|base_class| from_qualified_name(base_class) == call_path)
    })
}

fn runtime_required_decorators(
    class_def: &ast::StmtClassDef,
    decorators: &[String],
    semantic: &SemanticModel,
) -> bool {
    if decorators.is_empty() {
        return false;
    }

    class_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(map_callable(&decorator.expression))
            .is_some_and(|call_path| {
                decorators
                    .iter()
                    .any(|base_class| from_qualified_name(base_class) == call_path)
            })
    })
}

/// Returns `true` if a function is registered as a `singledispatch` interface.
///
/// For example, `fun` below is a `singledispatch` interface:
/// ```python
/// from functools import singledispatch
///
/// @singledispatch
/// def fun(arg, verbose=False):
///     ...
/// ```
pub(crate) fn is_singledispatch_interface(
    function_def: &ast::StmtFunctionDef,
    semantic: &SemanticModel,
) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        semantic
            .resolve_call_path(&decorator.expression)
            .is_some_and(|call_path| {
                matches!(call_path.as_slice(), ["functools", "singledispatch"])
            })
    })
}

/// Returns `true` if a function is registered as a `singledispatch` implementation.
///
/// For example, `_` below is a `singledispatch` implementation:
/// For example:
/// ```python
/// from functools import singledispatch
///
/// @singledispatch
/// def fun(arg, verbose=False):
///     ...
///
/// @fun.register
/// def _(arg: int, verbose=False):
///     ...
/// ```
pub(crate) fn is_singledispatch_implementation(
    function_def: &ast::StmtFunctionDef,
    semantic: &SemanticModel,
) -> bool {
    function_def.decorator_list.iter().any(|decorator| {
        let Expr::Attribute(attribute) = &decorator.expression else {
            return false;
        };

        if attribute.attr.as_str() != "register" {
            return false;
        };

        let Some(id) = semantic.lookup_attribute(attribute.value.as_ref()) else {
            return false;
        };

        let binding = semantic.binding(id);
        let Some(function_def) = binding
            .kind
            .as_function_definition()
            .map(|id| &semantic.scopes[*id])
            .and_then(|scope| scope.kind.as_function())
        else {
            return false;
        };

        is_singledispatch_interface(function_def, semantic)
    })
}

/// Wrap a type annotation in quotes.
///
/// This requires more than just wrapping the reference itself in quotes. For example:
/// - When quoting `Series` in `Series[pd.Timestamp]`, we want `"Series[pd.Timestamp]"`.
/// - When quoting `kubernetes` in `kubernetes.SecurityContext`, we want `"kubernetes.SecurityContext"`.
/// - When quoting `Series` in `Series["pd.Timestamp"]`, we want `"Series[pd.Timestamp]"`. (This is currently unsupported.)
/// - When quoting `Series` in `Series[Literal["pd.Timestamp"]]`, we want `"Series[Literal['pd.Timestamp']]"`. (This is currently unsupported.)
///
/// In general, when expanding a component of a call chain, we want to quote the entire call chain.
pub(crate) fn quote_annotation(
    node_id: NodeId,
    semantic: &SemanticModel,
    locator: &Locator,
    stylist: &Stylist,
    generator: Generator,
) -> Result<Edit> {
    let expr = semantic.expression(node_id).expect("Expression not found");
    if let Some(parent_id) = semantic.parent_expression_id(node_id) {
        match semantic.expression(parent_id) {
            Some(Expr::Subscript(parent)) => {
                if expr == parent.value.as_ref() {
                    // If we're quoting the value of a subscript, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `DataFrame[int]`, we
                    // should generate `"DataFrame[int]"`.
                    return quote_annotation(parent_id, semantic, locator, stylist, generator);
                }
            }
            Some(Expr::Attribute(parent)) => {
                if expr == parent.value.as_ref() {
                    // If we're quoting the value of an attribute, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `pd.DataFrame`, we
                    // should generate `"pd.DataFrame"`.
                    return quote_annotation(parent_id, semantic, locator, stylist, generator);
                }
            }
            Some(Expr::Call(parent)) => {
                if expr == parent.func.as_ref() {
                    // If we're quoting the function of a call, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `DataFrame()`, we
                    // should generate `"DataFrame()"`.
                    return quote_annotation(parent_id, semantic, locator, stylist, generator);
                }
            }
            Some(Expr::BinOp(parent)) => {
                if parent.op.is_bit_or() {
                    // If we're quoting the left or right side of a binary operation, we need to
                    // quote the entire expression. For example, when quoting `DataFrame` in
                    // `DataFrame | Series`, we should generate `"DataFrame | Series"`.
                    return quote_annotation(parent_id, semantic, locator, stylist, generator);
                }
            }
            _ => {}
        }
    }

    // If the annotation already contains a quote, avoid attempting to re-quote it. For example:
    // ```python
    // from typing import Literal
    //
    // Set[Literal["Foo"]]
    // ```
    let text = locator.slice(expr);
    if text.contains('\'') || text.contains('"') {
        return Err(anyhow::anyhow!("Annotation already contains a quote"));
    }

    // Quote the entire expression.
    let quote = stylist.quote();
    let annotation = generator.expr(expr);

    Ok(Edit::range_replacement(
        format!("{quote}{annotation}{quote}"),
        expr.range(),
    ))
}

/// Filter out any [`Edit`]s that are completely contained by any other [`Edit`].
pub(crate) fn filter_contained(edits: Vec<Edit>) -> Vec<Edit> {
    let mut filtered: Vec<Edit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if filtered
            .iter()
            .all(|filtered_edit| !filtered_edit.range().contains_range(edit.range()))
        {
            filtered.push(edit);
        }
    }
    filtered
}
