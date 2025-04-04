use std::cmp::Reverse;

use ruff_diagnostics::Edit;
use ruff_python_ast::helpers::{map_callable, map_subscript};
use ruff_python_ast::name::QualifiedName;
use ruff_python_ast::str::Quote;
use ruff_python_ast::visitor::transformer::{walk_expr, Transformer};
use ruff_python_ast::{self as ast, Decorator, Expr, StringLiteralFlags};
use ruff_python_codegen::{Generator, Stylist};
use ruff_python_parser::typing::parse_type_annotation;
use ruff_python_semantic::{
    analyze, Binding, BindingKind, Modules, NodeId, ResolvedReference, ScopeKind, SemanticModel,
};
use ruff_text_size::{Ranged, TextRange};

use crate::rules::flake8_type_checking::settings::Settings;
use crate::Locator;

/// Returns `true` if the [`ResolvedReference`] is in a typing-only context _or_ a runtime-evaluated
/// context (with quoting enabled).
pub(crate) fn is_typing_reference(reference: &ResolvedReference, settings: &Settings) -> bool {
    reference.in_type_checking_block()
        // if we're not in a type checking block, we necessarily need to be within a
        // type definition to be considered a typing reference
        || (reference.in_type_definition()
            && (reference.in_typing_only_annotation()
                || reference.in_string_type_definition()
                || (settings.quote_annotations && reference.in_runtime_evaluated_annotation())))
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
    analyze::class::any_qualified_base_class(class_def, semantic, &|qualified_name| {
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
        let expression = map_callable(&decorator.expression);
        semantic
            // First try to resolve the qualified name normally for cases like:
            // ```python
            // from mymodule import app
            //
            // @app.get(...)
            // def test(): ...
            //  ```
            .resolve_qualified_name(expression)
            // If we can't resolve the name, then try resolving the assignment
            // in order to support cases like:
            // ```python
            // from fastapi import FastAPI
            //
            // app = FastAPI()
            //
            // @app.get(...)
            // def test(): ...
            // ```
            .or_else(|| analyze::typing::resolve_assignment(expression, semantic))
            .is_some_and(|qualified_name| {
                decorators
                    .iter()
                    .any(|decorator| QualifiedName::from_dotted_name(decorator) == qualified_name)
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
            // Determine whether the annotation is `typing.ClassVar`, `dataclasses.InitVar`, or `dataclasses.KW_ONLY`.
            return semantic
                .resolve_qualified_name(map_subscript(annotation))
                .is_some_and(|qualified_name| {
                    matches!(
                        qualified_name.segments(),
                        ["dataclasses", "InitVar" | "KW_ONLY"]
                    ) || semantic.match_typing_qualified_name(&qualified_name, "ClassVar")
                });
        }
    }

    false
}

/// Returns `true` if a function is registered as a `singledispatch` or `singledispatchmethod` interface.
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
                matches!(
                    qualified_name.segments(),
                    ["functools", "singledispatch" | "singledispatchmethod"]
                )
            })
    })
}

/// Returns `true` if a function is registered as a `singledispatch` or `singledispatchmethod` implementation.
///
/// For example, `_` below is a `singledispatch` implementation:
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
        }

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
/// - When quoting `Series` in `Series["pd.Timestamp"]`, we want `"Series[pd.Timestamp]"`.
/// - When quoting `Series` in `Series[Literal["pd.Timestamp"]]`, we want `"Series[Literal['pd.Timestamp']]"`.
///
/// In general, when expanding a component of a call chain, we want to quote the entire call chain.
pub(crate) fn quote_annotation(
    node_id: NodeId,
    semantic: &SemanticModel,
    stylist: &Stylist,
    locator: &Locator,
    flags: StringLiteralFlags,
) -> Edit {
    let expr = semantic.expression(node_id).expect("Expression not found");
    if let Some(parent_id) = semantic.parent_expression_id(node_id) {
        match semantic.expression(parent_id) {
            Some(Expr::Subscript(parent)) => {
                if expr == parent.value.as_ref() {
                    // If we're quoting the value of a subscript, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `DataFrame[int]`, we
                    // should generate `"DataFrame[int]"`.
                    return quote_annotation(parent_id, semantic, stylist, locator, flags);
                }
            }
            Some(Expr::Attribute(parent)) => {
                if expr == parent.value.as_ref() {
                    // If we're quoting the value of an attribute, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `pd.DataFrame`, we
                    // should generate `"pd.DataFrame"`.
                    return quote_annotation(parent_id, semantic, stylist, locator, flags);
                }
            }
            Some(Expr::Call(parent)) => {
                if expr == parent.func.as_ref() {
                    // If we're quoting the function of a call, we need to quote the entire
                    // expression. For example, when quoting `DataFrame` in `DataFrame()`, we
                    // should generate `"DataFrame()"`.
                    return quote_annotation(parent_id, semantic, stylist, locator, flags);
                }
            }
            Some(Expr::BinOp(parent)) => {
                if parent.op.is_bit_or() {
                    // If we're quoting the left or right side of a binary operation, we need to
                    // quote the entire expression. For example, when quoting `DataFrame` in
                    // `DataFrame | Series`, we should generate `"DataFrame | Series"`.
                    return quote_annotation(parent_id, semantic, stylist, locator, flags);
                }
            }
            _ => {}
        }
    }

    quote_type_expression(expr, semantic, stylist, locator, flags)
}

/// Wrap a type expression in quotes.
///
/// This function assumes that the callee already expanded expression components
/// to the minimum acceptable range for quoting, i.e. the parent node may not be
/// a [`Expr::Subscript`], [`Expr::Attribute`], `[Expr::Call]` or `[Expr::BinOp]`.
///
/// In most cases you want to call [`quote_annotation`] instead, which provides
/// that guarantee by expanding the expression before calling into this function.
pub(crate) fn quote_type_expression(
    expr: &Expr,
    semantic: &SemanticModel,
    stylist: &Stylist,
    locator: &Locator,
    flags: StringLiteralFlags,
) -> Edit {
    // Quote the entire expression.
    let quote_annotator = QuoteAnnotator::new(semantic, stylist, locator, flags);

    Edit::range_replacement(quote_annotator.into_annotation(expr), expr.range())
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

pub(crate) struct QuoteAnnotator<'a> {
    semantic: &'a SemanticModel<'a>,
    stylist: &'a Stylist<'a>,
    locator: &'a Locator<'a>,
    flags: StringLiteralFlags,
}

impl<'a> QuoteAnnotator<'a> {
    fn new(
        semantic: &'a SemanticModel<'a>,
        stylist: &'a Stylist<'a>,
        locator: &'a Locator<'a>,
        flags: StringLiteralFlags,
    ) -> Self {
        Self {
            semantic,
            stylist,
            locator,
            flags,
        }
    }

    fn into_annotation(self, expr: &Expr) -> String {
        let mut expr_without_forward_references = expr.clone();
        self.visit_expr(&mut expr_without_forward_references);
        let generator = Generator::from(self.stylist);
        // we first generate the annotation with the inverse quote, so we can
        // generate the string literal with the preferred quote
        let subgenerator = Generator::new(self.stylist.indentation(), self.stylist.line_ending());
        let annotation = subgenerator.expr(&expr_without_forward_references);
        generator.expr(&Expr::from(ast::StringLiteral {
            range: TextRange::default(),
            value: annotation.into_boxed_str(),
            flags: self.flags,
        }))
    }

    fn visit_annotated_slice(&self, slice: &mut Expr) {
        // we only want to walk the first tuple element if it exists,
        // anything else should not be transformed
        if let Expr::Tuple(ast::ExprTuple { elts, .. }) = slice {
            if !elts.is_empty() {
                self.visit_expr(&mut elts[0]);
                // The outer annotation will use the preferred quote.
                // As such, any quotes found in metadata elements inside an `Annotated` slice
                // should use the opposite quote to the preferred quote.
                for elt in elts.iter_mut().skip(1) {
                    QuoteRewriter::new(self.stylist).visit_expr(elt);
                }
            }
        }
    }
}

impl Transformer for QuoteAnnotator<'_> {
    fn visit_expr(&self, expr: &mut Expr) {
        match expr {
            Expr::Subscript(ast::ExprSubscript { value, slice, .. }) => {
                if let Some(qualified_name) = self.semantic.resolve_qualified_name(value) {
                    if self
                        .semantic
                        .match_typing_qualified_name(&qualified_name, "Literal")
                    {
                        // The outer annotation will use the preferred quote.
                        // As such, any quotes found inside a `Literal` slice
                        // should use the opposite quote to the preferred quote.
                        QuoteRewriter::new(self.stylist).visit_expr(slice);
                    } else if self
                        .semantic
                        .match_typing_qualified_name(&qualified_name, "Annotated")
                    {
                        self.visit_annotated_slice(slice);
                    } else {
                        self.visit_expr(slice);
                    }
                }
            }
            Expr::StringLiteral(literal) => {
                // try to parse the forward reference and replace the string
                // literal node with the parsed expression, if we fail to
                // parse the forward reference, we just keep treating this
                // like a regular string literal
                if let Ok(annotation) = parse_type_annotation(literal, self.locator.contents()) {
                    *expr = annotation.expression().clone();
                    // we need to visit the parsed expression too
                    // since it may contain forward references itself
                    self.visit_expr(expr);
                }
            }
            _ => {
                walk_expr(self, expr);
            }
        }
    }
}

/// A [`Transformer`] struct that rewrites all strings in an expression
/// to use a specified quotation style
#[derive(Debug)]
struct QuoteRewriter {
    preferred_inner_quote: Quote,
}

impl QuoteRewriter {
    fn new(stylist: &Stylist) -> Self {
        Self {
            preferred_inner_quote: stylist.quote().opposite(),
        }
    }
}

impl Transformer for QuoteRewriter {
    fn visit_string_literal(&self, literal: &mut ast::StringLiteral) {
        literal.flags = literal.flags.with_quote_style(self.preferred_inner_quote);
    }
}
