use anyhow::Result;
use ast::str::Quote;
use ast::visitor::source_order;
use ruff_python_ast::visitor::source_order::SourceOrderVisitor;
use std::cmp::Reverse;

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::{self as ast, Decorator, Expr};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_semantic::{
    analyze, Binding, BindingKind, Modules, NodeId, ResolvedReference, ScopeKind, SemanticModel,
};
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

/// Returns `true` if a function's parameters should be treated as runtime-required.
pub(crate) fn runtime_required_function(
    function_def: &ast::StmtFunctionDef,
    decorators: &[String],
    semantic: &SemanticModel,
) -> bool {
    if runtime_required_decorators(&function_def.decorator_list, decorators, semantic) {
        return true;
    }
    false
}

/// Returns `true` if a class's assignments should be treated as runtime-required.
pub(crate) fn runtime_required_class(
    class_def: &ast::StmtClassDef,
    base_classes: &[String],
    decorators: &[String],
    semantic: &SemanticModel,
) -> bool {
    if runtime_required_base_class(class_def, base_classes, semantic) {
        return true;
    }
    if runtime_required_decorators(&class_def.decorator_list, decorators, semantic) {
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
    analyze::class::any_qualified_name(class_def, semantic, &|qualified_name| {
        base_classes
            .iter()
            .any(|base_class| QualifiedName::from_dotted_name(base_class) == qualified_name)
    })
}

fn runtime_required_decorators(
    decorator_list: &[Decorator],
    decorators: &[String],
    semantic: &SemanticModel,
) -> bool {
    if decorators.is_empty() {
        return false;
    }

    decorator_list.iter().any(|decorator| {
        semantic
            .resolve_qualified_name(map_callable(&decorator.expression))
            .is_some_and(|qualified_name| {
                decorators
                    .iter()
                    .any(|base_class| QualifiedName::from_dotted_name(base_class) == qualified_name)
            })
    })
}

/// Returns `true` if an annotation will be inspected at runtime by the `dataclasses` module.
///
/// Specifically, detects whether an annotation is to either `dataclasses.InitVar` or
/// `typing.ClassVar` within a `@dataclass` class definition.
///
/// See: <https://docs.python.org/3/library/dataclasses.html#init-only-variables>
pub(crate) fn is_dataclass_meta_annotation(annotation: &Expr, semantic: &SemanticModel) -> bool {
    if !semantic.seen_module(Modules::DATACLASSES) {
        return false;
    }

    // Determine whether the assignment is in a `@dataclass` class definition.
    if let ScopeKind::Class(class_def) = semantic.current_scope().kind {
        if class_def.decorator_list.iter().any(|decorator| {
            semantic
                .resolve_qualified_name(map_callable(&decorator.expression))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["dataclasses", "dataclass"])
                })
        }) {
            // Determine whether the annotation is `typing.ClassVar` or `dataclasses.InitVar`.
            return semantic
                .resolve_qualified_name(map_subscript(annotation))
                .is_some_and(|qualified_name| {
                    matches!(qualified_name.segments(), ["dataclasses", "InitVar"])
                        || semantic.match_typing_qualified_name(&qualified_name, "ClassVar")
                });
        }
    }

    false
}

/// Returns `true` if a function is registered as a `singledispatch` interface.
///
/// For example, `fun` below is a `singledispatch` interface:
/// ```python
/// from functools import singledispatch
///
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
            .resolve_qualified_name(&decorator.expression)
            .is_some_and(|qualified_name| {
                matches!(qualified_name.segments(), ["functools", "singledispatch"])
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
                    return quote_annotation(parent_id, semantic, stylist, generator);
                }
            }
            Some(Expr::Attribute(parent)) => {
                if expr == parent.value.as_ref() {
                    // If we're quoting the value of an attribute, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `pd.DataFrame`, we
                    // should generate `"pd.DataFrame"`.
                    return quote_annotation(parent_id, semantic, stylist, generator);
                }
            }
            Some(Expr::Call(parent)) => {
                if expr == parent.func.as_ref() {
                    // If we're quoting the function of a call, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `DataFrame()`, we
                    // should generate `"DataFrame()"`.
                    return quote_annotation(parent_id, semantic, stylist, generator);
                }
            }
            Some(Expr::BinOp(parent)) => {
                if parent.op.is_bit_or() {
                    // If we're quoting the left or right side of a binary operation, we need to
                    // quote the entire expression. For example, when quoting `DataFrame` in
                    // `DataFrame | Series`, we should generate `"DataFrame | Series"`.
                    return quote_annotation(parent_id, semantic, stylist, generator);
                }
            }
            _ => {}
        }
    }

    let quote = stylist.quote();
    let mut quote_annotation = QuoteAnnotation::new(stylist);
    quote_annotation.visit_expr(&expr);

    let annotation = quote_annotation.annotation;

    dbg!(&annotation);
    Ok(Edit::range_replacement(
        format!("{quote}{annotation}{quote}"),
        expr.range(),
    ))
}

/// Filter out any [`Edit`]s that are completely contained by any other [`Edit`].
pub(crate) fn filter_contained(edits: Vec<Edit>) -> Vec<Edit> {
    let mut edits = edits;

    // Sort such that the largest edits are prioritized.
    edits.sort_unstable_by_key(|edit| (edit.start(), Reverse(edit.end())));

    // Remove any edits that are completely contained by another edit.
    let mut filtered: Vec<Edit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if !filtered
            .iter()
            .any(|filtered_edit| filtered_edit.range().contains_range(edit.range()))
        {
            filtered.push(edit);
        }
    }
    filtered
}

#[derive(Copy, PartialEq, Clone)]
enum State {
    Literal,
    Annotated,
    AnnotatedNonFirstElm,
    Other,
}

pub(crate) struct QuoteAnnotation<'a> {
    state: Vec<State>,
    stylist: &'a Stylist<'a>,
    annotation: String,
    final_quote_type: Quote,
}

impl<'a> QuoteAnnotation<'a> {
    pub(crate) fn new(stylist: &'a Stylist<'a>) -> Self {
        let final_quote_type = stylist.quote();
        Self {
            state: vec![],
            stylist,
            annotation: String::new(),
            final_quote_type,
        }
    }
}

impl<'a> source_order::SourceOrderVisitor<'a> for QuoteAnnotation<'a> {
    fn visit_expr(&mut self, expr: &'a Expr) {
        let generator = Generator::from(self.stylist);
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if let Some(name) = value.as_name_expr() {
                    let value = generator.expr(value);
                    self.annotation.push_str(&value);
                    self.annotation.push_str("[");
                    match name.id.as_str() {
                        "Literal" => self.state.push(State::Literal),
                        "Annotated" => self.state.push(State::Annotated),
                        _ => self.state.push(State::Other),
                    }

                    self.visit_expr(slice);
                    self.state.pop();
                    self.annotation.push_str(&format!("]"));
                }
            }
            Expr::Tuple(ast::ExprTuple { elts, .. }) => {
                let first_elm = elts.first().unwrap();
                self.visit_expr(first_elm);
                if self.state.last().copied() == Some(State::Annotated) {
                    self.state.push(State::AnnotatedNonFirstElm);
                }
                for elm in elts.iter().skip(1) {
                    self.annotation.push_str(", ");
                    self.visit_expr(elm);
                }
                self.state.pop();
            }
            Expr::BinOp(ast::ExprBinOp {
                left, op, right, ..
            }) => {
                self.visit_expr(left);
                self.annotation.push_str(&format!(" {op} "));
                self.visit_expr(right);
            }
            _ => {
                let source = match self.state.last().copied() {
                    Some(State::Literal | State::Annotated) => {
                        let mut source = generator.expr(expr);
                        source = source.replace(
                            self.final_quote_type.as_char(),
                            &self.final_quote_type.opposite().as_char().to_string(),
                        );
                        source
                    }
                    _ => {
                        let mut source = generator.expr(expr);
                        source = source.replace(self.final_quote_type.as_char(), "");
                        source = source.replace(self.final_quote_type.opposite().as_char(), "");
                        source
                    }
                };
                self.annotation.push_str(&source);
            }
        }
    }
}
